import { useMemo } from "react";
import type { SpeciesInfo } from "../engine/protocol";
import { formatYears, rgb } from "../util";

interface Props {
  species: SpeciesInfo[];
  year: number;
  selected: number | null;
  onSelect: (id: number) => void;
  onExport: () => void;
}

/** Species tree over time: each lineage is a horizontal line from origin to now/extinction. */
export function PhyloTree({ species, year, selected, onSelect, onExport }: Props) {
  const layout = useMemo(() => {
    const children = new Map<number | null, SpeciesInfo[]>();
    for (const s of species) {
      const k = s.parent;
      if (!children.has(k)) children.set(k, []);
      children.get(k)!.push(s);
    }
    for (const v of children.values()) v.sort((a, b) => a.origin_year - b.origin_year);
    const rows: { s: SpeciesInfo; row: number }[] = [];
    const visit = (s: SpeciesInfo) => {
      rows.push({ s, row: rows.length });
      for (const c of children.get(s.id) ?? []) visit(c);
    };
    for (const r of children.get(null) ?? []) visit(r);
    return rows;
  }, [species]);

  if (species.length === 0) return <p className="muted panel-body">No species yet.</p>;
  const t0 = Math.min(...species.map((s) => s.origin_year));
  const t1 = Math.max(year, t0 + 1);
  const width = 340;
  const rowH = 26;
  const left = 8;
  const right = 120;
  const height = layout.length * rowH + 30;
  const sx = (t: number) => left + ((t - t0) / (t1 - t0)) * (width - left - right);
  const rowOf = new Map(layout.map((r) => [r.s.id, r.row]));
  const yOf = (row: number) => 14 + row * rowH;

  return (
    <div className="panel-body">
      <div className="row between">
        <h3>Tree of life</h3>
        <button className="ghost" onClick={onExport}>Export Newick</button>
      </div>
      <p className="muted small">
        Lineages over {formatYears(t1 - t0)}. Daughter species branch off where reproductive isolation was detected; dashed lines are extinct.
      </p>
      <svg viewBox={`0 0 ${width} ${height}`} className="phylo" role="img" aria-label="Phylogenetic tree">
        {layout.map(({ s, row }) => {
          const y = yOf(row);
          const end = s.extinct_year ?? year;
          const c = rgb(s.color);
          const parentRow = s.parent !== null ? rowOf.get(s.parent) : undefined;
          return (
            <g key={s.id} className={`branch ${s.id === selected ? "active" : ""}`} onClick={() => onSelect(s.id)}>
              {parentRow !== undefined && <line x1={sx(s.origin_year)} x2={sx(s.origin_year)} y1={yOf(parentRow)} y2={y} stroke={c} strokeWidth={1.5} />}
              <line x1={sx(s.origin_year)} x2={sx(end)} y1={y} y2={y} stroke={c} strokeWidth={s.id === selected ? 5 : 3} strokeDasharray={s.extinct_year !== null ? "4 3" : undefined} opacity={s.extinct_year !== null ? 0.6 : 1} />
              <text x={sx(end) + 6} y={y + 4} className="phylo-label">
                {s.common_name}
                {s.extinct_year !== null ? " †" : ""}
              </text>
            </g>
          );
        })}
        <text x={left} y={height - 4} className="tick">yr {Math.floor(t0)}</text>
        <text x={width - right} y={height - 4} textAnchor="end" className="tick">yr {Math.floor(t1)}</text>
      </svg>
    </div>
  );
}
