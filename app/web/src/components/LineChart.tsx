import { useMemo, useState } from "react";
import { fmt } from "../util";

export interface Series {
  name: string;
  color: string;
  points: [number, number][];
  /** Optional ±band around each point. */
  band?: number[];
}

interface Props {
  series: Series[];
  height?: number;
  yLabel?: string;
  xLabel?: string;
  log?: boolean;
}

/** Round tick values (1, 2 or 5 × 10ⁿ apart) covering [min, max], about `count` of them. */
function niceTicks(min: number, max: number, count = 3): number[] {
  const raw = (max - min) / Math.max(1, count - 1);
  if (!(raw > 0) || !isFinite(raw)) return [min];
  const mag = 10 ** Math.floor(Math.log10(raw));
  const step = [1, 2, 5, 10].map((m) => m * mag).find((s) => s >= raw) ?? 10 * mag;
  const ticks: number[] = [];
  for (let t = Math.ceil(min / step - 1e-9) * step; t <= max + step * 1e-9; t += step) ticks.push(Math.abs(t) < step * 1e-9 ? 0 : t);
  return ticks;
}

/** Small dependency-free SVG line chart with axes and a hover read-out. */
export function LineChart({ series, height = 160, yLabel, xLabel = "Year", log = false }: Props) {
  const width = 340;
  const pad = { l: 46, r: 10, t: 10, b: 26 };
  const [hover, setHover] = useState<number | null>(null);
  const tf = (v: number) => (log ? Math.log10(Math.max(1, v)) : v);

  const { xmin, xmax, ymin, ymax } = useMemo(() => {
    let xmin = Infinity, xmax = -Infinity, ymin = Infinity, ymax = -Infinity;
    for (const s of series) {
      s.points.forEach(([x, y], i) => {
        const b = s.band?.[i] ?? 0;
        xmin = Math.min(xmin, x);
        xmax = Math.max(xmax, x);
        ymin = Math.min(ymin, tf(y - b));
        ymax = Math.max(ymax, tf(y + b));
      });
    }
    if (!isFinite(xmin)) return { xmin: 0, xmax: 1, ymin: 0, ymax: 1 };
    if (xmax === xmin) xmax = xmin + 1;
    if (ymax === ymin) {
      ymax += Math.abs(ymax) * 0.05 + 1e-3;
      ymin -= Math.abs(ymin) * 0.05 + 1e-3;
    }
    const m = (ymax - ymin) * 0.06;
    return { xmin, xmax, ymin: log ? ymin : ymin - m, ymax: ymax + m };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [series, log]);

  const sx = (x: number) => pad.l + ((x - xmin) / (xmax - xmin)) * (width - pad.l - pad.r);
  const sy = (y: number) => pad.t + (1 - (tf(y) - ymin) / (ymax - ymin)) * (height - pad.t - pad.b);
  // y ticks in transformed units; on a log scale prefer whole powers of ten.
  const decades = log ? niceTicks(ymin, ymax, 4).filter((t) => Number.isInteger(t)) : [];
  const yTicks = decades.length >= 2 ? decades : log ? [0, 0.5, 1].map((f) => ymin + f * (ymax - ymin)) : niceTicks(ymin, ymax, 4);
  const xTicks = niceTicks(xmin, xmax, 4);
  const syT = (t: number) => pad.t + (1 - (t - ymin) / (ymax - ymin)) * (height - pad.t - pad.b);

  const onMove = (e: React.PointerEvent<SVGSVGElement>) => {
    const r = e.currentTarget.getBoundingClientRect();
    const x = ((e.clientX - r.left) / r.width) * width;
    setHover(xmin + ((x - pad.l) / (width - pad.l - pad.r)) * (xmax - xmin));
  };

  const hoverRows = hover === null ? [] : series.map((s) => {
    let best = s.points[0];
    for (const p of s.points) if (Math.abs(p[0] - hover) < Math.abs((best?.[0] ?? Infinity) - hover)) best = p;
    return { name: s.name, color: s.color, p: best };
  }).filter((r) => r.p);

  return (
    <div className="chart">
      {yLabel && <div className="chart-ylabel">{yLabel}</div>}
      <svg viewBox={`0 0 ${width} ${height}`} onPointerMove={onMove} onPointerLeave={() => setHover(null)} role="img" aria-label={yLabel ?? "chart"}>
        {yTicks.map((t, i) => (
          <g key={i}>
            <line x1={pad.l} x2={width - pad.r} y1={syT(t)} y2={syT(t)} className="grid" />
            <text x={pad.l - 6} y={syT(t) + 4} textAnchor="end" className="tick">
              {fmt(log ? 10 ** t : t)}
            </text>
          </g>
        ))}
        {xTicks.map((t, i) => (
          <text key={i} x={sx(t)} y={height - 8} textAnchor={sx(t) < pad.l + 12 ? "start" : sx(t) > width - pad.r - 12 ? "end" : "middle"} className="tick">
            {xmax - xmin >= 10 ? Math.round(t).toLocaleString() : fmt(t, xmax - xmin < 1 ? 2 : 1)}
          </text>
        ))}
        {series.map((s) => (
          <g key={s.name}>
            {s.band && s.points.length > 1 && (
              <path
                d={
                  s.points.map(([x, y], i) => `${i ? "L" : "M"}${sx(x)},${sy(y + (s.band![i] ?? 0))}`).join("") +
                  s.points.slice().reverse().map(([x, y], i) => `L${sx(x)},${sy(y - (s.band![s.points.length - 1 - i] ?? 0))}`).join("") +
                  "Z"
                }
                fill={s.color}
                opacity={0.15}
              />
            )}
            <path d={s.points.map(([x, y], i) => `${i ? "L" : "M"}${sx(x)},${sy(y)}`).join("")} fill="none" stroke={s.color} strokeWidth={2} strokeLinejoin="round" />
            {s.points.length === 1 && <circle cx={sx(s.points[0][0])} cy={sy(s.points[0][1])} r={3} fill={s.color} />}
          </g>
        ))}
        {hover !== null && hover >= xmin && hover <= xmax && <line x1={sx(hover)} x2={sx(hover)} y1={pad.t} y2={height - pad.b} className="cursor" />}
      </svg>
      <div className="chart-foot">
        {hoverRows.length > 0 ? (
          hoverRows.map((r) => (
            <span key={r.name}>
              <i style={{ background: r.color }} /> {r.name}: <b>{fmt(r.p[1])}</b> at {xLabel.toLowerCase()} {fmt(r.p[0], 1)}
            </span>
          ))
        ) : (
          <span className="muted">{xLabel} →</span>
        )}
      </div>
    </div>
  );
}
