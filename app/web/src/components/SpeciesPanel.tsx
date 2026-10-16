import { useMemo, useState } from "react";
import type { Catalog, Sample, SpeciesInfo, TraitRow } from "../engine/protocol";
import { fmt, formatYears, rgb } from "../util";
import { LineChart } from "./LineChart";

interface Props {
  species: SpeciesInfo[];
  year: number;
  catalog: Catalog;
  selected: number | null;
  onSelect: (id: number) => void;
  detail: { id: number; traits: TraitRow[]; metrics: Sample[] } | null;
}

const KEY_TRAITS = ["body_mass", "insulation", "appendage", "coat", "diet", "brain", "litter", "pace", "heat_tolerance", "kidney", "ornament", "leg_length"];

export function SpeciesPanel({ species, year, catalog, selected, onSelect, detail }: Props) {
  const [trait, setTrait] = useState("body_mass");
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
            </>
          ) : (
            <p className="muted">Collecting data…</p>
          )}
        </div>
      )}
    </div>
  );
}
