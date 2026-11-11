import type { Brush } from "../engine/protocol";

/** The editor's brushes, in the order the panel lists them (PRD FR-W3). */
export const BRUSHES: { key: Brush; label: string; title: string }[] = [
  { key: "Raise", label: "▲ Raise", title: "Raise the ground: hills and mountains (up to 1,500 m a stroke)" },
  { key: "Lower", label: "▼ Lower", title: "Lower the ground; below sea level it becomes sea" },
  { key: "Ocean", label: "Sea", title: "Make it sea: cut a strait or flood a valley" },
  { key: "Land", label: "Land", title: "Make it land: build a land bridge or an island" },
  { key: "Lake", label: "Lake", title: "Dig a lake (only inland: a basin open to the sea drains)" },
  { key: "Wetter", label: "Wetter", title: "More rain (up to +1,000 mm a year)" },
  { key: "Drier", label: "Drier", title: "Less rain (up to −1,000 mm a year)" },
  { key: "Warmer", label: "Warmer", title: "Warmer (up to +10 °C)" },
  { key: "Colder", label: "Colder", title: "Colder (up to −10 °C)" },
];

/**
 * Adds a pointer position (tile coordinates, x not wrapped) to a stroke, filling gaps
 * so a fast drag leaves no holes: points are kept about half a brush radius apart.
 */
export function extendStroke(points: [number, number][], x: number, y: number, radius: number) {
  const step = Math.max(0.25, radius * 0.5);
  const last = points[points.length - 1];
  if (!last) {
    points.push([x, y]);
    return;
  }
  const d = Math.hypot(x - last[0], y - last[1]);
  if (d < step) return;
  const n = Math.ceil(d / step);
  for (let k = 1; k <= n; k++) points.push([last[0] + ((x - last[0]) * k) / n, last[1] + ((y - last[1]) * k) / n]);
}

/** The points to send: moved so the stroke starts on the map, clamped to its rows, and thinned to the engine's limit. */
export function strokePoints(points: [number, number][], w: number, h: number, max = 4096): [number, number][] {
  if (!points.length) return [];
  const shift = Math.floor(points[0][0] / w) * w;
  let out = points.map(([x, y]) => [Math.min(2 * w, Math.max(-w, x - shift)), Math.min(h, Math.max(0, y))] as [number, number]);
  if (out.length > max) {
    const k = Math.ceil(out.length / max);
    out = out.filter((_, i) => i % k === 0 || i === out.length - 1);
  }
  return out;
}
