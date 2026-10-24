// The 3D world: one scene that holds the terrain, sea, climate layers, every animal,
// Population-mode herds, the watched region and the selection (3D World SDD §4).
//
// World units are tiles: x runs east (0..w, wrapping), z runs south (0..h, row 0 is the
// northern edge), y is up. Animal positions from the engine are already in tile units,
// so a tile coordinate (tx, ty) is the scene point (tx, height, ty).

import * as THREE from "three";
import { mergeGeometries } from "three/examples/jsm/utils/BufferGeometryUtils.js";
import type { Hydrology } from "../engine/protocol";
import { ramp, type RGB } from "../util";

const HEAT_RAMP: RGB[] = [
  [30, 30, 80],
  [180, 40, 120],
  [255, 200, 60],
];

/** Height of land in tiles per metre of elevation (5,000 m ≈ 3.3 tiles). */
export const RELIEF = 1 / 1500;
/** The sea floor is flattened so oceans read as basins rather than canyons. */
const SEA_FLOOR = 0.12;
const FOV = 45;
const MIN_PITCH = (18 * Math.PI) / 180;
const MAX_PITCH = (88 * Math.PI) / 180;

export type Rect = [number, number, number, number];

export interface CameraState {
  /** Point looked at, in tiles; x is kept in [0, w). */
  tx: number;
  ty: number;
  distance: number;
  /** Rotation about the vertical axis; 0 looks north. */
  yaw: number;
  pitch: number;
}

/** A geometry made non-indexed with every vertex tagged as one body part (see the animal shader). */
function part(geo: THREE.BufferGeometry, id: number) {
  const g = geo.index ? geo.toNonIndexed() : geo;
  g.setAttribute("aPart", new THREE.Float32BufferAttribute(new Float32Array(g.attributes.position.count).fill(id), 1));
  return g;
}

// Body parts the animal shader treats differently.
const BODY = 0;
/** Tail plume, scaled by the ornament trait and drawn in the ornament colour. */
const ORNAMENT = 1;
/** Two diagonal leg pairs that swing in opposite phase when walking. */
const LEGS_A = 2;
const LEGS_B = 3;
/** Legs that stretch with leg length but do not swing (the simple body). */
const LEGS_STILL = 4;

/** A small four-legged body, about 1 unit long, standing on y = 0 and facing +x. */
function animalGeometry() {
  const parts: THREE.BufferGeometry[] = [];
  const body = new THREE.CapsuleGeometry(0.2, 0.45, 3, 8);
  body.rotateZ(Math.PI / 2);
  body.translate(0, 0.42, 0);
  parts.push(part(body, BODY));
  const head = new THREE.SphereGeometry(0.15, 8, 6);
  head.translate(0.45, 0.58, 0);
  parts.push(part(head, BODY));
  const tail = new THREE.ConeGeometry(0.11, 0.42, 6);
  tail.rotateZ(0.75);
  tail.translate(-0.55, 0.62, 0);
  parts.push(part(tail, ORNAMENT));
  for (const [lx, lz, id] of [
    [0.25, 0.11, LEGS_A],
    [0.25, -0.11, LEGS_B],
    [-0.25, 0.11, LEGS_B],
    [-0.25, -0.11, LEGS_A],
  ]) {
    const leg = new THREE.CylinderGeometry(0.045, 0.04, 0.3, 5);
    leg.translate(lx, 0.15, lz);
    parts.push(part(leg, id));
  }
  const merged = mergeGeometries(parts)!;
  merged.computeVertexNormals();
  return merged;
}

/** The same body as boxes (48 triangles), for figures too small on screen to show detail. */
function simpleAnimalGeometry() {
  const body = new THREE.BoxGeometry(0.85, 0.36, 0.36).translate(0, 0.42, 0);
  const head = new THREE.BoxGeometry(0.26, 0.26, 0.26).translate(0.45, 0.58, 0);
  const tail = new THREE.BoxGeometry(0.3, 0.14, 0.14).translate(-0.55, 0.62, 0);
  const legs = new THREE.BoxGeometry(0.62, 0.3, 0.26).translate(0, 0.15, 0);
  const merged = mergeGeometries([part(body, BODY), part(head, BODY), part(tail, ORNAMENT), part(legs, LEGS_STILL)])!;
  merged.computeVertexNormals();
  return merged;
}

/**
 * Bends the animal body per instance (`aShape`: leg length, ornament size, unused,
 * gait). Legs stretch and lift the body; the tail plume grows from the rump with the
 * ornament trait and takes the ornament colour; walking animals swing their legs.
 */
function shapeAnimals(mat: THREE.Material, time: { value: number }) {
  mat.onBeforeCompile = (sh) => {
    sh.uniforms.uTime = time;
    sh.vertexShader = sh.vertexShader
      .replace(
        "#include <common>",
        `#include <common>
attribute float aPart;
attribute vec4 aShape;
uniform float uTime;
varying float vOrnament;`,
      )
      .replace(
        "#include <begin_vertex>",
        `#include <begin_vertex>
{
  bool ornament = aPart > 0.5 && aPart < 1.5;
  if (ornament) transformed = vec3(-0.42, 0.5, 0.0) + (transformed - vec3(-0.42, 0.5, 0.0)) * aShape.y;
  if (aPart > 1.5) {
    if (aShape.w > 0.5 && aPart < 3.5) {
      float swing = sin(uTime * 11.0 + fract(aShape.w) * 6.2832) * 0.2 * (0.3 - transformed.y) / 0.3;
      transformed.x += aPart < 2.5 ? swing : -swing;
    }
    transformed.y *= aShape.x;
  } else {
    transformed.y += 0.3 * (aShape.x - 1.0);
  }
  vOrnament = ornament ? 1.0 : 0.0;
}`,
      );
    sh.fragmentShader = sh.fragmentShader
      .replace("#include <common>", "#include <common>\nvarying float vOrnament;")
      .replace("#include <color_fragment>", "#include <color_fragment>\ndiffuseColor.rgb = mix(diffuseColor.rgb, vec3(0.93, 0.5, 0.1), vOrnament);");
  };
}

/**
 * Wind in the plants (`aBend`: how far a vertex moves, growing with its height above the
 * base). Every plant leans and sways the same way, with gusts that roll across the land
 * from the west; neighbours sway nearly together. Runs on the wall clock, so the plants
 * move when the simulation is paused.
 */
function windPlants(mat: THREE.Material, time: { value: number }) {
  mat.onBeforeCompile = (sh) => {
    sh.uniforms.uTime = time;
    sh.vertexShader = sh.vertexShader
      .replace(
        "#include <common>",
        `#include <common>
attribute float aBend;
uniform float uTime;`,
      )
      .replace(
        "#include <begin_vertex>",
        `#include <begin_vertex>
#ifdef USE_INSTANCING
{
  vec2 at = instanceMatrix[3].xz;
  float gust = 0.55 + 0.45 * sin(uTime * 0.4 - at.x * 0.08 + sin(at.y * 0.05));
  float sway = 0.35 + sin(uTime * 1.6 - at.x * 0.5 + at.y * 0.3) * 0.7 + sin(uTime * 3.7 + at.x * 1.3 - at.y * 1.1) * 0.25;
  // Push in the scene's axes, carried back into the plant's own (it is turned and scaled).
  vec3 push = vec3(0.94, 0.0, 0.34) * (sway * gust * aBend * length(instanceMatrix[1].xyz));
  transformed += inverse(mat3(instanceMatrix)) * push;
}
#endif`,
      );
  };
}

/** Marks how far each vertex bends in the wind: `amount` at the top, 0 at the base, rising with height². */
function withBend(g: THREE.BufferGeometry, amount: number) {
  const pos = g.attributes.position;
  let top = 0;
  for (let i = 0; i < pos.count; i++) top = Math.max(top, pos.getY(i));
  const bend = new Float32Array(pos.count);
  for (let i = 0; i < pos.count; i++) bend[i] = amount * Math.max(0, pos.getY(i) / top) ** 2;
  g.setAttribute("aBend", new THREE.BufferAttribute(bend, 1));
  return g;
}

/** A white side-on animal silhouette, tinted per point for distant figures drawn as sprites. */
function spriteTexture() {
  const c = document.createElement("canvas");
  c.width = c.height = 64;
  const g = c.getContext("2d")!;
  g.fillStyle = "#fff";
  g.beginPath();
  g.ellipse(30, 30, 20, 10, 0, 0, Math.PI * 2);
  g.ellipse(52, 22, 8, 7, 0, 0, Math.PI * 2);
  g.fill();
  for (const x of [16, 22, 38, 44]) g.fillRect(x - 2.5, 34, 5, 18);
  const tex = new THREE.CanvasTexture(c);
  tex.colorSpace = THREE.SRGBColorSpace;
  return tex;
}

/** Many figures as camera-facing sprites: one vertex each instead of a mesh. */
class Sprites {
  readonly points: THREE.Points;
  private pos = new Float32Array(0);
  private col = new Float32Array(0);

  constructor(material: THREE.PointsMaterial) {
    this.points = new THREE.Points(new THREE.BufferGeometry(), material);
    this.points.frustumCulled = false;
    this.points.visible = false;
  }

  reserve(n: number) {
    if (this.pos.length >= n * 3) return;
    const cap = Math.ceil(n * 1.3);
    this.pos = new Float32Array(cap * 3);
    this.col = new Float32Array(cap * 3);
    const g = this.points.geometry;
    g.setAttribute("position", new THREE.BufferAttribute(this.pos, 3));
    g.setAttribute("color", new THREE.BufferAttribute(this.col, 3));
  }

  set(k: number, x: number, y: number, z: number, c: THREE.Color) {
    this.pos[3 * k] = x;
    this.pos[3 * k + 1] = y;
    this.pos[3 * k + 2] = z;
    this.col[3 * k] = c.r;
    this.col[3 * k + 1] = c.g;
    this.col[3 * k + 2] = c.b;
  }

  /** Shows the first `count` figures, each about `size` scene units long. */
  commit(count: number, size: number) {
    const g = this.points.geometry;
    this.points.visible = count > 0;
    if (!count) return;
    g.setDrawRange(0, count);
    g.attributes.position.needsUpdate = true;
    g.attributes.color.needsUpdate = true;
    // With size attenuation three.js leaves the field of view out of the point size; dividing
    // by tan(fov / 2) makes a sprite as long on screen as a body `size` long.
    (this.points.material as THREE.PointsMaterial).size = (size * 1.25) / Math.tan(((FOV / 2) * Math.PI) / 180);
  }

  hide() {
    this.points.visible = false;
  }

  dispose() {
    this.points.geometry.dispose();
  }
}

/** Scene height of an elevation (m): land to scale, the sea floor flattened. */
function toHeight(e: number) {
  return e >= 0 ? e * RELIEF : e * RELIEF * SEA_FLOOR;
}

/** Triangles from a flat position list, turned to face up and lit as if level. */
function flatGeometry(pos: number[]) {
  for (let t = 0; t < pos.length; t += 9) {
    // Front faces wind counter-clockwise seen from above: (b - a) × (c - a) points up.
    const cross = (pos[t + 5] - pos[t + 2]) * (pos[t + 6] - pos[t]) - (pos[t + 3] - pos[t]) * (pos[t + 8] - pos[t + 2]);
    if (cross < 0) {
      for (let k = 0; k < 3; k++) [pos[t + 3 + k], pos[t + 6 + k]] = [pos[t + 6 + k], pos[t + 3 + k]];
    }
  }
  const g = new THREE.BufferGeometry();
  g.setAttribute("position", new THREE.Float32BufferAttribute(pos, 3));
  const normals = new Float32Array(pos.length);
  for (let i = 1; i < normals.length; i += 3) normals[i] = 1;
  g.setAttribute("normal", new THREE.BufferAttribute(normals, 3));
  return g;
}

/** A geometry with one vertex colour throughout (multiplied by the instance colour). */
function tinted(geo: THREE.BufferGeometry, rgb: [number, number, number]) {
  const g = geo.index ? geo.toNonIndexed() : geo;
  const n = g.attributes.position.count;
  const c = new Float32Array(n * 3);
  for (let i = 0; i < n; i++) c.set(rgb, 3 * i);
  g.setAttribute("color", new THREE.BufferAttribute(c, 3));
  g.deleteAttribute("uv");
  return g;
}

const TRUNK: [number, number, number] = [0.45, 0.32, 0.22];

/** A conifer about 0.3 tiles tall: a cone on a short trunk (16 triangles). */
function coniferGeometry() {
  const trunk = tinted(new THREE.CylinderGeometry(0.012, 0.016, 0.06, 4, 1, true).translate(0, 0.03, 0), TRUNK);
  const crown = tinted(new THREE.ConeGeometry(0.07, 0.26, 6).translate(0, 0.18, 0), [1, 1, 1]);
  const g = mergeGeometries([trunk, crown])!;
  g.computeVertexNormals();
  return withBend(g, 0.012);
}

/** A broadleaf tree about 0.25 tiles tall: a round crown on a trunk (28 triangles). */
function broadleafGeometry() {
  const trunk = tinted(new THREE.CylinderGeometry(0.012, 0.018, 0.12, 4, 1, true).translate(0, 0.06, 0), TRUNK);
  const crown = tinted(new THREE.IcosahedronGeometry(0.08, 0).scale(1, 0.8, 1).translate(0, 0.17, 0), [1, 1, 1]);
  const g = mergeGeometries([trunk, crown])!;
  g.computeVertexNormals();
  return withBend(g, 0.014);
}

/** A tuft of grass: three thin blades (12 triangles). */
function grassGeometry() {
  const blades = [-0.6, 0, 0.6].map((a, k) =>
    tinted(new THREE.ConeGeometry(0.012, 0.07, 4, 1, true).rotateZ(a * 0.5).translate(Math.sin(a) * 0.02, 0.03, (k - 1) * 0.012), [1, 1, 1]),
  );
  const g = mergeGeometries(blades)!;
  g.computeVertexNormals();
  return withBend(g, 0.016);
}

/**
 * Plants per tile at full forage, by biome id (the engine's `Biome` order): conifers,
 * broadleaf trees, grass tufts. Fractions are drawn on some tiles only.
 */
const PLANTS: [number, number, number][] = [
  [0, 0, 0], // ocean
  [0, 0, 0], // lake
  [0, 0, 0], // ice
  [0, 0, 2], // tundra
  [3, 0, 1], // boreal forest
  [0, 0, 0.6], // cold desert
  [0, 0.1, 4], // temperate grassland
  [0.6, 2.4, 1], // temperate forest
  [3.5, 0.5, 1], // temperate rainforest
  [0, 0, 0.3], // hot desert
  [0, 0.6, 3], // savanna
  [0, 3, 1], // tropical seasonal forest
  [0, 4, 0.5], // tropical rainforest
];
const DRY_GRASS = new THREE.Color(0.72, 0.62, 0.36);
const LUSH_GRASS = new THREE.Color(0.3, 0.56, 0.18);
const CONIFER = new THREE.Color(0.16, 0.36, 0.2);
const BROADLEAF = new THREE.Color(0.25, 0.5, 0.18);
const SPRING_LEAF = new THREE.Color(0.48, 0.66, 0.26);
const AUTUMN_GOLD = new THREE.Color(0.86, 0.62, 0.14);
const AUTUMN_RED = new THREE.Color(0.72, 0.24, 0.1);
/** Bare branches, seen from afar as a grey-brown crown. */
const BARE = new THREE.Color(0.42, 0.36, 0.3);
const SNOW = new THREE.Color(0.95, 0.97, 1);
/** Biomes whose broadleaf trees drop their leaves in winter (temperate grassland, forest, rainforest). */
const DECIDUOUS = new Set([6, 7, 8]);
/** Seasons in the plants are laid out again at most this often (ms) while the days move on. */
const SEASON_MS = 250;
/** Plants are drawn when a tile is at least this many pixels across. */
const PLANT_PX = 10;
/** Plants are laid out over a square this many tiles from the camera target (at most). */
const PLANT_RADIUS = 70;
const PLANT_MAX = 80_000;
/** Sun shadow map size (texels per side). */
const SHADOW_MAP = 2048;

/** A number in [0, 1) from an integer, for placing plants the same way every time. */
const hash01 = (i: number) => ((Math.imul(i ^ (i >>> 15), 2246822519) ^ Math.imul(i, 3266489917)) >>> 0) / 4294967296;

/**
 * Broadleaf crown colour for a day of the year counted as in the northern hemisphere
 * (day 172 is midsummer): green through summer, gold or red in autumn, bare in winter,
 * light green as the leaves come out in spring. `r` (0–1) varies trees: some turn
 * red rather than gold, and not all on the same day.
 */
function leafColor(day: number, r: number, out: THREE.Color) {
  const d = (((day - 8 * (r - 0.5)) % 365) + 365) % 365;
  const t = (a: number, b: number) => THREE.MathUtils.smoothstep(d, a, b);
  const turned = r < 0.35 ? AUTUMN_RED : AUTUMN_GOLD;
  if (d < 105 || d >= 320) return out.copy(BARE);
  if (d < 150) return out.copy(BARE).lerp(SPRING_LEAF, t(105, 130)).lerp(BROADLEAF, t(130, 150));
  if (d < 255) return out.copy(BROADLEAF);
  if (d < 290) return out.copy(BROADLEAF).lerp(turned, t(255, 285));
  return out.copy(turned).lerp(BARE, t(295, 320));
}

/** Below this on-screen body length (px) animals use the simple geometry. */
const DETAIL_PX = 16;
/** Above this many figures, small ones are drawn as sprites rather than meshes. */
const SPRITE_MIN = 20_000;
/** Animals are moved smoothly between frames up to this many (the engine's individual cap). */
const ANIMATE_MAX = 25_000;
/** A move longer than this (tiles) between frames is a jump, not a walk. */
const MAX_STEP = 3;

/** Hash an index to an angle so a herd does not all face the same way. */
const yawOf = (i: number) => (((i * 2654435761) >>> 0) / 4294967296) * Math.PI * 2;

interface AnimalFrame {
  pos: Float32Array;
  species: Uint32Array;
  actions?: Uint8Array;
  ids?: Float64Array;
  shape?: Float32Array;
  colors: Map<number, RGB>;
  /** Where each animal starts its current move (unwrapped, so the move is short). */
  fromX: Float32Array;
  fromY: Float32Array;
  /** Facing angle about the vertical axis. */
  heading: Float32Array;
  /** Until when (ms) each animal counts as walking. */
  walkUntil: Float64Array;
  /** Row of each id, when animals are moved smoothly. */
  index: Map<number, number> | null;
  /** Body length relative to the frame's typical animal (from mass). */
  scale: Float32Array;
}

/** Where animal `i` is drawn `a` (0–1) of the way through its move (x unwrapped). */
function drawnAt(d: AnimalFrame, i: number, a: number): [number, number] {
  const x = d.pos[2 * i];
  const y = d.pos[2 * i + 1];
  return [d.fromX[i] + (x - d.fromX[i]) * a, d.fromY[i] + (y - d.fromY[i]) * a];
}

const DAY_SKY = new THREE.Color(0x9cc3e4);
const DUSK_SKY = new THREE.Color(0xd9906a);
const NIGHT_SKY = new THREE.Color(0x22345a);
/** The path of the inspected animal covers this many days. */
const PATH_DAYS = 1;
const PATH_MAX = 4096;

/**
 * Where the sun is, as east, north and up components of a unit vector, at a latitude
 * (degrees), solar declination (radians) and hour angle (radians, 0 at noon).
 */
function sunVector(lat: number, decl: number, hour: number) {
  const phi = (lat * Math.PI) / 180;
  return {
    east: -Math.cos(decl) * Math.sin(hour),
    north: Math.cos(phi) * Math.sin(decl) - Math.sin(phi) * Math.cos(decl) * Math.cos(hour),
    up: Math.sin(phi) * Math.sin(decl) + Math.cos(phi) * Math.cos(decl) * Math.cos(hour),
  };
}

export class World3D {
  readonly renderer: THREE.WebGLRenderer;
  readonly camera = new THREE.PerspectiveCamera(FOV, 1, 0.05, 4000);
  readonly cam: CameraState = { tx: 0, ty: 0, distance: 100, yaw: 0, pitch: 1 };

  private scene = new THREE.Scene();
  private raycaster = new THREE.Raycaster();
  private w = 1;
  private h = 1;
  /** Height in scene units at each tile centre. */
  private heights = new Float32Array(1);

  private terrainGeo: THREE.BufferGeometry | null = null;
  private terrainTex: THREE.CanvasTexture | null = null;
  private terrainMat = new THREE.MeshLambertMaterial({ color: 0xffffff });
  /** Three copies (west, centre, east) so the east–west wrap is seamless. */
  private terrain: THREE.Mesh[] = [];
  private sea: THREE.Mesh | null = null;

  private animalGeo = animalGeometry();
  private simpleGeo = simpleAnimalGeometry();
  /** Herds use their own copy: the animal geometries carry per-instance shape attributes. */
  private herdGeo = simpleAnimalGeometry();
  private time = { value: 0 };
  private animalMat = new THREE.MeshLambertMaterial({ color: 0xffffff });
  /** Casts the animals' shadows with the same bent legs and plume as their bodies. */
  private animalDepthMat = new THREE.MeshDepthMaterial({ depthPacking: THREE.RGBADepthPacking });
  private animals: THREE.InstancedMesh | null = null;
  private shapeAttr: THREE.InstancedBufferAttribute | null = null;
  private animalData: AnimalFrame | null = null;
  /** When the current moves started, and how long they take (the time between position updates). */
  private moveStart = 0;
  private moveMs = 100;
  private lastMoveAt = 0;
  private spriteTex = spriteTexture();
  private animalSprites = new Sprites(new THREE.PointsMaterial({ map: this.spriteTex, vertexColors: true, alphaTest: 0.5 }));
  private herdSprites = new Sprites(new THREE.PointsMaterial({ map: this.spriteTex, vertexColors: true, alphaTest: 0.5, transparent: true, opacity: 0.8 }));
  /** Animal the camera follows, and who to tell when it can no longer be found. */
  private followId: number | null = null;
  onFollowLost: (() => void) | null = null;

  /** Latitude (degrees) of the northern and southern edges, and the axial tilt. */
  private latN = 60;
  private latS = -60;
  private tilt = 23.44;
  private clock = { day: 0.5, speed: 0 };

  private biome = new Float32Array(1);
  private waterKind = new Float32Array(1);
  private forage: Float32Array | null = null;
  private snow: Float32Array | null = null;
  private plantsOn = true;
  private plantMat = new THREE.MeshLambertMaterial({ color: 0xffffff, vertexColors: true });
  /** Casts the plants' shadows swaying with them. */
  private plantDepthMat = new THREE.MeshDepthMaterial({ depthPacking: THREE.RGBADepthPacking });
  private plantGeos = [coniferGeometry(), broadleafGeometry(), grassGeometry()];
  private plants: THREE.InstancedMesh[] = [];
  /** Camera target, detail and day when plants were last laid out (NaN: lay out again). */
  private plantAt = { tx: NaN, ty: NaN, radius: 0, full: false, day: NaN, at: 0 };

  private path: { t: number; x: number; y: number }[] = [];
  private pathLine: THREE.Line;
  private pathPos = new Float32Array(PATH_MAX * 3);
  private pathCol = new Float32Array(PATH_MAX * 3);
  private days = 0;

  private hemi = new THREE.HemisphereLight(0xdfefff, 0x4a4030, 1.6);
  private sun = new THREE.DirectionalLight(0xfff4e0, 2.2);
  /** Unit vector towards the sun (scene axes). */
  private sunDir = new THREE.Vector3(0, 1, 0);
  private water: THREE.Mesh[] = [];
  private riverMat = new THREE.MeshLambertMaterial({ color: 0x3e7ab2, polygonOffset: true, polygonOffsetFactor: -4, polygonOffsetUnits: -4 });
  private lakeMat = new THREE.MeshLambertMaterial({
    color: 0x3a78b0,
    transparent: true,
    opacity: 0.75,
    depthWrite: false,
    polygonOffset: true,
    polygonOffsetFactor: -2,
    polygonOffsetUnits: -2,
  });

  private barGeo = new THREE.BoxGeometry(0.8, 1, 0.8).translate(0, 0.5, 0);
  private barMat = new THREE.MeshLambertMaterial({ color: 0xffffff, transparent: true, opacity: 0.9 });
  private bars: THREE.InstancedMesh | null = null;
  /** Population-mode crowds: figures sampled from density, drawn translucent as approximate. */
  private herdMat = new THREE.MeshLambertMaterial({ color: 0xffffff, transparent: true, opacity: 0.8 });
  private herdData: { density: Float32Array; dominant: Uint32Array; colors: Map<number, RGB>; heat: boolean } | null = null;

  private focusBox: THREE.LineSegments;
  private regionBox: THREE.LineSegments;
  private focusRect: Rect | null = null;
  private selection: THREE.Mesh;
  private selected: { x: number; y: number } | null = null;
  private selectedId: number | null = null;

  private frameRequested = false;
  private lastSize = -1;
  /** Camera x when animals and herds were last laid out (they sit on the nearest copy). */
  private layoutTx = NaN;
  private disposed = false;
  /** Distance that shows the whole world; zooming out stops a little beyond it. */
  private fitDistance = 100;
  /** Until the user moves the camera, resizing reframes the whole world. */
  private userMoved = false;

  constructor(canvas: HTMLCanvasElement) {
    this.renderer = new THREE.WebGLRenderer({ canvas, antialias: true, preserveDrawingBuffer: true });
    this.renderer.setPixelRatio(Math.min(2, window.devicePixelRatio || 1));
    this.renderer.outputColorSpace = THREE.SRGBColorSpace;
    this.renderer.shadowMap.enabled = true;
    this.renderer.shadowMap.type = THREE.PCFSoftShadowMap;
    // The sun casts shadows over the ground around the camera target (see placeShadows).
    this.sun.castShadow = true;
    this.sun.shadow.mapSize.set(SHADOW_MAP, SHADOW_MAP);
    this.sun.shadow.bias = -0.0004;

    this.scene.background = DAY_SKY.clone();
    this.scene.fog = new THREE.Fog(DAY_SKY.clone(), 100, 400);
    this.scene.add(this.hemi, this.sun, this.sun.target, this.animalSprites.points, this.herdSprites.points);
    shapeAnimals(this.animalMat, this.time);
    shapeAnimals(this.animalDepthMat, this.time);
    windPlants(this.plantMat, this.time);
    windPlants(this.plantDepthMat, this.time);

    const boxGeo = new THREE.EdgesGeometry(new THREE.BoxGeometry(1, 1, 1).translate(0.5, 0.5, 0.5));
    this.focusBox = new THREE.LineSegments(boxGeo, new THREE.LineBasicMaterial({ color: 0xffffff, fog: false }));
    this.focusBox.visible = false;
    this.regionBox = new THREE.LineSegments(boxGeo, new THREE.LineBasicMaterial({ color: 0xffd54a, fog: false }));
    this.regionBox.visible = false;
    this.selection = new THREE.Mesh(
      new THREE.RingGeometry(0.75, 1, 32).rotateX(-Math.PI / 2),
      new THREE.MeshBasicMaterial({ color: 0xffd54a, side: THREE.DoubleSide, depthTest: false, transparent: true, fog: false }),
    );
    this.selection.renderOrder = 10;
    this.selection.visible = false;
    const pathGeo = new THREE.BufferGeometry();
    pathGeo.setAttribute("position", new THREE.BufferAttribute(this.pathPos, 3));
    pathGeo.setAttribute("color", new THREE.BufferAttribute(this.pathCol, 3));
    this.pathLine = new THREE.Line(pathGeo, new THREE.LineBasicMaterial({ vertexColors: true, fog: false, depthTest: false, transparent: true }));
    this.pathLine.renderOrder = 9;
    this.pathLine.frustumCulled = false;
    this.pathLine.visible = false;
    this.plants = this.plantGeos.map((g) => {
      const m = new THREE.InstancedMesh(g, this.plantMat, PLANT_MAX);
      m.count = 0;
      m.frustumCulled = false;
      m.castShadow = true;
      m.receiveShadow = true;
      m.customDepthMaterial = this.plantDepthMat;
      // Allocate instance colours up front so the material compiles with them.
      m.setColorAt(0, new THREE.Color(1, 1, 1));
      return m;
    });
    this.scene.add(this.focusBox, this.regionBox, this.selection, this.pathLine, ...this.plants);
  }

  // ---------------------------------------------------------------- world

  setWorld(
    w: number,
    h: number,
    elevation: Float32Array,
    water: Float32Array,
    hydro: Hydrology,
    biome: Float32Array,
    lat: { north: number; south: number; tilt: number },
  ) {
    this.w = w;
    this.h = h;
    this.biome = biome;
    this.waterKind = water;
    this.forage = null;
    this.plantAt.tx = NaN;
    this.latN = lat.north;
    this.latS = lat.south;
    this.tilt = lat.tilt;
    this.path = [];
    this.heights = new Float32Array(w * h);
    for (let i = 0; i < w * h; i++) this.heights[i] = toHeight(elevation[i]);
    for (const m of this.terrain) this.scene.remove(m);
    if (this.sea) this.scene.remove(this.sea);
    this.terrainGeo?.dispose();
    for (const m of this.water) m.geometry.dispose();

    // One vertex per tile centre, plus a column that repeats column 0 so copies join up.
    const cols = w + 1;
    const pos = new Float32Array(cols * h * 3);
    const uv = new Float32Array(cols * h * 2);
    for (let y = 0; y < h; y++) {
      for (let x = 0; x < cols; x++) {
        const k = y * cols + x;
        pos[3 * k] = x + 0.5;
        pos[3 * k + 1] = this.heights[y * w + (x % w)];
        pos[3 * k + 2] = y + 0.5;
        uv[2 * k] = (x + 0.5) / w;
        uv[2 * k + 1] = 1 - (y + 0.5) / h;
      }
    }
    const index: number[] = [];
    for (let y = 0; y < h - 1; y++) {
      for (let x = 0; x < w; x++) {
        const a = y * cols + x;
        const b = a + 1;
        const c = a + cols;
        const d = c + 1;
        index.push(a, c, b, b, c, d);
      }
    }
    const geo = new THREE.BufferGeometry();
    geo.setAttribute("position", new THREE.BufferAttribute(pos, 3));
    geo.setAttribute("uv", new THREE.BufferAttribute(uv, 2));
    geo.setIndex(index);
    geo.computeVertexNormals();
    geo.computeBoundingSphere();
    this.terrainGeo = geo;
    this.water = [new THREE.Mesh(this.riverGeometry(water, hydro), this.riverMat), new THREE.Mesh(this.lakeGeometry(water, hydro), this.lakeMat)];
    this.water[1].renderOrder = 2;
    this.terrain = [-1, 0, 1].map((k) => {
      const m = new THREE.Mesh(geo, this.terrainMat);
      m.position.x = k * w;
      // Mountains shade the valleys behind them when the sun is low.
      m.castShadow = true;
      m.receiveShadow = true;
      // Rivers and lakes ride on each copy; picking ignores them (it hits the terrain only).
      for (const wm of this.water) {
        const c = new THREE.Mesh(wm.geometry, wm.material);
        c.renderOrder = wm.renderOrder;
        m.add(c);
      }
      this.scene.add(m);
      return m;
    });

    const sea = new THREE.Mesh(
      // Extends past the northern and southern edges so the world sits in open sea.
      new THREE.PlaneGeometry(w * 3, h + 2 * w).rotateX(-Math.PI / 2).translate(w / 2, -0.02, h / 2),
      new THREE.MeshLambertMaterial({ color: 0x2f6aa0, transparent: true, opacity: 0.55, depthWrite: false }),
    );
    sea.renderOrder = 1;
    // Open sea beyond the northern and southern edges, at the depth and colour of the
    // terrain's deep ocean so the world's edges do not show.
    let sum = 0;
    let n = 0;
    for (const e of elevation) {
      if (e < 0) {
        sum += e;
        n++;
      }
    }
    const meanDepth = n ? sum / n : -3000;
    // Same ramp as the terrain image's open water.
    const col = ramp([[64, 120, 170], [22, 52, 98]], Math.min(1, -meanDepth / 4000));
    const floorMat = new THREE.MeshLambertMaterial({ color: new THREE.Color().setRGB(col[0] / 255, col[1] / 255, col[2] / 255, THREE.SRGBColorSpace) });
    const depth = meanDepth * RELIEF * SEA_FLOOR;
    for (const [z0, z1] of [
      [-w, 0.5],
      [h - 0.5, h + w],
    ]) {
      const strip = new THREE.Mesh(new THREE.PlaneGeometry(w * 3, z1 - z0).rotateX(-Math.PI / 2).translate(w / 2, depth, (z0 + z1) / 2), floorMat);
      sea.add(strip);
    }
    this.sea = sea;
    this.scene.add(sea);

    this.fit();
    this.setFocus(this.focusRect);
  }

  /**
   * Rivers as ribbons from each river tile's centre to the tile it drains into, wider
   * where more water flows, laid just above the ground.
   */
  private riverGeometry(water: Float32Array, hydro: Hydrology) {
    const { w } = this;
    let minFlow = Infinity;
    for (let i = 0; i < water.length; i++) if (water[i] === 3) minFlow = Math.min(minFlow, hydro.flow[i]);
    const pos: number[] = [];
    const STEPS = 4;
    for (let i = 0; i < water.length; i++) {
      const j = hydro.downstream[i];
      if (water[i] !== 3 || j < 0) continue;
      const x0 = (i % w) + 0.5;
      const y0 = Math.floor(i / w) + 0.5;
      let dx = (j % w) + 0.5 - x0;
      dx -= Math.round(dx / w) * w;
      const dy = Math.floor(j / w) + 0.5 - y0;
      const len = Math.hypot(dx, dy) || 1;
      const half = Math.min(0.3, 0.05 + 0.045 * Math.log2(hydro.flow[i] / minFlow + 1));
      // Overlap the next segment by half a width so bends join up.
      const ux = dx / len;
      const uy = dy / len;
      const sx = x0 - ux * half;
      const sy = y0 - uy * half;
      const ex = dx + 2 * ux * half;
      const ey = dy + 2 * uy * half;
      const nx = -uy * half;
      const ny = ux * half;
      const edge = (t: number, side: number): [number, number, number] => {
        const px = sx + ex * t + nx * side;
        const py = sy + ey * t + ny * side;
        const ground = Math.max(this.heightAt(px, py), this.heightAt(sx + ex * t, sy + ey * t));
        return [px, ground + 0.03, py];
      };
      for (let k = 0; k < STEPS; k++) {
        const a = edge(k / STEPS, -1);
        const b = edge(k / STEPS, 1);
        const c = edge((k + 1) / STEPS, -1);
        const d = edge((k + 1) / STEPS, 1);
        pos.push(...a, ...c, ...b, ...b, ...c, ...d);
      }
    }
    return flatGeometry(pos);
  }

  /** Lake surfaces: flat at each lake's level over every grid cell that touches the lake. */
  private lakeGeometry(water: Float32Array, hydro: Hydrology) {
    const { w, h } = this;
    const pos: number[] = [];
    for (let y = 0; y < h - 1; y++) {
      for (let x = 0; x < w; x++) {
        let level = -Infinity;
        for (const [cx, cy] of [
          [x, y],
          [(x + 1) % w, y],
          [x, y + 1],
          [(x + 1) % w, y + 1],
        ]) {
          const i = cy * w + cx;
          if (water[i] === 2) level = Math.max(level, hydro.level[i]);
        }
        if (level === -Infinity) continue;
        const ly = toHeight(level) + 0.01;
        const [x0, x1, z0, z1] = [x + 0.5, x + 1.5, y + 0.5, y + 1.5];
        pos.push(x0, ly, z0, x0, ly, z1, x1, ly, z0, x1, ly, z0, x0, ly, z1, x1, ly, z1);
      }
    }
    return flatGeometry(pos);
  }

  /**
   * Time of day and season from the engine's clock (`day`: day of the year with the
   * time of day as the fraction; `days`: days since the start). The light is worked out
   * when drawing, for the latitude the camera looks at.
   */
  setClock(day: number, speed: number, days = 0) {
    this.clock.day = day;
    this.clock.speed = speed;
    if (days < this.days) this.path = [];
    this.days = days;
    this.requestRender();
  }

  /**
   * Sun and sky for the season and the latitude at the camera target: the sun is higher
   * in summer and lower in winter, stays up at midsummer and below the horizon at
   * midwinter beyond the polar circles. Night falls only when time runs slowly enough to
   * see it (up to about an hour per second, fading out by six); faster, the light is the
   * day's mid-morning sun, dimmed where the sun barely rises.
   */
  private applyLight() {
    const { day, speed } = this.clock;
    const strength = speed <= 3600 ? 1 : speed >= 6 * 3600 ? 0 : 1 - Math.log(speed / 3600) / Math.log(6);
    const lat = this.latN - (Math.max(0, Math.min(this.h, this.cam.ty)) / this.h) * (this.latN - this.latS);
    // Solar declination: furthest north at the June solstice (day 172).
    const decl = ((this.tilt * Math.PI) / 180) * Math.cos((2 * Math.PI * (day - 172)) / 365.25);
    const now = sunVector(lat, decl, (day - Math.floor(day) - 0.5) * 2 * Math.PI);
    const morning = sunVector(lat, decl, -Math.PI / 3);
    const s = {
      east: morning.east + (now.east - morning.east) * strength,
      north: morning.north + (now.north - morning.north) * strength,
      up: morning.up + (now.up - morning.up) * strength,
    };
    const daylight = THREE.MathUtils.smoothstep(s.up, -0.1, 0.25);
    const dusk = Math.exp(-((s.up / 0.15) ** 2)) * strength;
    const fast = 0.4 + 0.6 * THREE.MathUtils.smoothstep(morning.up, -0.05, 0.35);
    const light = fast + (daylight - fast) * strength;
    this.sun.intensity = 0.25 + 1.95 * light;
    this.hemi.intensity = 0.75 + 0.85 * light;
    // Scene x is east and z is south. At night the faint light comes from high up, like moonlight.
    this.sunDir.set(s.east * daylight, Math.max(0.08, s.up, 1 - daylight), -s.north * daylight).normalize();
    // Shadows fade out with the light: the faint night light casts none.
    this.sun.shadow.intensity = THREE.MathUtils.smoothstep(light, 0.2, 0.6);
    // Warm at dusk, cool blue at night.
    const night = 1 - light;
    this.sun.color.setRGB(1 - 0.45 * night, 0.96 - 0.25 * dusk - 0.3 * night, 0.88 - 0.4 * dusk + 0.12 * night);
    const sky = NIGHT_SKY.clone().lerp(DAY_SKY, light).lerp(DUSK_SKY, dusk * 0.6);
    (this.scene.background as THREE.Color).copy(sky);
    (this.scene.fog as THREE.Fog).color.copy(sky);
  }

  /** Colours draped over the terrain: one pixel per tile (biomes, a climate layer, ranges). */
  setTexture(canvas: HTMLCanvasElement) {
    if (!this.terrainTex || this.terrainTex.image !== canvas) {
      this.terrainTex?.dispose();
      const tex = new THREE.CanvasTexture(canvas);
      tex.colorSpace = THREE.SRGBColorSpace;
      tex.wrapS = THREE.RepeatWrapping;
      tex.magFilter = THREE.LinearFilter;
      tex.minFilter = THREE.LinearFilter;
      tex.generateMipmaps = false;
      this.terrainTex = tex;
      this.terrainMat.map = tex;
      this.terrainMat.needsUpdate = true;
    } else {
      this.terrainTex.needsUpdate = true;
    }
    this.requestRender();
  }

  /** Terrain height (scene units) at a tile coordinate, bilinear between tile centres. */
  heightAt(tx: number, ty: number) {
    const { w, h } = this;
    const fx = tx - 0.5;
    const fy = Math.max(0, Math.min(h - 1, ty - 0.5));
    const x0 = Math.floor(fx);
    const y0 = Math.floor(fy);
    const ax = fx - x0;
    const ay = fy - y0;
    const y1 = Math.min(h - 1, y0 + 1);
    const c0 = ((x0 % w) + w) % w;
    const c1 = (c0 + 1) % w;
    const hs = this.heights;
    const top = hs[y0 * w + c0] * (1 - ax) + hs[y0 * w + c1] * ax;
    const bottom = hs[y1 * w + c0] * (1 - ax) + hs[y1 * w + c1] * ax;
    return Math.max(0, top * (1 - ay) + bottom * ay);
  }

  // ---------------------------------------------------------------- life

  /** How far the current moves have got (0–1). */
  private moveProgress(now: number) {
    return Math.min(1, Math.max(0, (now - this.moveStart) / this.moveMs));
  }

  /** Row of an animal id in the current frame, or -1. */
  private rowOf(id: number) {
    const d = this.animalData;
    if (!d?.ids) return -1;
    if (d.index) return d.index.get(id) ?? -1;
    return d.ids.indexOf(id);
  }

  /**
   * The animals simulated one by one. With ids, each animal walks smoothly from where it
   * is drawn now to its new position over the time between position updates, and faces
   * the way it walks.
   */
  setAnimals(
    pos: Float32Array | undefined,
    species: Uint32Array | undefined,
    actions: Uint8Array | undefined,
    ids: Float64Array | undefined,
    shape: Float32Array | undefined,
    colors: Map<number, RGB>,
  ) {
    const prev = this.animalData;
    if (!pos || !species) {
      this.animalData = null;
      this.lastSize = -1;
      this.requestRender();
      return;
    }
    const now = performance.now();
    const n = species.length;
    const a = this.moveProgress(now);
    const smooth = !!ids && n <= ANIMATE_MAX;
    const d: AnimalFrame = {
      pos,
      species,
      actions,
      ids,
      shape,
      colors,
      fromX: new Float32Array(n),
      fromY: new Float32Array(n),
      heading: new Float32Array(n),
      walkUntil: new Float64Array(n),
      index: smooth ? new Map() : null,
      scale: new Float32Array(n).fill(1),
    };
    let changed = !prev || prev.species.length !== n;
    for (let i = 0; i < n; i++) {
      const x = pos[2 * i];
      const y = pos[2 * i + 1];
      d.fromX[i] = x;
      d.fromY[i] = y;
      d.heading[i] = yawOf(ids ? ids[i] : i);
      if (!smooth) continue;
      const id = ids![i];
      d.index!.set(id, i);
      const j = prev?.index?.get(id);
      if (j === undefined) {
        changed = true;
        continue;
      }
      d.heading[i] = prev!.heading[j];
      d.walkUntil[i] = prev!.walkUntil[j];
      const [cx, cy] = drawnAt(prev!, j, a);
      let dx = x - cx;
      dx -= Math.round(dx / this.w) * this.w;
      const dy = y - cy;
      const step = Math.abs(dx) + Math.abs(dy);
      if (step > MAX_STEP) continue;
      d.fromX[i] = x - dx;
      d.fromY[i] = y - dy;
      if (prev!.pos[2 * j] !== x || prev!.pos[2 * j + 1] !== y) {
        changed = true;
        if (step > 1e-4) {
          d.heading[i] = Math.atan2(-dy, dx);
          d.walkUntil[i] = now + Math.max(600, this.moveMs * 1.5);
        }
      }
      if (actions?.[i] === 0) d.walkUntil[i] = 0;
    }
    if (smooth && prev?.index && !changed) {
      // Nothing moved (paused, or between engine steps): let the current moves finish.
      d.fromX = prev.fromX;
      d.fromY = prev.fromY;
      d.heading = prev.heading;
      d.walkUntil = prev.walkUntil;
    } else if (changed) {
      // Moves last as long as the time between position updates, so walking looks continuous.
      if (this.lastMoveAt) this.moveMs += (Math.min(1000, Math.max(30, now - this.lastMoveAt)) - this.moveMs) * 0.5;
      this.lastMoveAt = now;
      this.moveStart = now;
    }
    if (shape) {
      // Body length from mass, relative to the frame's typical animal (geometric mean).
      let logSum = 0;
      for (let i = 0; i < n; i++) logSum += Math.log(Math.max(1e-3, shape[4 * i]));
      const ref = Math.exp(logSum / Math.max(1, n));
      for (let i = 0; i < n; i++) d.scale[i] = Math.min(1.6, Math.max(0.7, Math.cbrt(Math.max(1e-3, shape[4 * i]) / ref)));
    }
    this.animalData = d;
    this.recordPath();
    this.lastSize = -1;
    this.requestRender();
  }

  /** Add the inspected animal's reported position to its path, and drop what is over a day old. */
  private recordPath() {
    const row = this.selectedId !== null ? this.rowOf(this.selectedId) : -1;
    const path = this.path;
    if (row >= 0) {
      const d = this.animalData!;
      let x = d.pos[2 * row];
      const y = d.pos[2 * row + 1];
      const last = path[path.length - 1];
      if (last) x -= Math.round((x - last.x) / this.w) * this.w;
      if (!last || Math.abs(x - last.x) + Math.abs(y - last.y) > 0.01) path.push({ t: this.days, x, y });
      else last.t = this.days;
    }
    let drop = 0;
    while (drop < path.length - 1 && path[drop].t < this.days - PATH_DAYS) drop++;
    if (path.length - drop > PATH_MAX - 1) drop = path.length - (PATH_MAX - 1);
    if (drop) path.splice(0, drop);
  }

  /**
   * Forage per tile (fraction of its maximum): plants are thinned where it has been
   * eaten. Snow cover (0–1) whitens trees and buries grass.
   */
  setPlants(forage: Float32Array | undefined, snow: Float32Array | undefined, visible: boolean) {
    if (forage && forage !== this.forage) {
      this.forage = forage;
      this.plantAt.tx = NaN;
    }
    if (snow && snow !== this.snow) {
      // Arrives every simulated day: laid out with the season, at most every SEASON_MS.
      this.snow = snow;
      this.plantAt.day = NaN;
    }
    if (visible !== this.plantsOn) {
      this.plantsOn = visible;
      this.plantAt.tx = NaN;
    }
    this.requestRender();
  }

  /**
   * Trees and grass around the camera target when tiles are large enough on screen:
   * forest biomes get conifers or broadleaf trees, open land grass tufts. Grass is drawn
   * in proportion to forage and dries from green to straw as it is grazed; trees thin
   * more slowly (browsing takes the understory first). Temperate broadleaf trees turn
   * and drop their leaves with the season of their hemisphere; snow whitens trees and
   * buries grass.
   */
  private layoutPlants(tilePx: number, now: number) {
    const on = this.plantsOn && !!this.forage && tilePx >= PLANT_PX;
    const radius = on ? Math.min(PLANT_RADIUS, Math.ceil(this.cam.distance * 1.3), Math.floor(this.w / 2)) : 0;
    const full = tilePx >= PLANT_PX * 3;
    const day = Math.floor(this.clock.day);
    const at = this.plantAt;
    if (!on && at.radius === 0 && !Number.isNaN(at.tx)) return;
    const near = Math.abs(this.cam.tx - at.tx) < radius / 4 && Math.abs(this.cam.ty - at.ty) < radius / 4;
    const sameDay = day === at.day || now - at.at < SEASON_MS;
    if (near && sameDay && radius <= at.radius && radius >= at.radius * 0.6 && full === at.full) {
      // A new day still waiting for its turn: come back for it.
      if (day !== at.day && on) this.requestRender();
      return;
    }
    at.tx = this.cam.tx;
    at.ty = this.cam.ty;
    at.radius = radius;
    at.full = full;
    at.day = day;
    at.at = now;
    const counts = [0, 0, 0];
    if (on) {
      const { w, h } = this;
      const forage = this.forage!;
      const snow = this.snow;
      // Far away, half the plants: they are small on screen and many.
      const keep = full ? 1 : 0.5;
      const m = new THREE.Matrix4();
      const q = new THREE.Quaternion();
      const sc = new THREE.Vector3();
      const p = new THREE.Vector3();
      const up = new THREE.Vector3(0, 1, 0);
      const c = new THREE.Color();
      const cx = Math.floor(this.cam.tx);
      const cy = Math.floor(this.cam.ty);
      for (let y = Math.max(0, cy - radius); y < Math.min(h, cy + radius); y++) {
        for (let ux = cx - radius; ux < cx + radius; ux++) {
          const x = ((ux % w) + w) % w;
          const i = y * w + x;
          const f = forage[i];
          if (f < 0 || this.waterKind[i] !== 0) continue;
          const kinds = PLANTS[this.biome[i]] ?? PLANTS[0];
          const white = snow ? Math.min(1, snow[i] ?? 0) : 0;
          // Day of the year as the northern hemisphere sees it: the south's seasons are half a year apart.
          const lat = this.latN - ((y + 0.5) / h) * (this.latN - this.latS);
          const leafDay = lat >= 0 ? day : day + 182;
          const deciduous = DECIDUOUS.has(this.biome[i]) && Math.abs(lat) > 23;
          for (let kind = 0; kind < 3; kind++) {
            const n = kinds[kind];
            if (!n) continue;
            // Grass grows with forage; trees keep at least half their number.
            // Deep snow buries the grass.
            const share = kind === 2 ? f * (1 - THREE.MathUtils.smoothstep(white, 0.3, 0.7)) : 0.5 + 0.5 * f;
            for (let j = 0; j < Math.ceil(n); j++) {
              const seed = (i * 16 + kind * 5 + j) | 0;
              if (hash01(seed) >= Math.min(1, n - j) * share * keep) continue;
              if (counts[kind] >= PLANT_MAX) continue;
              const px = ux + 0.1 + 0.8 * hash01(seed ^ 0x51ed);
              const py = y + 0.1 + 0.8 * hash01(seed ^ 0x9e37);
              const big = 0.75 + 0.5 * hash01(seed ^ 0x2c1b);
              q.setFromAxisAngle(up, hash01(seed ^ 0x7f4a) * Math.PI * 2);
              sc.set(big, kind === 2 ? 0.6 + 0.6 * f : big, big);
              p.set(px, this.heightAt(px, py) - 0.005, py);
              this.plants[kind].setMatrixAt(counts[kind], m.compose(p, q, sc));
              const r = hash01(seed ^ 0x33);
              if (kind === 2) c.copy(DRY_GRASS).lerp(LUSH_GRASS, f);
              else if (kind === 1 && deciduous) leafColor(leafDay, r, c);
              else c.copy(kind === 0 ? CONIFER : BROADLEAF);
              if (kind !== 2) c.multiplyScalar(0.85 + 0.3 * r);
              // Snow settles on the branches (less on bare ones, which hold little).
              c.lerp(SNOW, white * (kind === 2 ? 0.8 : 0.65));
              this.plants[kind].setColorAt(counts[kind]++, c);
            }
          }
        }
      }
    }
    this.plants.forEach((mesh, k) => {
      mesh.count = counts[k];
      mesh.instanceMatrix.needsUpdate = true;
      if (mesh.instanceColor) mesh.instanceColor.needsUpdate = true;
    });
  }

  /** The inspected animal's path over the last day, ending where it is drawn now. */
  private drawPath(sel: [number, number] | null) {
    const path = this.path;
    if (!sel || path.length < 1) {
      this.pathLine.visible = false;
      return;
    }
    const last = path[path.length - 1];
    // Draw on the world copy nearest the camera, joined to the animal as drawn.
    const endX = sel[0] - Math.round((sel[0] - last.x) / this.w) * this.w;
    const shift = this.nearest(endX) - endX;
    const n = path.length + 1;
    const t0 = path[0].t;
    const span = Math.max(1e-6, this.days - t0);
    for (let k = 0; k < n; k++) {
      const pt = k < path.length ? path[k] : { t: this.days, x: endX, y: sel[1] };
      const x = pt.x + shift;
      this.pathPos[3 * k] = x;
      this.pathPos[3 * k + 1] = this.heightAt(x, pt.y) + 0.03;
      this.pathPos[3 * k + 2] = pt.y;
      // Older stretches fade towards the ground colour.
      const a = 0.25 + 0.75 * Math.max(0, Math.min(1, (pt.t - t0) / span));
      this.pathCol[3 * k] = a;
      this.pathCol[3 * k + 1] = 0.84 * a;
      this.pathCol[3 * k + 2] = 0.29 * a;
    }
    const g = this.pathLine.geometry;
    g.setDrawRange(0, n);
    g.attributes.position.needsUpdate = true;
    g.attributes.color.needsUpdate = true;
    this.pathLine.visible = n >= 2;
  }

  /** Keep the camera on an animal (null stops); `onFollowLost` fires if it disappears. */
  follow(id: number | null) {
    this.followId = id;
    if (id !== null) this.userMoved = true;
    this.requestRender();
  }

  /**
   * Life not simulated one by one. On the density layer (heat) each occupied tile is a
   * column whose height is log density; otherwise it is a crowd of 1–4 figures in the
   * dominant species' colour.
   */
  setHerds(density: Float32Array | undefined, dominant: Uint32Array | undefined, colors: Map<number, RGB>, heat: boolean, visible: boolean) {
    this.herdData = visible && density && dominant ? { density, dominant, colors, heat } : null;
    this.lastSize = -1;
    this.requestRender();
  }

  private layoutHerds(size: number) {
    const d = this.herdData;
    if (!d) {
      if (this.bars) this.bars.count = 0;
      this.herdSprites.hide();
      return;
    }
    const { w, h } = this;
    let max = 0;
    let n = 0;
    for (let i = 0; i < d.density.length; i++) {
      if (d.density[i] > 0) n++;
      max = Math.max(max, d.density[i]);
    }
    if (n === 0) {
      if (this.bars) this.bars.count = 0;
      this.herdSprites.hide();
      return;
    }
    const lmax = Math.log1p(max) || 1;
    const per = d.heat ? 1 : 4;
    // Herds are many and approximate, so they use the simple body, or sprites when there are very many.
    const sprites = !d.heat && n * 2.5 > SPRITE_MIN;
    if (sprites) {
      if (this.bars) this.bars.count = 0;
      this.herdSprites.reserve(n * per);
    } else {
      this.herdSprites.hide();
    }
    const geo = d.heat ? this.barGeo : this.herdGeo;
    // Reuse the mesh while it fits: density arrives every frame and reallocating stalls rendering.
    let bars = this.bars;
    if (!sprites && (!bars || bars.geometry !== geo || bars.instanceMatrix.count < n * per)) {
      if (bars) {
        this.scene.remove(bars);
        bars.dispose();
      }
      bars = new THREE.InstancedMesh(geo, d.heat ? this.barMat : this.herdMat, Math.ceil(n * per * 1.3));
      bars.frustumCulled = false;
      this.bars = bars;
      this.scene.add(bars);
    }
    const m = new THREE.Matrix4();
    const q = new THREE.Quaternion();
    const sc = new THREE.Vector3();
    const p = new THREE.Vector3();
    const up = new THREE.Vector3(0, 1, 0);
    const c = new THREE.Color();
    const f = this.focusRect;
    const inFocus = (x: number, y: number) =>
      !!f && y >= f[1] && y < f[3] && ((x >= f[0] && x < f[2]) || (x + w >= f[0] && x + w < f[2]));
    let k = 0;
    for (let y = 0; y < h; y++) {
      for (let x = 0; x < w; x++) {
        const i = y * w + x;
        const v = d.density[i];
        if (v <= 0) continue;
        const t = Math.log1p(v) / lmax;
        const col: RGB = d.heat ? ramp(HEAT_RAMP, t) : d.colors.get(d.dominant[i]) ?? [255, 255, 255];
        c.setRGB(col[0] / 255, col[1] / 255, col[2] / 255, THREE.SRGBColorSpace);
        if (d.heat) {
          m.makeScale(1, 0.1 + 1.6 * t, 1).setPosition(this.nearest(x + 0.5), Math.max(0, this.heights[i]), y + 0.5);
          bars!.setMatrixAt(k, m);
          bars!.setColorAt(k++, c);
          continue;
        }
        // Animals inside the watched region are drawn individually.
        if (inFocus(x + 0.5, y + 0.5)) continue;
        const count = 1 + Math.floor(3 * t);
        for (let j = 0; j < count; j++) {
          const r = yawOf(i * 4 + j);
          const px = x + 0.5 + 0.35 * Math.cos(r * 1.7 + j);
          const py = y + 0.5 + 0.35 * Math.sin(r * 2.3 + j);
          if (sprites) {
            this.herdSprites.set(k++, this.nearest(px), this.heightAt(px, py) + size * 0.42, py, c);
            continue;
          }
          q.setFromAxisAngle(up, r);
          sc.setScalar(size);
          p.set(this.nearest(px), this.heightAt(px, py), py);
          bars!.setMatrixAt(k, m.compose(p, q, sc));
          bars!.setColorAt(k++, c);
        }
      }
    }
    if (sprites) {
      this.herdSprites.commit(k, size);
      return;
    }
    bars = bars!;
    bars.count = k;
    bars.instanceMatrix.needsUpdate = true;
    if (bars.instanceColor) bars.instanceColor.needsUpdate = true;
  }

  setFocus(rect: Rect | null) {
    if (rect?.join() !== this.focusRect?.join()) this.lastSize = -1;
    this.focusRect = rect;
    this.requestRender();
  }

  /** The box being dragged, in unwrapped scene coordinates. */
  setRegionPreview(rect: Rect | null) {
    this.placeBox(this.regionBox, rect, false);
    this.requestRender();
  }

  /** The inspected animal: its last reported position, and its id to track it smoothly. */
  setSelected(p: { x: number; y: number } | null, id: number | null = null) {
    const nextId = p ? id : null;
    if (nextId !== this.selectedId) this.path = [];
    this.selected = p;
    this.selectedId = nextId;
    if (this.path.length === 0) this.recordPath();
    this.requestRender();
  }

  private placeBox(box: THREE.LineSegments, rect: Rect | null, nearCamera: boolean) {
    if (!rect) {
      box.visible = false;
      return;
    }
    const [x0, y0, x1, y1] = rect;
    box.visible = true;
    box.position.set(nearCamera ? this.nearest(x0 + (x1 - x0) / 2) - (x1 - x0) / 2 : x0, -0.3, y0);
    box.scale.set(Math.max(0.01, x1 - x0), 4, Math.max(0.01, y1 - y0));
  }

  // ---------------------------------------------------------------- camera

  /** Look at the whole world from the south. */
  fit() {
    this.cam.tx = this.w / 2;
    this.cam.ty = this.h / 2;
    this.cam.yaw = 0;
    this.cam.pitch = (62 * Math.PI) / 180;
    // The nearest distance at which all four corners of the world are on screen.
    const corners = [
      [0, 0],
      [this.w, 0],
      [0, this.h],
      [this.w, this.h],
    ].map(([x, z]) => new THREE.Vector3(x, 0, z));
    const fits = (d: number) => {
      this.cam.distance = d;
      this.updateCamera();
      return corners.every((c) => {
        const v = c.clone().project(this.camera);
        return Math.abs(v.x) <= 0.96 && Math.abs(v.y) <= 0.96 && v.z < 1;
      });
    };
    let lo = 1;
    let hi = Math.max(this.w, this.h) * 4;
    for (let i = 0; i < 30; i++) {
      const mid = (lo + hi) / 2;
      if (fits(mid)) hi = mid;
      else lo = mid;
    }
    this.cam.distance = this.fitDistance = Math.max(4, hi);
    this.userMoved = false;
    this.requestRender();
  }

  /** x moved by whole world widths to the copy nearest the camera target. */
  private nearest(x: number) {
    return x - Math.round((x - this.cam.tx) / this.w) * this.w;
  }

  /** Scene units per screen pixel at the look-at distance. */
  unitsPerPixel() {
    const hpx = this.renderer.domElement.clientHeight || 1;
    return (2 * this.cam.distance * Math.tan(((FOV / 2) * Math.PI) / 180)) / hpx;
  }

  zoom(factor: number, toward: THREE.Vector3 | null) {
    const c = this.cam;
    this.userMoved = true;
    const maxDist = Math.max(Math.max(this.w, this.h) * 1.6, this.fitDistance * 1.3);
    const nd = Math.max(1.2, Math.min(maxDist, c.distance * factor));
    const f = 1 - nd / c.distance;
    if (toward) {
      c.tx += (toward.x - c.tx) * f;
      c.ty += (toward.z - c.ty) * f;
    }
    c.distance = nd;
    this.clampCamera();
  }

  pan(dx: number, dz: number) {
    this.userMoved = true;
    if (this.followId !== null) {
      this.followId = null;
      this.onFollowLost?.();
    }
    this.cam.tx += dx;
    this.cam.ty += dz;
    return this.clampCamera();
  }

  rotate(dyaw: number, dpitch: number) {
    this.userMoved = true;
    this.cam.yaw += dyaw;
    this.cam.pitch = Math.max(MIN_PITCH, Math.min(MAX_PITCH, this.cam.pitch + dpitch));
    this.requestRender();
  }

  /** Keep the target on the world; returns the east–west shift applied by wrapping. */
  private clampCamera() {
    const c = this.cam;
    const shift = Math.floor(c.tx / this.w) * this.w;
    c.tx -= shift;
    c.ty = Math.max(0, Math.min(this.h, c.ty));
    this.requestRender();
    return shift;
  }

  private updateCamera() {
    const c = this.cam;
    const ground = this.heightAt(c.tx, c.ty) * 0.5;
    const cp = Math.cos(c.pitch);
    this.camera.position.set(
      c.tx + c.distance * cp * Math.sin(c.yaw),
      ground + c.distance * Math.sin(c.pitch),
      c.ty + c.distance * cp * Math.cos(c.yaw),
    );
    this.camera.lookAt(c.tx, ground, c.ty);
    this.camera.near = Math.max(0.02, c.distance / 200);
    this.camera.far = c.distance * 8 + 200;
    this.camera.updateProjectionMatrix();
    const fog = this.scene.fog as THREE.Fog;
    fog.near = c.distance * 1.4;
    fog.far = c.distance * 4 + Math.max(this.w, this.h) * 0.6;
  }

  // ---------------------------------------------------------------- picking

  private ndc(clientX: number, clientY: number) {
    const r = this.renderer.domElement.getBoundingClientRect();
    return new THREE.Vector2(((clientX - r.left) / r.width) * 2 - 1, -((clientY - r.top) / r.height) * 2 + 1);
  }

  /** The terrain point under a screen position (unwrapped x), falling back to sea level. */
  groundAt(clientX: number, clientY: number): THREE.Vector3 | null {
    this.updateCamera();
    this.raycaster.setFromCamera(this.ndc(clientX, clientY), this.camera);
    const hit = this.raycaster.intersectObjects(this.terrain, false)[0];
    if (hit) return hit.point.clone();
    return this.planeAt(clientX, clientY);
  }

  /** Where a screen ray meets the horizontal plane y = level. */
  planeAt(clientX: number, clientY: number, level = 0): THREE.Vector3 | null {
    this.updateCamera();
    this.raycaster.setFromCamera(this.ndc(clientX, clientY), this.camera);
    const p = new THREE.Vector3();
    return this.raycaster.ray.intersectPlane(new THREE.Plane(new THREE.Vector3(0, 1, 0), -level), p);
  }

  /** Client (CSS pixel) position of a tile coordinate, on the copy nearest the camera. */
  toScreen(tx: number, ty: number): [number, number] {
    this.updateCamera();
    const x = this.nearest(tx);
    const v = new THREE.Vector3(x, this.heightAt(tx, ty), ty).project(this.camera);
    const r = this.renderer.domElement.getBoundingClientRect();
    return [r.left + ((v.x + 1) / 2) * r.width, r.top + ((1 - v.y) / 2) * r.height];
  }

  // ---------------------------------------------------------------- drawing

  /**
   * Fit the sun's shadow camera to the ground around the camera target: a square about
   * as wide as the view, so shadows stay sharp when zoomed in and cover the screen when
   * zoomed out. The light sits up-sun of the target.
   */
  private placeShadows() {
    const half = Math.min(Math.max(this.w, this.h) * 0.75, Math.max(4, this.cam.distance * 0.9));
    const target = new THREE.Vector3(this.cam.tx, this.heightAt(this.cam.tx, this.cam.ty), this.cam.ty);
    const reach = half * 2 + 20;
    this.sun.target.position.copy(target);
    this.sun.position.copy(target).addScaledVector(this.sunDir, reach);
    const sc = this.sun.shadow.camera;
    sc.left = sc.bottom = -half;
    sc.right = sc.top = half;
    sc.near = 1;
    sc.far = reach * 2;
    sc.updateProjectionMatrix();
    // Offset along the normal by about a shadow-map texel, which grows as the area does.
    this.sun.shadow.normalBias = ((2 * half) / SHADOW_MAP) * 1.5;
  }

  resize() {
    const el = this.renderer.domElement;
    const cw = el.clientWidth;
    const ch = el.clientHeight;
    if (cw === 0 || ch === 0) return;
    this.renderer.setSize(cw, ch, false);
    this.camera.aspect = cw / ch;
    if (!this.userMoved) this.fit();
    this.lastSize = -1;
    this.requestRender();
  }

  requestRender() {
    if (this.frameRequested || this.disposed) return;
    this.frameRequested = true;
    requestAnimationFrame(() => {
      this.frameRequested = false;
      if (!this.disposed) this.render();
    });
  }

  /** Lay out the animals: drawn on the world copy nearest the camera, sized to stay visible. */
  private layoutAnimals(size: number, detailed: boolean, now: number) {
    const d = this.animalData;
    if (!d) {
      if (this.animals) this.animals.count = 0;
      this.animalSprites.hide();
      return;
    }
    const n = d.species.length;
    const a = this.moveProgress(now);
    const c = new THREE.Color();
    const colorOf = (i: number) => {
      const col = d.colors.get(d.species[i]) ?? [255, 255, 255];
      c.setRGB(col[0] / 255, col[1] / 255, col[2] / 255, THREE.SRGBColorSpace);
      // Coat luminance lightens or darkens the species colour a little.
      if (d.shape) c.multiplyScalar(0.7 + 0.6 * d.shape[4 * i + 3]);
      return c;
    };
    if (n > SPRITE_MIN && !detailed) {
      if (this.animals) this.animals.count = 0;
      this.animalSprites.reserve(n);
      for (let i = 0; i < n; i++) {
        const [x, y] = drawnAt(d, i, a);
        this.animalSprites.set(i, this.nearest(x), this.heightAt(x, y) + size * d.scale[i] * 0.42, y, colorOf(i));
      }
      this.animalSprites.commit(n, size);
      return;
    }
    this.animalSprites.hide();
    if (!this.animals || this.animals.instanceMatrix.count < n) {
      if (this.animals) {
        this.scene.remove(this.animals);
        this.animals.dispose();
      }
      const cap = Math.max(256, Math.ceil(n * 1.3));
      this.shapeAttr = new THREE.InstancedBufferAttribute(new Float32Array(cap * 4), 4);
      this.animalGeo.setAttribute("aShape", this.shapeAttr);
      this.simpleGeo.setAttribute("aShape", this.shapeAttr);
      this.animals = new THREE.InstancedMesh(this.animalGeo, this.animalMat, cap);
      this.animals.frustumCulled = false;
      this.animals.receiveShadow = true;
      this.animals.customDepthMaterial = this.animalDepthMat;
      this.scene.add(this.animals);
    }
    const mesh = this.animals;
    const shape = this.shapeAttr!;
    const m = new THREE.Matrix4();
    const q = new THREE.Quaternion();
    const s = new THREE.Vector3();
    const p = new THREE.Vector3();
    const up = new THREE.Vector3(0, 1, 0);
    for (let i = 0; i < n; i++) {
      const [x, y] = drawnAt(d, i, a);
      const resting = d.actions?.[i] === 0;
      const k = size * d.scale[i];
      q.setFromAxisAngle(up, d.heading[i]);
      s.set(k, resting ? k * 0.55 : k, k);
      p.set(this.nearest(x), this.heightAt(x, y), y);
      mesh.setMatrixAt(i, m.compose(p, q, s));
      mesh.setColorAt(i, colorOf(i));
      // Leg length (tempered so extremes stay readable), ornament, and gait phase while walking.
      const legs = d.shape ? Math.min(1.6, Math.max(0.65, Math.sqrt(d.shape[4 * i + 1]))) : 1;
      const ornament = d.shape ? Math.min(1, d.shape[4 * i + 2] * 1.3) : 0.4;
      const gait = d.walkUntil[i] > now ? 1 + (((d.ids ? d.ids[i] : i) * 0.618) % 1) * 0.98 : 0;
      shape.setXYZW(i, legs, ornament, 0, gait);
    }
    mesh.count = n;
    mesh.instanceMatrix.needsUpdate = true;
    if (mesh.instanceColor) mesh.instanceColor.needsUpdate = true;
    shape.needsUpdate = true;
  }

  render() {
    const now = performance.now();
    this.time.value = now / 1000;
    const d = this.animalData;
    const a = this.moveProgress(now);
    const followRow = this.followId !== null ? this.rowOf(this.followId) : -1;
    if (this.followId !== null) {
      if (followRow < 0) {
        this.followId = null;
        this.onFollowLost?.();
      } else {
        const [x, y] = drawnAt(d!, followRow, a);
        this.cam.tx = x;
        this.cam.ty = y;
        this.cam.tx -= Math.floor(this.cam.tx / this.w) * this.w;
      }
    }
    this.updateCamera();
    this.applyLight();
    this.placeShadows();
    const upp = this.unitsPerPixel();
    this.layoutPlants(1 / upp, now);
    // Bodies are at least ~9 px long so a herd stays visible from far away.
    const size = Math.min(4, Math.max(0.12, 9 * upp));
    const detailed = size / upp >= DETAIL_PX;
    const moving = !!d?.index && a < 1;
    const moved = !(Math.abs(this.cam.tx - this.layoutTx) < this.w / 8);
    if (moved || this.lastSize < 0 || Math.abs(size - this.lastSize) > this.lastSize * 0.05) {
      this.layoutAnimals(size, detailed, now);
      this.layoutHerds(size);
      this.lastSize = size;
      this.layoutTx = this.cam.tx;
    } else if (moving) {
      this.layoutAnimals(this.lastSize, detailed, now);
    }
    if (this.animals) {
      this.animals.geometry = detailed ? this.animalGeo : this.simpleGeo;
      // Bodies are enlarged to stay visible when small on screen; only true-to-size ones cast shadows.
      this.animals.castShadow = detailed;
    }
    this.placeBox(this.focusBox, this.focusRect, true);
    const selRow = this.selectedId !== null ? this.rowOf(this.selectedId) : -1;
    const sel = selRow >= 0 ? drawnAt(d!, selRow, a) : this.selected ? [this.selected.x, this.selected.y] : null;
    if (sel) {
      const r = Math.max(0.25, 14 * upp);
      this.selection.visible = true;
      this.selection.position.set(this.nearest(sel[0]), this.heightAt(sel[0], sel[1]) + 0.02, sel[1]);
      this.selection.scale.setScalar(r);
    } else {
      this.selection.visible = false;
    }
    this.drawPath(sel as [number, number] | null);
    this.renderer.render(this.scene, this.camera);
    // Keep drawing while animals walk: positions ease between updates and legs swing.
    let walking = false;
    if (d && detailed && this.animals?.count) {
      for (let i = 0; i < d.walkUntil.length && !walking; i++) walking = d.walkUntil[i] > now;
    }
    // And while plants are shown: they sway in the wind.
    const plants = this.plants.some((m) => m.count > 0);
    if (moving || walking || plants) this.requestRender();
  }

  dispose() {
    this.disposed = true;
    this.terrainGeo?.dispose();
    for (const m of this.water) m.geometry.dispose();
    this.animalSprites.dispose();
    this.herdSprites.dispose();
    this.spriteTex.dispose();
    this.herdGeo.dispose();
    this.terrainTex?.dispose();
    this.animals?.dispose();
    this.animalGeo.dispose();
    this.simpleGeo.dispose();
    this.barGeo.dispose();
    this.bars?.dispose();
    for (const m of this.plants) m.dispose();
    for (const g of this.plantGeos) g.dispose();
    this.plantMat.dispose();
    this.plantDepthMat.dispose();
    this.animalMat.dispose();
    this.animalDepthMat.dispose();
    this.pathLine.geometry.dispose();
    this.renderer.dispose();
  }
}
