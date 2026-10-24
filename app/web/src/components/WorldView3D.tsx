import { useEffect, useMemo, useRef, useState } from "react";
import type * as THREE from "three";
import { World3D, type Rect } from "../render/world3d";
import { buildDemes, buildScalar, buildSnow, buildTerrain } from "./mapLayers";
import type { MapProps } from "./MapView";

interface Props extends MapProps {
  /** Called when WebGL is unavailable, so the app can fall back to the flat map. */
  onUnsupported: () => void;
}

/**
 * The 3D world: terrain, sea, layers, animals, herds, the watched region and the
 * selection in one scene. Same props and tools as the flat map.
 *
 * Drag to pan, right-drag (or Ctrl-drag) to orbit, scroll to zoom, click to inspect,
 * shift-drag to watch a region.
 */
export function WorldView3D(props: Props) {
  const { world, elevation, biome, water, hydro, frame, frameVersion, speciesColors, layer, layerData, selected, selectedId, tool } = props;
  const [following, setFollowing] = useState(false);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const sceneRef = useRef<World3D | null>(null);
  const composite = useRef<HTMLCanvasElement | null>(null);
  const drag = useRef<{
    x: number;
    y: number;
    moved: boolean;
    mode: "pan" | "orbit" | "region";
    anchor: THREE.Vector3 | null;
    start: THREE.Vector3 | null;
  } | null>(null);
  const regionRef = useRef<Rect | null>(null);
  const handlers = useRef(props);
  handlers.current = props;

  // Renderer: one per mounted canvas.
  useEffect(() => {
    const canvas = canvasRef.current!;
    let scene: World3D;
    try {
      scene = new World3D(canvas);
    } catch {
      handlers.current.onUnsupported();
      return;
    }
    sceneRef.current = scene;
    scene.onFollowLost = () => setFollowing(false);
    scene.resize();
    const ro = new ResizeObserver(() => scene.resize());
    ro.observe(canvas);
    return () => {
      ro.disconnect();
      scene.dispose();
      sceneRef.current = null;
    };
  }, []);

  useEffect(() => {
    sceneRef.current?.setWorld(world.w, world.h, elevation, water, hydro);
  }, [world, elevation, water, hydro]);

  // Rivers are drawn as water in the scene, so the ground under them shows its biome.
  const terrain = useMemo(() => buildTerrain(world, elevation, biome, water, false), [world, elevation, biome, water]);
  const snowSource = frame.current.snow;
  const snow = useMemo(() => (snowSource ? buildSnow(world, snowSource, water) : null), [world, snowSource, water]);
  const overlay = useMemo(
    () => (layerData && layer !== "terrain" && layer !== "population" ? buildScalar(world, layerData, layer, water) : null),
    [world, layerData, layer, water],
  );

  // Drape biomes, the chosen layer and species ranges over the terrain, and place life on it.
  useEffect(() => {
    const scene = sceneRef.current;
    if (!scene) return;
    const f = frame.current;
    let c = composite.current;
    if (!c || c.width !== world.w || c.height !== world.h) {
      c = composite.current = document.createElement("canvas");
      c.width = world.w;
      c.height = world.h;
    }
    const ctx = c.getContext("2d")!;
    ctx.clearRect(0, 0, c.width, c.height);
    ctx.drawImage(terrain, 0, 0);
    const rangesVisible = layer === "terrain" || layer === "population";
    if (snow && rangesVisible) ctx.drawImage(snow, 0, 0);
    if (overlay) ctx.drawImage(overlay, 0, 0);
    const showRanges = rangesVisible && !!f.density && !!f.dominant && (f.mode === "Population" || !!f.focus || layer === "population");
    if (showRanges) ctx.drawImage(buildDemes(world, f.density!, f.dominant!, speciesColors, layer === "population"), 0, 0);
    scene.setTexture(c);
    // Herds stand on the map wherever animals are not simulated one by one, and on the density layer.
    scene.setHerds(f.density, f.dominant, speciesColors, layer === "population", showRanges);
    scene.setAnimals(layer === "population" ? undefined : f.positions, f.species, f.actions, f.ids, f.shape, speciesColors);
    scene.setFocus(f.focus);
    scene.setClock(f.day ?? 0.5, f.speed ?? 0);
  }, [frameVersion, world, terrain, overlay, snow, layer, speciesColors, frame]);

  useEffect(() => {
    sceneRef.current?.setSelected(selected, selectedId);
  }, [selected, selectedId]);

  // Following ends when the inspected animal changes or is gone.
  const canFollow = selectedId !== null && !!frame.current.ids;
  useEffect(() => {
    setFollowing(false);
  }, [selectedId]);
  useEffect(() => {
    sceneRef.current?.follow(following && selectedId !== null ? selectedId : null);
  }, [following, selectedId]);

  // Test hook: screen position of a tile coordinate (used by the end-to-end test).
  useEffect(() => {
    (window as unknown as { __evoMap: unknown }).__evoMap = {
      toScreen: (tx: number, ty: number) => sceneRef.current?.toScreen(tx, ty) ?? [0, 0],
      firstAnimal: () => {
        const p = frame.current.positions;
        return p && p.length >= 2 ? [p[0], p[1]] : null;
      },
    };
  }, [frame]);

  const onWheel = (e: React.WheelEvent) => {
    const scene = sceneRef.current;
    if (!scene) return;
    scene.zoom(Math.exp(e.deltaY * 0.0015), scene.groundAt(e.clientX, e.clientY));
  };

  const onPointerDown = (e: React.PointerEvent) => {
    const scene = sceneRef.current;
    if (!scene) return;
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    const orbit = e.button === 2 || e.ctrlKey || e.altKey;
    const region = !orbit && (e.shiftKey || tool === "focus");
    drag.current = {
      x: e.clientX,
      y: e.clientY,
      moved: false,
      mode: orbit ? "orbit" : region ? "region" : "pan",
      anchor: scene.planeAt(e.clientX, e.clientY),
      start: region ? scene.groundAt(e.clientX, e.clientY) : null,
    };
  };

  const onPointerMove = (e: React.PointerEvent) => {
    const d = drag.current;
    const scene = sceneRef.current;
    if (!d || !scene) return;
    const dx = e.clientX - d.x;
    const dy = e.clientY - d.y;
    if (Math.abs(dx) + Math.abs(dy) > 4) d.moved = true;
    if (d.mode === "orbit") {
      scene.rotate(-dx * 0.006, dy * 0.006);
      d.x = e.clientX;
      d.y = e.clientY;
    } else if (d.mode === "region") {
      const b = scene.groundAt(e.clientX, e.clientY);
      if (!d.start || !b) return;
      // Unwrapped tile coordinates, so a box dragged across the east–west seam stays one box:
      // x0 is moved into [0, w) and x1 may run past the seam (up to one world width).
      const ax = d.start.x;
      const bx = b.x;
      const shift = Math.floor(Math.min(ax, bx) / world.w) * world.w;
      const x0 = Math.min(ax, bx) - shift;
      const x1 = Math.min(Math.max(ax, bx) - shift, x0 + world.w - 1);
      const rect: Rect = [x0, Math.max(0, Math.min(d.start.z, b.z)), x1, Math.min(world.h, Math.max(d.start.z, b.z))];
      scene.setRegionPreview([rect[0] + shift, rect[1], rect[2] + shift, rect[3]]);
      regionRef.current = rect;
    } else if (d.anchor) {
      // Grab the ground: keep the point first pressed under the pointer.
      const p = scene.planeAt(e.clientX, e.clientY);
      if (!p) return;
      const shift = scene.pan(d.anchor.x - p.x, d.anchor.z - p.z);
      d.anchor.x -= shift;
    }
  };

  const onPointerUp = (e: React.PointerEvent) => {
    const d = drag.current;
    const scene = sceneRef.current;
    drag.current = null;
    if (!d || !scene) return;
    if (d.mode === "region") {
      scene.setRegionPreview(null);
      const r = regionRef.current;
      regionRef.current = null;
      if (r && d.moved) {
        handlers.current.onRegion([Math.floor(r[0]), Math.floor(r[1]), Math.ceil(r[2]), Math.ceil(r[3])]);
        return;
      }
    }
    if (d.moved || d.mode === "orbit") return;
    const hit = scene.groundAt(e.clientX, e.clientY);
    if (!hit) return;
    const tx = ((hit.x % world.w) + world.w) % world.w;
    const ty = hit.z;
    if (ty < 0 || ty > world.h) return;
    if (tool === "meteor" || tool === "founders") handlers.current.onToolClick(tx / world.w, ty / world.h);
    else handlers.current.onPick(tx, ty, Math.max(0.3, 8 * scene.unitsPerPixel()));
  };

  return (
    <>
      <canvas
        ref={canvasRef}
        className={`map world3d tool-${tool}`}
        onWheel={onWheel}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onContextMenu={(e) => e.preventDefault()}
        aria-label="3D world. Drag to pan, right-drag to orbit, scroll to zoom, click an animal to inspect it, shift-drag to watch a region."
        role="img"
      />
      {(canFollow || following) && (
        <button
          className={`follow-button${following ? " active" : ""}`}
          aria-pressed={following}
          onClick={() => setFollowing((v) => !v)}
          title="Keep the camera on the inspected animal. Dragging the map stops following."
        >
          {following ? "Following" : "Follow animal"}
        </button>
      )}
    </>
  );
}
