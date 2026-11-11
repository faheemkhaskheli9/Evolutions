import { useMemo, useState } from "react";
import type { BranchInfo } from "../engine/branches";
import { diffEvents, sharedSpecies, splitYear } from "../engine/branches";
import type { Catalog, CompareSide, SimEvent } from "../engine/protocol";
import { LineChart, type Series } from "./LineChart";
import { checkpointYear } from "./Timeline";

/** Line colours of the two branches (the same in every chart). */
export const BRANCH_COLORS = ["#6fcf8a", "#ff9a5a"] as const;

/** Population or a trait mean of one species in one branch, as chart points. */
export function branchSeries(side: CompareSide, species: number, trait: number | null, color: string, from = -Infinity): Series {
  const m = (side.metrics[species] ?? []).filter((s) => s.year >= from);
  const points: [number, number][] = trait === null ? m.map((s) => [s.year, s.n]) : m.filter((s) => s.n > 0).map((s) => [s.year, s.mean[trait]]);
  return { name: side.name, color, points };
}

function EventList({ events }: { events: SimEvent[] }) {
  if (!events.length) return <p className="muted small">Nothing of note.</p>;
  return (
    <ul className="compare-events">
      {events.map((e, i) => (
        <li key={i}>
          <span className="muted">yr {checkpointYear(e.year)}</span> {e.text}
        </li>
      ))}
    </ul>
  );
}

/** Side-by-side comparison of two branches (PRD FR-T7): overlaid charts and the events that differ. */
export function CompareBranches({
  catalog,
  branches,
  a,
  b,
  data,
  onPick,
  onClose,
}: {
  catalog: Catalog;
  branches: BranchInfo[];
  /** The branches compared. */
  a: number;
  b: number;
  /** The two branches' histories, once the worker has sent them. */
  data: { a: CompareSide; b: CompareSide } | null;
  /** Asks for the histories of branches `a` and `b`. */
  onPick: (a: number, b: number) => void;
  onClose: () => void;
}) {
  const [speciesId, setSpeciesId] = useState<number | null>(null);
  const [trait, setTrait] = useState(catalog.traits[0]?.key ?? "");
  const shared = useMemo(() => (data ? sharedSpecies(data.a, data.b) : []), [data]);
  const diff = useMemo(() => (data ? diffEvents(data.a.events, data.b.events) : null), [data]);
  const sp = shared.find((s) => s.id === speciesId) ?? shared.find((s) => s.extinct_year === null) ?? shared[0];
  const traitIndex = catalog.traits.findIndex((t) => t.key === trait);
  const traitMeta = catalog.traits[traitIndex];
  const split = splitYear(branches, a, b);
  const [zoom, setZoom] = useState(true);
  // Zoomed: as much time before the split as after it, so the shared lead-up shows too.
  const end = data ? Math.max(data.a.year, data.b.year) : 0;
  const from = zoom && split !== null ? split - Math.max(end - split, 1) : -Infinity;

  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true" aria-labelledby="cmp-title">
      <div className="modal compare-dialog">
        <div className="row between">
          <h2 id="cmp-title">Compare branches</h2>
          <button className="ghost" onClick={onClose} aria-label="Close">
            ✕
          </button>
        </div>
        <div className="row wrap gap">
          {(["a", "b"] as const).map((side, k) => (
            <label key={side}>
              <i className="swatch" style={{ background: BRANCH_COLORS[k] }} />{" "}
              <select
                value={side === "a" ? a : b}
                aria-label={side === "a" ? "First branch" : "Second branch"}
                onChange={(e) => (side === "a" ? onPick(Number(e.target.value), b) : onPick(a, Number(e.target.value)))}
              >
                {branches.map((x) => (
                  <option key={x.id} value={x.id}>
                    {x.name} (yr {checkpointYear(x.year)})
                  </option>
                ))}
              </select>
            </label>
          ))}
          {split !== null && a !== b && <span className="muted small">Split in year {checkpointYear(split)}.</span>}
        </div>
        {a === b ? (
          <p className="muted">Pick two different branches.</p>
        ) : !data ? (
          <p className="muted">Loading the branches…</p>
        ) : (
          <>
            {shared.length === 0 ? (
              <p className="muted">No species exists in both branches.</p>
            ) : (
              <>
                <div className="row wrap gap">
                  <select value={sp?.id} onChange={(e) => setSpeciesId(Number(e.target.value))} aria-label="Species to compare">
                    {shared.map((s) => (
                      <option key={s.id} value={s.id}>
                        {s.common_name}
                      </option>
                    ))}
                  </select>
                  <select value={trait} onChange={(e) => setTrait(e.target.value)} aria-label="Trait to compare">
                    {catalog.traits.map((t) => (
                      <option key={t.key} value={t.key}>
                        {t.label}
                      </option>
                    ))}
                  </select>
                  {split !== null && (
                    <label className="small">
                      <input type="checkbox" checked={zoom} onChange={(e) => setZoom(e.target.checked)} /> Zoom to the split
                    </label>
                  )}
                </div>
                {sp && (
                  <div className="compare-grid">
                    <LineChart yLabel={`${sp.common_name}: population`} series={[branchSeries(data.a, sp.id, null, BRANCH_COLORS[0], from), branchSeries(data.b, sp.id, null, BRANCH_COLORS[1], from)]} log />
                    {traitMeta && (
                      <LineChart
                        yLabel={`${sp.common_name}: ${traitMeta.label} (${traitMeta.unit}), mean`}
                        series={[branchSeries(data.a, sp.id, traitIndex, BRANCH_COLORS[0], from), branchSeries(data.b, sp.id, traitIndex, BRANCH_COLORS[1], from)]}
                      />
                    )}
                  </div>
                )}
              </>
            )}
            <h3>What differs</h3>
            <p className="muted small">
              The first {diff!.shared} speciations, extinctions and interventions are shared. After that, only one branch had:
            </p>
            <div className="compare-grid">
              <div>
                <h4>
                  <i className="swatch" style={{ background: BRANCH_COLORS[0] }} /> Only in {data.a.name}
                </h4>
                <EventList events={diff!.onlyA} />
              </div>
              <div>
                <h4>
                  <i className="swatch" style={{ background: BRANCH_COLORS[1] }} /> Only in {data.b.name}
                </h4>
                <EventList events={diff!.onlyB} />
              </div>
            </div>
          </>
        )}
        <p className="muted small">To keep a branch on its own, switch to it and use Save or Export replay.</p>
      </div>
    </div>
  );
}
