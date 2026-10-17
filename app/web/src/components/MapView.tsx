import { useCallback, useEffect, useMemo, useRef } from "react";
import type { WorldInfo } from "../engine/protocol";
import { BIOME_COLORS, RAIN_RAMP, ramp, TEMP_RAMP, VEG_RAMP, type RGB } from "../util";

export type Layer = "terrain" | "temperature" | "rainfall" | "vegetation" | "population";
export type Tool = "inspect" | "focus" | "meteor" | "founders";

export interface FrameData {
  mode: "Individual" | "Population";
  focus: [number, number, number, number] | null;
  positions?: Float32Array;
  species?: Uint32Array;
  density?: Float32Array;
  dominant?: Uint32Array;
}

interface Props {
  world: WorldInfo;
  elevation: Float32Array;
  biome: Float32Array;
  water: Float32Array;
  frame: React.MutableRefObject<FrameData>;
  frameVersion: number;
  speciesColors: Map<number, RGB>;
  layer: Layer;
  layerData: Float32Array | null;
  selected: { x: number; y: number } | null;
  tool: Tool;
  onPick: (x: number, y: number, radius: number) => void;
  onRegion: (rect: [number, number, number, number]) => void;
  onToolClick: (fx: number, fy: number) => void;
}

/** Terrain image: biome colours with hillshading, water by depth. */
function buildTerrain(world: WorldInfo, elevation: Float32Array, biome: Float32Array, water: Float32Array) {
  const { w, h } = world;
  const c = document.createElement("canvas");
  c.width = w;
  c.height = h;
  const ctx = c.getContext("2d")!;
  const img = ctx.createImageData(w, h);
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const i = y * w + x;
      let col: RGB;
      const wt = water[i];
      if (wt === 1) {
        const depth = Math.min(1, -elevation[i] / 4000);
        col = ramp([[64, 120, 170], [22, 52, 98]], depth);
      } else if (wt === 2 || wt === 3) {
        col = [62, 122, 178];
      } else {
        col = BIOME_COLORS[biome[i] | 0] ?? [120, 120, 120];
        // Relief shading against neighbouring land (the sea floor would exaggerate coasts).
        const e = elevation[i];
        const ex = Math.max(0, elevation[y * w + ((x + 1) % w)]);
        const ey = Math.max(0, elevation[Math.min(h - 1, y + 1) * w + x]);
        const shade = Math.max(-0.35, Math.min(0.35, ((e - ex) + (e - ey)) / 900));
        const high = Math.min(0.35, Math.max(0, (e - 2000) / 5000));
        col = [
          col[0] * (1 + shade) + (255 - col[0]) * high,
          col[1] * (1 + shade) + (255 - col[1]) * high,
          col[2] * (1 + shade) + (255 - col[2]) * high,
        ];
      }
      img.data[i * 4] = col[0];
      img.data[i * 4 + 1] = col[1];
      img.data[i * 4 + 2] = col[2];
      img.data[i * 4 + 3] = 255;
    }
  }
  ctx.putImageData(img, 0, 0);
  return c;
}

function buildScalar(world: WorldInfo, data: Float32Array, layer: Layer, water: Float32Array) {
  const { w, h } = world;
  const c = document.createElement("canvas");
  c.width = w;
  c.height = h;
  const ctx = c.getContext("2d")!;
  const img = ctx.createImageData(w, h);
  for (let i = 0; i < w * h; i++) {
    if (water[i] === 1) continue;
    const v = data[i];
    let col: RGB;
    if (layer === "temperature") col = ramp(TEMP_RAMP, (v + 25) / 60);
    else if (layer === "rainfall") col = ramp(RAIN_RAMP, Math.sqrt(Math.max(0, v) / 3500));
    else {
      if (v < 0) continue;
      col = ramp(VEG_RAMP, v);
    }
    img.data[i * 4] = col[0];
    img.data[i * 4 + 1] = col[1];
    img.data[i * 4 + 2] = col[2];
    img.data[i * 4 + 3] = 200;
  }
  ctx.putImageData(img, 0, 0);
  return c;
}

function buildDemes(world: WorldInfo, density: Float32Array, dominant: Uint32Array, colors: Map<number, RGB>, heat: boolean) {
  const { w, h } = world;
  const c = document.createElement("canvas");
  c.width = w;
  c.height = h;
  const ctx = c.getContext("2d")!;
  const img = ctx.createImageData(w, h);
  let max = 0;
  for (let i = 0; i < density.length; i++) max = Math.max(max, density[i]);
  const lmax = Math.log1p(max) || 1;
  for (let i = 0; i < w * h; i++) {
    const d = density[i];
    if (d <= 0) continue;
    const t = Math.log1p(d) / lmax;
    const col: RGB = heat ? ramp([[30, 30, 80], [180, 40, 120], [255, 200, 60]], t) : colors.get(dominant[i]) ?? [255, 255, 255];
    img.data[i * 4] = col[0];
    img.data[i * 4 + 1] = col[1];
    img.data[i * 4 + 2] = col[2];
    img.data[i * 4 + 3] = heat ? 220 : 50 + 130 * t;
  }
  ctx.putImageData(img, 0, 0);
  return c;
}

export function MapView(props: Props) {
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
    for (let k = -2; k <= 2; k++) {
      const x0 = v.ox + k * span;
      if (x0 + span > 0 && x0 < cw) copies.push(k);
    }
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
      const [ax, ay] = toTile(d.x - rect.left, d.y - rect.top);
      const [bx, by] = toTile(e.clientX - rect.left, e.clientY - rect.top);
      regionRef.current = [Math.min(ax, bx), Math.max(0, Math.min(ay, by)), Math.max(ax, bx), Math.min(world.h, Math.max(ay, by))];
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
