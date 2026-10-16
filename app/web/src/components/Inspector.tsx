import { fmt } from "../util";

/* eslint-disable @typescript-eslint/no-explicit-any */
export function Inspector({ data, onClose }: { data: Record<string, any> | null; onClose: () => void }) {
  if (!data) {
    return (
      <div className="panel-body">
        <h3>Inspector</h3>
        <p className="muted">
          Click an animal on the map to see its body, genes, needs and life story. Animals are visible in <b>Individual</b> mode — slow time down, or shift-drag
          on the map to watch a region.
        </p>
      </div>
    );
  }
  const bar = (label: string, pct: number, cls: string) => (
    <div className="meter">
      <span>{label}</span>
      <div className={`meter-track ${cls}`}>
        <div style={{ width: `${Math.max(0, Math.min(100, pct))}%` }} />
      </div>
      <b>{Math.round(pct)}%</b>
    </div>
  );
  const p = data.physiology ?? {};
  const t = data.tile ?? {};
  return (
    <div className="panel-body">
      <div className="row between">
        <h3>
          {data.common_name} #{data.id}
        </h3>
        <button className="ghost" onClick={onClose} aria-label="Close inspector">✕</button>
      </div>
      <p className="muted small">
        <em>{data.species_name}</em> · {data.sex} · {fmt(data.age_years, 2)} years old · generation {data.generation}
      </p>
      <p className="now-doing">{data.action}{data.pregnant ? ` · pregnant with ${data.pregnant}` : ""}</p>
      {bar("Energy reserves", data.energy_pct, "energy")}
      {bar("Hydration", data.hydration_pct, "water")}
      <dl className="facts">
        <dt>Body mass</dt>
        <dd>{fmt(data.mass_kg)} kg of {fmt(data.adult_mass_kg)} kg adult{data.mature ? "" : " (growing)"}</dd>
        <dt>Top speed</dt>
        <dd>{fmt(p.top_speed_kmh)} km/h</dd>
        <dt>Resting metabolism</dt>
        <dd>{fmt(p.bmr_w)} W</dd>
        <dt>Digestion</dt>
        <dd>plants {fmt(p.plant_digestion)} · meat {fmt(p.meat_digestion)}</dd>
        <dt>Offspring</dt>
        <dd>{data.offspring} (matings {data.matings})</dd>
        <dt>New mutations</dt>
        <dd>{data.new_mutations}</dd>
        <dt>Heterozygosity</dt>
        <dd>{fmt(data.heterozygosity, 2)}</dd>
        <dt>Here</dt>
        <dd>
          {t.biome}, {fmt(t.temp_now, 1)} °C now (mean {fmt(t.temp_mean, 1)} °C), {fmt(t.precip, 0)} mm/yr, forage {fmt(t.forage_pct, 0)}%
        </dd>
      </dl>
      <h4>Phenotype (from genes + environment)</h4>
      <table className="traits compact">
        <tbody>
          {(data.traits as { key: string; label: string; unit: string; value: number }[]).map((tr) => (
            <tr key={tr.key}>
              <td>{tr.label}</td>
              <td>
                {fmt(tr.value)} <span className="muted">{tr.unit}</span>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
