import { useMemo, useState } from "react";
import type { AlleleInfo, Catalog, SpeciesDetail, SpeciesInfo, Why } from "../engine/protocol";
import { fmt, formatYears, rgb } from "../util";
import { LineChart } from "./LineChart";
import { ALLELE_COLORS, alleleSeries, heterozygositySeries, MAX_ALLELES, neSeries } from "./popgenCharts";
import { describeAllele } from "./WorldPanel";
import { WhyPanel } from "./WhyPanel";

interface Props {
  species: SpeciesInfo[];
  year: number;
  catalog: Catalog;
  selected: number | null;
  onSelect: (id: number) => void;
  detail: SpeciesDetail | null;
  /** Polymorphic alleles of the selected species (null while loading). */
  alleles: AlleleInfo[] | null;
  /** Alleles whose frequency trajectories are charted. */
  trackUids: number[];
  setTrackUids: (uids: number[]) => void;
  /** The trait charted. */
  trait: string;
  setTrait: (key: string) => void;
  /** Explanation of the charted trait's change since the species arose: undefined until asked, then null while working. */
  traitWhy: Why | null | undefined;
  onExplainTrait: (key: string) => void;
}

const KEY_TRAITS = ["body_mass", "insulation", "appendage", "coat", "diet", "brain", "litter", "pace", "heat_tolerance", "kidney", "ornament", "leg_length"];

export function SpeciesPanel({ species, year, catalog, selected, onSelect, detail, alleles, trackUids, setTrackUids, trait, setTrait, traitWhy, onExplainTrait }: Props) {
  const [showExtinct, setShowExtinct] = useState(true);
  const sorted = useMemo(
    () =>
      species
        .filter((s) => showExtinct || s.extinct_year === null)
        .sort((a, b) => Number(a.extinct_year !== null) - Number(b.extinct_year !== null) || b.n - a.n),
    [species, showExtinct],
  );
  const sel = species.find((s) => s.id === selected) ?? null;
  const traitIndex = catalog.traits.findIndex((t) => t.key === trait);
  const traitMeta = catalog.traits[traitIndex];

  return (
    <div className="panel-body">
      <div className="row between">
        <h3>Species</h3>
        <label className="check">
          <input type="checkbox" checked={showExtinct} onChange={(e) => setShowExtinct(e.target.checked)} /> show extinct
        </label>
      </div>
      <ul className="species-list">
        {sorted.map((s) => (
          <li key={s.id}>
            <button className={`species-row ${s.id === selected ? "active" : ""} ${s.extinct_year !== null ? "extinct" : ""}`} onClick={() => onSelect(s.id)}>
              <i className="swatch" style={{ background: rgb(s.color) }} />
              <span className="sp-names">
                <b>{s.common_name}</b>
                <em>{s.name}</em>
              </span>
              <span className="sp-n">{s.extinct_year !== null ? `† yr ${Math.floor(s.extinct_year)}` : s.n.toLocaleString()}</span>
            </button>
          </li>
        ))}
      </ul>
      {sel && (
        <div className="species-detail">
          <h3>
            <i className="swatch" style={{ background: rgb(sel.color) }} /> {sel.common_name}
          </h3>
          <p className="muted small">
            <em>{sel.name}</em> · arose {formatYears(year - sel.origin_year)} ago
            {sel.parent !== null && ` from ${species.find((p) => p.id === sel.parent)?.name ?? "?"}`}
            {sel.extinct_year !== null && ` · extinct in year ${Math.floor(sel.extinct_year)}`}
          </p>
          {sel.split_cause && <p className="callout small">Split: {sel.split_cause}</p>}
          {detail && detail.id === sel.id && detail.metrics.length > 0 ? (
            <>
              <LineChart
                yLabel="Population"
                series={[{ name: "Individuals", color: rgb(sel.color), points: detail.metrics.map((m) => [m.year, m.n]) }]}
                log
              />
              <div className="row">
                <select value={trait} onChange={(e) => setTrait(e.target.value)} aria-label="Trait to chart">
                  {catalog.traits.map((t) => (
                    <option key={t.key} value={t.key}>
                      {t.label}
                    </option>
                  ))}
                </select>
              </div>
              {traitIndex >= 0 && (
                <LineChart
                  yLabel={`${traitMeta.label} (${traitMeta.unit}) — mean ± SD`}
                  series={[
                    {
                      name: "Mean",
                      color: rgb(sel.color),
                      points: detail.metrics.filter((m) => m.n > 0).map((m) => [m.year, m.mean[traitIndex]]),
                      band: detail.metrics.filter((m) => m.n > 0).map((m) => m.sd[traitIndex]),
                    },
                  ]}
                />
              )}
              {traitIndex >= 0 && (
                <div className="trait-why">
                  {traitWhy === undefined ? (
                    <button className="link" onClick={() => onExplainTrait(trait)}>
                      Why did {traitMeta.label.toLowerCase()} change? Selection or drift?
                    </button>
                  ) : (
                    <WhyPanel why={traitWhy ?? undefined} />
                  )}
                </div>
              )}
              <table className="traits">
                <thead>
                  <tr>
                    <th>Trait</th>
                    <th>At origin</th>
                    <th>Now</th>
                    <th>Change</th>
                  </tr>
                </thead>
                <tbody>
                  {detail.traits
                    .filter((t) => KEY_TRAITS.includes(t.key))
                    .map((t) => {
                      const ch = t.now !== null && t.origin !== null && t.origin !== 0 ? (100 * (t.now - t.origin)) / Math.abs(t.origin) : null;
                      return (
                        <tr key={t.key} onClick={() => setTrait(t.key)} className={t.key === trait ? "active" : ""}>
                          <td>{t.label}</td>
                          <td>{t.origin !== null ? fmt(t.origin) : "–"}</td>
                          <td>{t.now !== null ? fmt(t.now) : "–"}</td>
                          <td className={ch === null ? "" : ch > 1 ? "up" : ch < -1 ? "down" : ""}>{ch === null ? "–" : `${ch > 0 ? "+" : ""}${ch.toFixed(1)}%`}</td>
                        </tr>
                      );
                    })}
                </tbody>
              </table>
              {detail.metrics.length > 0 && (
                <p className="muted small">
                  Neutral genetic diversity (heterozygosity): {fmt(detail.metrics[detail.metrics.length - 1].het, 3)}
                </p>
              )}
              <PopGenCharts detail={detail} color={rgb(sel.color)} alleles={alleles} trackUids={trackUids} setTrackUids={setTrackUids} />
            </>
          ) : (
            <p className="muted">Collecting data…</p>
          )}
        </div>
      )}
    </div>
  );
}

/** Population-genetics charts (PRD FR-O7): heterozygosity, F_ST, effective size, allelic richness, allele trajectories. */
function PopGenCharts({ detail, color, alleles, trackUids, setTrackUids }: { detail: SpeciesDetail; color: string } & Pick<Props, "alleles" | "trackUids" | "setTrackUids">) {
  const g = detail.popgen;
  const names = useMemo(() => {
    const byUid = new Map((alleles ?? []).map((a) => [a.uid, a]));
    return detail.uids.map((u) => {
      const a = byUid.get(u);
      return a ? describeAllele(a).replace(/ · \d+%$/, "") : `allele ${u} (now lost, fixed or rare)`;
    });
  }, [alleles, detail.uids]);
  if (g.length === 0) return <p className="muted small">Population-genetics charts start from the next sample (this world was saved before they were recorded).</p>;
  const last = g[g.length - 1];
  const addable = (alleles ?? []).filter((a) => !trackUids.includes(a.uid));
  return (
    <div className="popgen">
      <h4>Population genetics</h4>
      <LineChart yLabel="Expected heterozygosity (neutral loci)" series={heterozygositySeries(g, color)} />
      <LineChart yLabel={`F_ST between patches — now ${fmt(last.fst, 3)}`} series={[{ name: "F_ST", color, points: g.map((s) => [s.year, s.fst]) }]} />
      <LineChart yLabel="Population size: census and effective (Nₑ)" series={neSeries(g, detail.metrics, color)} log />
      <LineChart yLabel="Segregating alleles (all loci)" series={[{ name: "Alleles", color, points: g.map((s) => [s.year, s.segregating]) }]} />
      <div className="row gap small">
        <select
          value=""
          disabled={trackUids.length >= MAX_ALLELES || addable.length === 0}
          onChange={(e) => e.target.value && setTrackUids([...trackUids, Number(e.target.value)])}
          aria-label="Add an allele to the trajectory chart"
        >
          <option value="">{alleles === null ? "Reading the gene pool…" : trackUids.length >= MAX_ALLELES ? `Up to ${MAX_ALLELES} alleles` : "Add allele…"}</option>
          {addable.map((a) => (
            <option key={a.uid} value={a.uid}>
              {describeAllele(a)}
            </option>
          ))}
        </select>
      </div>
      {detail.uids.length > 0 && (
        <>
          <LineChart yLabel="Allele frequency" series={alleleSeries(g, detail.uids, names)} legend={false} />
          <ul className="allele-chips small">
            {detail.uids.map((u, k) => (
              <li key={u}>
                <span>
                  <i className="swatch" style={{ background: ALLELE_COLORS[k % ALLELE_COLORS.length] }} /> {names[k]}
                </span>
                <button className="link" onClick={() => setTrackUids(trackUids.filter((x) => x !== u))} aria-label={`Stop charting ${names[k]}`}>
                  ×
                </button>
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  );
}
