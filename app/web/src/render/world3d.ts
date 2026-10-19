// The 3D world: one scene that holds the terrain, sea, climate layers, every animal,
// Population-mode herds, the watched region and the selection (3D World SDD §4).
//
// World units are tiles: x runs east (0..w, wrapping), z runs south (0..h, row 0 is the
// northern edge), y is up. Animal positions from the engine are already in tile units,
// so a tile coordinate (tx, ty) is the scene point (tx, height, ty).

import * as THREE from "three";
import { mergeGeometries } from "three/examples/jsm/utils/BufferGeometryUtils.js";
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

/** A small four-legged body, about 1 unit long, standing on y = 0 and facing +x. */
function animalGeometry() {
  const parts: THREE.BufferGeometry[] = [];
  const body = new THREE.CapsuleGeometry(0.2, 0.45, 3, 8);
  body.rotateZ(Math.PI / 2);
  body.translate(0, 0.42, 0);
  parts.push(body);
  const head = new THREE.SphereGeometry(0.15, 8, 6);
  head.translate(0.45, 0.58, 0);
  parts.push(head);
  for (const [lx, lz] of [
    [0.25, 0.11],
    [0.25, -0.11],
    [-0.25, 0.11],
    [-0.25, -0.11],
  ]) {
    const leg = new THREE.CylinderGeometry(0.045, 0.04, 0.3, 5);
    leg.translate(lx, 0.15, lz);
    parts.push(leg);
  }
  const merged = mergeGeometries(parts.map((p) => p.toNonIndexed()))!;
  merged.computeVertexNormals();
  return merged;
}

/** The same body as three boxes (36 triangles), for figures too small on screen to show detail. */
function simpleAnimalGeometry() {
  const body = new THREE.BoxGeometry(0.85, 0.36, 0.36).translate(0, 0.42, 0);
  const head = new THREE.BoxGeometry(0.26, 0.26, 0.26).translate(0.45, 0.58, 0);
  const legs = new THREE.BoxGeometry(0.62, 0.3, 0.26).translate(0, 0.15, 0);
  const merged = mergeGeometries([body, head, legs].map((p) => p.toNonIndexed()))!;
  merged.computeVertexNormals();
  return merged;
}

/** Below this on-screen body length (px) animals use the simple geometry. */
const DETAIL_PX = 16;

/** Hash an index to an angle so a herd does not all face the same way. */
const yawOf = (i: number) => (((i * 2654435761) >>> 0) / 4294967296) * Math.PI * 2;

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
  private animalMat = new THREE.MeshLambertMaterial({ color: 0xffffff });
  private animals: THREE.InstancedMesh | null = null;
  private animalData: { pos: Float32Array; species: Uint32Array; actions?: Uint8Array; colors: Map<number, RGB> } | null = null;

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

    const sky = new THREE.Color(0x9cc3e4);
    this.scene.background = sky;
    this.scene.fog = new THREE.Fog(sky, 100, 400);
    this.scene.add(new THREE.HemisphereLight(0xdfefff, 0x4a4030, 1.6));
    const sun = new THREE.DirectionalLight(0xfff4e0, 2.2);
    sun.position.set(-0.6, 1, -0.4);
    this.scene.add(sun);

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
    this.scene.add(this.focusBox, this.regionBox, this.selection);
  }

  // ---------------------------------------------------------------- world

  setWorld(w: number, h: number, elevation: Float32Array) {
    this.w = w;
    this.h = h;
    this.heights = new Float32Array(w * h);
    for (let i = 0; i < w * h; i++) {
      const e = elevation[i];
      this.heights[i] = e >= 0 ? e * RELIEF : e * RELIEF * SEA_FLOOR;
    }
    for (const m of this.terrain) this.scene.remove(m);
    if (this.sea) this.scene.remove(this.sea);
    this.terrainGeo?.dispose();

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
    this.terrain = [-1, 0, 1].map((k) => {
      const m = new THREE.Mesh(geo, this.terrainMat);
      m.position.x = k * w;
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

  setAnimals(pos: Float32Array | undefined, species: Uint32Array | undefined, actions: Uint8Array | undefined, colors: Map<number, RGB>) {
    this.animalData = pos && species ? { pos, species, actions, colors } : null;
    this.lastSize = -1;
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
      return;
    }
    const lmax = Math.log1p(max) || 1;
    const per = d.heat ? 1 : 4;
    // Herds are many and approximate, so they always use the simple body.
    const geo = d.heat ? this.barGeo : this.simpleGeo;
    // Reuse the mesh while it fits: density arrives every frame and reallocating stalls rendering.
    let bars = this.bars;
    if (!bars || bars.geometry !== geo || bars.instanceMatrix.count < n * per) {
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
          bars.setMatrixAt(k, m);
          bars.setColorAt(k++, c);
          continue;
        }
        // Animals inside the watched region are drawn individually.
        if (inFocus(x + 0.5, y + 0.5)) continue;
        const count = 1 + Math.floor(3 * t);
        for (let j = 0; j < count; j++) {
          const r = yawOf(i * 4 + j);
          const px = x + 0.5 + 0.35 * Math.cos(r * 1.7 + j);
          const py = y + 0.5 + 0.35 * Math.sin(r * 2.3 + j);
          q.setFromAxisAngle(up, r);
          sc.setScalar(size);
          p.set(this.nearest(px), this.heightAt(px, py), py);
          bars.setMatrixAt(k, m.compose(p, q, sc));
          bars.setColorAt(k++, c);
        }
      }
    }
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

  setSelected(p: { x: number; y: number } | null) {
    this.selected = p;
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
  private layoutAnimals(size: number) {
    const d = this.animalData;
    if (!d) {
      if (this.animals) this.animals.count = 0;
      return;
    }
    const n = d.species.length;
    if (!this.animals || this.animals.instanceMatrix.count < n) {
      if (this.animals) {
        this.scene.remove(this.animals);
        this.animals.dispose();
      }
      this.animals = new THREE.InstancedMesh(this.animalGeo, this.animalMat, Math.max(256, Math.ceil(n * 1.3)));
      this.animals.frustumCulled = false;
      this.scene.add(this.animals);
    }
    const mesh = this.animals;
    const m = new THREE.Matrix4();
    const q = new THREE.Quaternion();
    const s = new THREE.Vector3();
    const p = new THREE.Vector3();
    const up = new THREE.Vector3(0, 1, 0);
    const c = new THREE.Color();
    for (let i = 0; i < n; i++) {
      const x = this.nearest(d.pos[2 * i]);
      const y = d.pos[2 * i + 1];
      const resting = d.actions?.[i] === 0;
      q.setFromAxisAngle(up, yawOf(i));
      s.set(size, resting ? size * 0.55 : size, size);
      p.set(x, this.heightAt(d.pos[2 * i], y), y);
      mesh.setMatrixAt(i, m.compose(p, q, s));
      const col = d.colors.get(d.species[i]) ?? [255, 255, 255];
      mesh.setColorAt(i, c.setRGB(col[0] / 255, col[1] / 255, col[2] / 255, THREE.SRGBColorSpace));
    }
    mesh.count = n;
    mesh.instanceMatrix.needsUpdate = true;
    if (mesh.instanceColor) mesh.instanceColor.needsUpdate = true;
  }

  render() {
    this.updateCamera();
    const upp = this.unitsPerPixel();
    // Bodies are at least ~9 px long so a herd stays visible from far away.
    const size = Math.min(4, Math.max(0.12, 9 * upp));
    const moved = !(Math.abs(this.cam.tx - this.layoutTx) < this.w / 8);
    if (moved || this.lastSize < 0 || Math.abs(size - this.lastSize) > this.lastSize * 0.05) {
      this.layoutAnimals(size);
      this.layoutHerds(size);
      this.lastSize = size;
      this.layoutTx = this.cam.tx;
    }
    if (this.animals) this.animals.geometry = size / upp >= DETAIL_PX ? this.animalGeo : this.simpleGeo;
    this.placeBox(this.focusBox, this.focusRect, true);
    if (this.selected) {
      const r = Math.max(0.25, 14 * upp);
      this.selection.visible = true;
      this.selection.position.set(this.nearest(this.selected.x), this.heightAt(this.selected.x, this.selected.y) + 0.02, this.selected.y);
      this.selection.scale.setScalar(r);
    } else {
      this.selection.visible = false;
    }
    this.renderer.render(this.scene, this.camera);
  }

  dispose() {
    this.disposed = true;
    this.terrainGeo?.dispose();
    this.terrainTex?.dispose();
    this.animals?.dispose();
    this.animalGeo.dispose();
    this.simpleGeo.dispose();
    this.barGeo.dispose();
    this.bars?.dispose();
    this.renderer.dispose();
  }
}
