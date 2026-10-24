import { useCallback, useEffect, useMemo, useRef } from "react";
import type { Hydrology, WorldInfo } from "../engine/protocol";
import type { RGB } from "../util";
import { buildDemes, buildScalar, buildTerrain, type Layer } from "./mapLayers";

export type { Layer } from "./mapLayers";
export type Tool = "inspect" | "focus" | "meteor" | "founders";

export interface FrameData {
  mode: "Individual" | "Population";
  focus: [number, number, number, number] | null;
  positions?: Float32Array;
  species?: Uint32Array;
  actions?: Uint8Array;
  ids?: Float64Array;
  shape?: Float32Array;
  density?: Float32Array;
  dominant?: Uint32Array;
  snow?: Float32Array;
  /** Day of the year, with the time of day as the fraction. */
  day?: number;
  /** Simulated seconds per real second while playing, 0 while paused. */
  speed?: number;
}

export interface MapProps {
  world: WorldInfo;
  elevation: Float32Array;
  biome: Float32Array;
  water: Float32Array;
  hydro: Hydrology;
  frame: React.MutableRefObject<FrameData>;
  frameVersion: number;
  speciesColors: Map<number, RGB>;
  layer: Layer;
  layerData: Float32Array | null;
  selected: { x: number; y: number } | null;
  /** Id of the inspected animal (the 3D world can follow it). */
  selectedId: number | null;
  tool: Tool;
  onPick: (x: number, y: number, radius: number) => void;
  onRegion: (rect: [number, number, number, number]) => void;
  onToolClick: (fx: number, fy: number) => void;
}

export function MapView(props: MapProps) {
  const { world, elevation, biome, water, frame, frameVersion, speciesColors, layer, layerData, selected, tool } = props;
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const view = useRef({ scale: 4, ox: 0, oy: 0, fitted: false });
  const drag = useRef<{ x: number; y: number; ox: number; oy: number; moved: boolean; region: boolean } | null>(null);
  const regionRef = useRef<[number, number, number, number] | null>(null);
  const demeCanvas = useRef<HTMLCanvasElement | null>(null);

  const terrain = useMemo(() => buildTerrain(world, elevation, biome, water), [world, elevation, biome, water]);
  const overlay = useMemo(
    () => (layerData && layer !== "terrain" && layer !== "population" ? buildScalar(world, layerData, layer, water) : null),
    [world, layerData, layer, water],
  );

  const toTile = useCallback((mx: number, my: number) => {
    const v = view.current;
    let tx = (mx - v.ox) / v.scale;
    tx = ((tx % world.w) + world.w) % world.w;
    const ty = (my - v.oy) / v.scale;
    return [tx, ty] as const;
  }, [world.w]);

  const draw = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d")!;
    const dpr = window.devicePixelRatio || 1;
    const cw = canvas.clientWidth;
    const ch = canvas.clientHeight;
    if (canvas.width !== Math.round(cw * dpr) || canvas.height !== Math.round(ch * dpr)) {
      canvas.width = Math.round(cw * dpr);
      canvas.height = Math.round(ch * dpr);
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    const v = view.current;
    if (!v.fitted && cw > 0) {
      v.scale = Math.max(1, Math.min(cw / world.w, ch / world.h));
      v.ox = (cw - world.w * v.scale) / 2;
      v.oy = (ch - world.h * v.scale) / 2;
      v.fitted = true;
    }
    ctx.fillStyle = getComputedStyle(canvas).getPropertyValue("--ocean") || "#163257";
    ctx.fillRect(0, 0, cw, ch);
    ctx.imageSmoothingEnabled = false;
    const span = world.w * v.scale;
    // The world wraps east–west: draw neighbouring copies where visible.
    const copies: number[] = [];
    for (let k = Math.floor(-v.ox / span); v.ox + k * span < cw; k++) copies.push(k);
    const f = frame.current;
    for (const k of copies) {
      const x0 = v.ox + k * span;
      ctx.drawImage(terrain, x0, v.oy, span, world.h * v.scale);
      if (overlay) ctx.drawImage(overlay, x0, v.oy, span, world.h * v.scale);
      // Species ranges sit on the terrain view (or are the density layer itself); climate
      // and vegetation layers are shown on their own so they stay readable.
      const rangesVisible = layer === "terrain" || layer === "population";
      if (demeCanvas.current && rangesVisible && (f.mode === "Population" || f.focus || layer === "population")) {
        ctx.drawImage(demeCanvas.current, x0, v.oy, span, world.h * v.scale);
      }
    }
    // Individuals.
    if (f.positions && f.species && layer !== "population") {
      const pos = f.positions;
      const sp = f.species;
      const size = Math.max(1.5, Math.min(6, v.scale * 0.12));
      const groups = new Map<number, number[]>();
      for (let i = 0; i < sp.length; i++) {
        let g = groups.get(sp[i]);
        if (!g) groups.set(sp[i], (g = []));
        g.push(i);
      }
      for (const [s, idx] of groups) {
        const c = speciesColors.get(s) ?? [255, 255, 255];
        ctx.fillStyle = `rgb(${c[0]},${c[1]},${c[2]})`;
        ctx.strokeStyle = "rgba(0,0,0,0.55)";
        for (const k of copies) {
          const x0 = v.ox + k * span;
          for (const i of idx) {
            const px = x0 + pos[2 * i] * v.scale - size / 2;
            const py = v.oy + pos[2 * i + 1] * v.scale - size / 2;
            if (px < -size || py < -size || px > cw || py > ch) continue;
            ctx.fillRect(px, py, size, size);
          }
        }
      }
    }
    // Focus region and selection.
    const outline = (r: [number, number, number, number], color: string, dash: number[]) => {
      ctx.save();
      ctx.strokeStyle = color;
      ctx.lineWidth = 2;
      ctx.setLineDash(dash);
      for (const k of copies) {
        ctx.strokeRect(v.ox + k * span + r[0] * v.scale, v.oy + r[1] * v.scale, (r[2] - r[0]) * v.scale, (r[3] - r[1]) * v.scale);
      }
      ctx.restore();
    };
    if (f.focus) outline(f.focus, "rgba(255,255,255,0.85)", [6, 4]);
    if (regionRef.current) outline(regionRef.current, "#ffd54a", []);
    if (selected) {
      for (const k of copies) {
        ctx.beginPath();
        ctx.arc(v.ox + k * span + selected.x * v.scale, v.oy + selected.y * v.scale, Math.max(8, v.scale * 0.5), 0, Math.PI * 2);
        ctx.strokeStyle = "#ffd54a";
        ctx.lineWidth = 2;
        ctx.stroke();
      }
    }
  }, [world, terrain, overlay, frame, speciesColors, selected, layer]);

  // Rebuild the deme overlay when new density arrives.
  useEffect(() => {
    const f = frame.current;
    if (f.density && f.dominant) {
      demeCanvas.current = buildDemes(world, f.density, f.dominant, speciesColors, layer === "population");
    } else if (f.mode === "Individual" && !f.focus) {
      demeCanvas.current = null;
    }
    draw();
  }, [frameVersion, draw, world, frame, speciesColors, layer]);

  useEffect(() => {
    view.current.fitted = false;
  }, [world]);

  // Test hook: screen position of a tile coordinate (used by the end-to-end test).
  useEffect(() => {
    (window as unknown as { __evoMap: unknown }).__evoMap = {
      toScreen: (tx: number, ty: number) => {
        const r = canvasRef.current!.getBoundingClientRect();
        return [r.left + view.current.ox + tx * view.current.scale, r.top + view.current.oy + ty * view.current.scale];
      },
      firstAnimal: () => {
        const p = frame.current.positions;
        return p && p.length >= 2 ? [p[0], p[1]] : null;
      },
    };
  }, [frame]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ro = new ResizeObserver(() => draw());
    ro.observe(canvas);
    return () => ro.disconnect();
  }, [draw]);

  const onWheel = (e: React.WheelEvent) => {
    const rect = canvasRef.current!.getBoundingClientRect();
    const mx = e.clientX - rect.left;
    const my = e.clientY - rect.top;
    const v = view.current;
    const factor = Math.exp(-e.deltaY * 0.0015);
    const ns = Math.max(0.5, Math.min(80, v.scale * factor));
    v.ox = mx - ((mx - v.ox) * ns) / v.scale;
    v.oy = my - ((my - v.oy) * ns) / v.scale;
    v.scale = ns;
    draw();
  };

  const onPointerDown = (e: React.PointerEvent) => {
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    drag.current = {
      x: e.clientX,
      y: e.clientY,
      ox: view.current.ox,
      oy: view.current.oy,
      moved: false,
      region: e.shiftKey || tool === "focus",
    };
  };

  const onPointerMove = (e: React.PointerEvent) => {
    const d = drag.current;
    if (!d) return;
    const dx = e.clientX - d.x;
    const dy = e.clientY - d.y;
    if (Math.abs(dx) + Math.abs(dy) > 4) d.moved = true;
    if (d.region) {
      const rect = canvasRef.current!.getBoundingClientRect();
      // Unwrapped tile coordinates, so a box dragged across the east–west seam stays one box:
      // x0 is moved into [0, w) and x1 may run past the seam (up to one world width).
      const v = view.current;
      const ax = (d.x - rect.left - v.ox) / v.scale;
      const bx = (e.clientX - rect.left - v.ox) / v.scale;
      const ay = (d.y - rect.top - v.oy) / v.scale;
      const by = (e.clientY - rect.top - v.oy) / v.scale;
      const shift = Math.floor(Math.min(ax, bx) / world.w) * world.w;
      const x0 = Math.min(ax, bx) - shift;
      const x1 = Math.min(Math.max(ax, bx) - shift, x0 + world.w - 1);
      regionRef.current = [x0, Math.max(0, Math.min(ay, by)), x1, Math.min(world.h, Math.max(ay, by))];
    } else {
      view.current.ox = d.ox + dx;
      view.current.oy = d.oy + dy;
    }
    draw();
  };

  const onPointerUp = (e: React.PointerEvent) => {
    const d = drag.current;
    drag.current = null;
    if (!d) return;
    const rect = canvasRef.current!.getBoundingClientRect();
    if (d.region && regionRef.current && d.moved) {
      const r = regionRef.current;
      props.onRegion([Math.floor(r[0]), Math.floor(r[1]), Math.ceil(r[2]), Math.ceil(r[3])]);
      regionRef.current = null;
      draw();
      return;
    }
    regionRef.current = null;
    if (!d.moved) {
      const [tx, ty] = toTile(e.clientX - rect.left, e.clientY - rect.top);
      if (ty < 0 || ty > world.h) return;
      if (tool === "meteor" || tool === "founders") props.onToolClick(tx / world.w, ty / world.h);
      else props.onPick(tx, ty, Math.max(0.3, 8 / view.current.scale));
    }
  };

  return (
    <canvas
      ref={canvasRef}
      className={`map tool-${tool}`}
      onWheel={onWheel}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      aria-label="World map. Drag to pan, scroll to zoom, click an animal to inspect it, shift-drag to watch a region."
      role="img"
    />
  );
}
