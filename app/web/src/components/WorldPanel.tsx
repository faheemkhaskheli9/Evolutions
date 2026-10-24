import { NumberInput } from "./NumberInput";
import { useState } from "react";
import type { AlleleInfo, Catalog, ModeChoice, SpeciesInfo, Status } from "../engine/protocol";
import { ALLELE_RAMP } from "../util";
import type { Layer, Tool } from "./MapView";

interface Props {
  catalog: Catalog;
  status: Status | null;
  species: SpeciesInfo[];
  layer: Layer;
  setLayer: (l: Layer) => void;
  densitySpecies: number | null;
  setDensitySpecies: (id: number | null) => void;
  alleleSpecies: number | null;
  setAlleleSpecies: (id: number) => void;
  /** Polymorphic alleles of `alleleSpecies` (null while loading). */
  alleles: AlleleInfo[] | null;
  alleleUid: number | null;
  setAlleleUid: (uid: number) => void;
  tool: Tool;
  setTool: (t: Tool) => void;
  modeChoice: ModeChoice;
  setMode: (m: ModeChoice) => void;
  apply: (iv: Record<string, unknown>) => void;
  founderSpecies: string;
  setFounderSpecies: (s: string) => void;
  founderCount: number;
  setFounderCount: (n: number) => void;
  onExportReplay: () => void;
  onReplay: () => void;
  /** A jump or replay is running. */
  busy: boolean;
}

const LAYERS: { key: Layer; label: string }[] = [
  { key: "terrain", label: "Terrain & biomes" },
  { key: "temperature", label: "Temperature (today)" },
  { key: "rainfall", label: "Rainfall" },
  { key: "vegetation", label: "Vegetation" },
  { key: "population", label: "Population density" },
  { key: "alleles", label: "Allele frequency" },
];

/** "Body mass +0.12 · new (year 1,200) · 34%" */
export function describeAllele(a: AlleleInfo) {
  const effects = a.effects.length
    ? a.effects.map((e) => `${e.label} ${e.effect >= 0 ? "+" : "−"}${Math.abs(e.effect).toPrecision(2)}`).join(", ")
    : a.kind === "neutral"
      ? `neutral marker ${a.locus}`
      : a.kind === "incompatibility"
        ? `incompatibility locus ${a.locus}`
        : `locus ${a.locus}, no effect`;
  const origin = a.derived ? `new (year ${Math.max(0, Math.floor(a.origin_year)).toLocaleString()})` : "founder";
  return `${effects} · ${origin} · ${(100 * a.freq).toFixed(0)}%`;
}

export function WorldPanel(p: Props) {
  const [plagueSpecies, setPlagueSpecies] = useState<number | "">("");
  const [plagueRate, setPlagueRate] = useState(50);
  const alive = p.species.filter((s) => s.extinct_year === null);
  return (
    <div className="panel-body">
      <h3>Map layer</h3>
      <div className="chips">
        {LAYERS.map((l) => (
          <button key={l.key} className={`chip ${p.layer === l.key ? "active" : ""}`} onClick={() => p.setLayer(l.key)}>
            {l.label}
          </button>
        ))}
      </div>
      {p.layer === "population" && (
        <label className="row gap small">
          Density of
          <select
            value={p.densitySpecies ?? ""}
            onChange={(e) => p.setDensitySpecies(e.target.value === "" ? null : Number(e.target.value))}
            aria-label="Density layer species"
          >
            <option value="">all species</option>
            {p.species
              .filter((s) => s.extinct_year === null || s.id === p.densitySpecies)
              .map((s) => (
                <option key={s.id} value={s.id}>
                  {s.common_name}
                </option>
              ))}
          </select>
        </label>
      )}
      {p.layer === "alleles" && (
        <div className="allele-layer">
          <label className="row gap small">
            Species
            <select value={p.alleleSpecies ?? ""} onChange={(e) => p.setAlleleSpecies(Number(e.target.value))} aria-label="Allele layer species">
              {p.species
                .filter((s) => s.extinct_year === null || s.id === p.alleleSpecies)
                .map((s) => (
                  <option key={s.id} value={s.id}>
                    {s.common_name}
                  </option>
                ))}
            </select>
          </label>
          {p.alleles === null ? (
            <p className="muted small">Reading the gene pool…</p>
          ) : p.alleles.length === 0 && p.alleleUid === null ? (
            <p className="muted small">No allele of this species is between 1% and 99% frequency.</p>
          ) : (
            <label className="row gap small">
              Allele
              <select value={p.alleleUid ?? ""} onChange={(e) => p.setAlleleUid(Number(e.target.value))} aria-label="Allele">
                {p.alleleUid !== null && !p.alleles.some((a) => a.uid === p.alleleUid) && <option value={p.alleleUid}>chosen allele (now lost or fixed)</option>}
                {p.alleles.map((a) => (
                  <option key={a.uid} value={a.uid}>
                    {describeAllele(a)}
                  </option>
                ))}
              </select>
            </label>
          )}
          <div className="ramp-legend small" aria-hidden>
            <span>0%</span>
            <i style={{ background: `linear-gradient(to right, ${ALLELE_RAMP.map((c) => `rgb(${c.join(",")})`).join(", ")})` }} />
            <span>100%</span>
          </div>
          <p className="muted small">
            Share of gene copies in each patch that carry this allele. Trait effects are on the genetic scale. Watch a mutation spread, or a cline form along a
            climate gradient.
          </p>
        </div>
      )}

      <h3>Simulation detail</h3>
      <div className="segmented" role="radiogroup" aria-label="Fidelity mode">
        {(["Auto", "Individual", "Population"] as ModeChoice[]).map((m) => (
          <button key={m} role="radio" aria-checked={p.modeChoice === m} className={p.modeChoice === m ? "active" : ""} onClick={() => p.setMode(m)}>
            {m}
          </button>
        ))}
      </div>
      <p className="muted small">
        <b>Individual</b>: every animal lives hour by hour. <b>Population</b>: groups evolve year by year using the same biology — needed for deep time. Auto picks by
        speed. To watch animals in a crowded world, choose <b>Watch region</b> and drag a box on the map.
      </p>
      <div className="chips">
        <button className={`chip ${p.tool === "focus" ? "active" : ""}`} onClick={() => p.setTool(p.tool === "focus" ? "inspect" : "focus")}>
          ⬚ Watch region
        </button>
      </div>

      <h3>Change the world</h3>
      <p className="muted small">Interventions change conditions, never genes. Evolution does the rest.</p>
      <div className="iv">
        <span>Climate</span>
        <div className="row wrap">
          {[-5, -2, +2, +5].map((d) => (
            <button key={d} onClick={() => p.apply({ kind: "ClimateShift", delta_c: d })}>
              {d > 0 ? `+${d}` : d} °C
            </button>
          ))}
        </div>
        {p.status && Math.abs(p.status.global_temp_offset) > 0.01 && (
          <span className="muted small">Global offset now {p.status.global_temp_offset > 0 ? "+" : ""}{p.status.global_temp_offset.toFixed(1)} °C</span>
        )}
      </div>
      <div className="iv">
        <span>Meteor impact</span>
        <button className={p.tool === "meteor" ? "danger active" : "danger"} onClick={() => p.setTool(p.tool === "meteor" ? "inspect" : "meteor")}>
          {p.tool === "meteor" ? "Click the map to strike…" : "Arm meteor"}
        </button>
      </div>
      <div className="iv">
        <span>Introduce species</span>
        <div className="row wrap">
          <select value={p.founderSpecies} onChange={(e) => p.setFounderSpecies(e.target.value)} aria-label="Species to introduce">
            {p.catalog.species.map((s) => (
              <option key={s.key} value={s.key}>
                {s.name}
              </option>
            ))}
          </select>
          <NumberInput min={2} max={5000} value={p.founderCount} onChange={p.setFounderCount} aria-label="Founder count" />
          <button className={p.tool === "founders" ? "active" : ""} onClick={() => p.setTool(p.tool === "founders" ? "inspect" : "founders")}>
            {p.tool === "founders" ? "Click the map…" : "Place"}
          </button>
          <button onClick={() => p.apply({ kind: "AddFounders", group: { species: p.founderSpecies, count: p.founderCount, diversity: 0.8, region: null } })}>Best habitat</button>
        </div>
      </div>
      <div className="iv">
        <span>Epidemic</span>
        <div className="row wrap">
          <select value={plagueSpecies} onChange={(e) => setPlagueSpecies(e.target.value === "" ? "" : Number(e.target.value))} aria-label="Species struck by epidemic">
            <option value="">Species…</option>
            {alive.map((s) => (
              <option key={s.id} value={s.id}>
                {s.common_name}
              </option>
            ))}
          </select>
          <NumberInput min={1} max={100} value={plagueRate} onChange={setPlagueRate} aria-label="Mortality percent" />
          <span className="muted">%</span>
          <button disabled={plagueSpecies === ""} onClick={() => plagueSpecies !== "" && p.apply({ kind: "Plague", species: plagueSpecies, mortality: plagueRate / 100 })}>
            Strike
          </button>
        </div>
      </div>
      <h3>Replay</h3>
      <p className="muted small">
        Every intervention, mode switch and watched region is logged with its date. Export the log to share the run: opening it rebuilds the world from its seed and
        replays it exactly.
      </p>
      <div className="row wrap">
        <button disabled={!p.status || p.busy} onClick={p.onExportReplay}>
          Export replay log
        </button>
        <button disabled={!p.status || p.busy} onClick={p.onReplay} title="Rebuild this world from its seed and check it ends up identical">
          Replay from the start
        </button>
      </div>
      {p.status && (
        <>
          <h3>Deaths so far</h3>
          <dl className="facts">
            {Object.entries(p.status.deaths)
              .filter(([, v]) => v > 0)
              .sort((a, b) => b[1] - a[1])
              .map(([k, v]) => (
                <div key={k} className="fact-row">
                  <dt>{k}</dt>
                  <dd>{v.toLocaleString()}</dd>
                </div>
              ))}
          </dl>
          <p className="muted small">
            Counted while animals are simulated individually. {p.status.births.toLocaleString()} births · {p.status.alleles.toLocaleString()} alleles tracked ·
            mutation rate {p.status.mutation_rate.toExponential(1)} per locus ({p.status.realism}).
          </p>
        </>
      )}
    </div>
  );
}
