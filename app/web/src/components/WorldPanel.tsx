import { NumberInput } from "./NumberInput";
import { useState } from "react";
import type { Catalog, ModeChoice, SpeciesInfo, Status } from "../engine/protocol";
import type { Layer, Tool } from "./MapView";

interface Props {
  catalog: Catalog;
  status: Status | null;
  species: SpeciesInfo[];
  layer: Layer;
  setLayer: (l: Layer) => void;
  densitySpecies: number | null;
  setDensitySpecies: (id: number | null) => void;
  tool: Tool;
  setTool: (t: Tool) => void;
  modeChoice: ModeChoice;
  setMode: (m: ModeChoice) => void;
  apply: (iv: Record<string, unknown>) => void;
  founderSpecies: string;
  setFounderSpecies: (s: string) => void;
  founderCount: number;
  setFounderCount: (n: number) => void;
}

const LAYERS: { key: Layer; label: string }[] = [
  { key: "terrain", label: "Terrain & biomes" },
  { key: "temperature", label: "Temperature (today)" },
  { key: "rainfall", label: "Rainfall" },
  { key: "vegetation", label: "Vegetation" },
  { key: "population", label: "Population density" },
];

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
