import { NumberInput } from "./NumberInput";
import { useState } from "react";
import { DEFAULT_PATHOGEN, type AlleleInfo, type Brush, type Catalog, type Climate, type Disease, type ExportKind, type ModeChoice, type PathogenSpec, type SimEvent, type SpeciesDesign, type SpeciesInfo, type Status } from "../engine/protocol";
import { ClimatePanel } from "./ClimatePanel";
import { DiseasePanel } from "./DiseasePanel";
import { BRUSHES } from "./brush";
import { ALLELE_RAMP, DISEASE_RAMP } from "../util";
import { DISEASE_FULL } from "./mapLayers";
import { TemplateForm } from "./TemplateForm";
import type { Layer, Tool } from "./MapView";

interface Props {
  /** Climate cycles and natural events, when the world has them (PRD FR-W7, FR-E3). */
  climate: Climate | null;
  /** Pathogens and host resistance, once one has been introduced (PRD FR-L12, FR-G14). */
  disease: Disease | null;
  /** The Chronicle, for chart markers. */
  events: SimEvent[];
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
  /** World editor (PRD FR-W3). */
  brush: Brush;
  setBrush: (b: Brush) => void;
  brushRadius: number;
  setBrushRadius: (r: number) => void;
  brushStrength: number;
  setBrushStrength: (s: number) => void;
  /** Edits that can be undone and redone now. */
  editHistory: [number, number];
  onUndoEdit: () => void;
  onRedoEdit: () => void;
  modeChoice: ModeChoice;
  setMode: (m: ModeChoice) => void;
  apply: (iv: Record<string, unknown>) => void;
  /** Releases the chosen founders where they will do best. */
  onAddFounders: () => void;
  /** Designed species that can be introduced (PRD FR-S2). */
  designs: SpeciesDesign[];
  onDesignSpecies: () => void;
  founderSpecies: string;
  setFounderSpecies: (s: string) => void;
  founderCount: number;
  setFounderCount: (n: number) => void;
  onExportReplay: () => void;
  /** The world's template name, offered as the name of a new user template. */
  worldName: string;
  /** Saves this world's creation settings as a user template and downloads it (PRD FR-W9). */
  onSaveTemplate: (name: string, description: string) => void;
  onReplay: () => void;
  /** Downloads data files; `species` limits the time series to one species. */
  onExportData: (what: ExportKind, species?: number) => void;
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
  { key: "disease", label: "Disease" },
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
  /** Epidemic kind: an infectious pathogen (PRD FR-L12) or an instant die-off. */
  const [infectious, setInfectious] = useState(true);
  const [pathogen, setPathogen] = useState<PathogenSpec>(DEFAULT_PATHOGEN);
  const [exportSpecies, setExportSpecies] = useState<number | "">("");
  const [savingTemplate, setSavingTemplate] = useState(false);
  const alive = p.species.filter((s) => s.extinct_year === null);
  return (
    <div className="panel-body">
      <h3>Map layer</h3>
      <div className="chips">
        {LAYERS.filter((l) => l.key !== "disease" || p.status?.disease || p.layer === "disease").map((l) => (
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

      {p.layer === "disease" && (
        <div className="allele-layer">
          <div className="ramp-legend small" aria-hidden>
            <span>0%</span>
            <i style={{ background: `linear-gradient(to right, ${DISEASE_RAMP.map((c) => `rgb(${c.join(",")})`).join(", ")})` }} />
            <span>{Math.round(DISEASE_FULL * 100)}%+</span>
          </div>
          <p className="muted small">Share of each patch's animals (all species) infected now.</p>
        </div>
      )}

      <h3>Simulation detail</h3>
      <div className="segmented" role="radiogroup" aria-label="Fidelity mode">
        {(["Auto", "Individual", "Population", "Macro"] as ModeChoice[]).map((m) => (
          <button key={m} role="radio" aria-checked={p.modeChoice === m} className={p.modeChoice === m ? "active" : ""} onClick={() => p.setMode(m)}>
            {m}
          </button>
        ))}
      </div>
      <p className="muted small">
        <b>Individual</b>: every animal lives hour by hour. <b>Population</b>: groups evolve year by year using the same biology. <b>Macro</b>: the same groups in
        1,000-year steps, for millions of years — it skips year-to-year swings such as population cycles. Auto picks by speed. To watch animals in a crowded world,
        choose <b>Watch region</b> and drag a box on the map.
      </p>
      <div className="chips">
        <button className={`chip ${p.tool === "focus" ? "active" : ""}`} onClick={() => p.setTool(p.tool === "focus" ? "inspect" : "focus")}>
          ⬚ Watch region
        </button>
      </div>

      <h3>Edit world</h3>
      <p className="muted small">
        Paint mountains, seas, straits, land bridges, lakes and climate. Time stops while you edit; rivers, climate and biomes follow the new terrain. Animals in
        new water drown. Edits are part of the replay log, and edits made at year 0 are kept in templates.
      </p>
      <div className="row wrap">
        <button
          className={p.tool === "paint" ? "active" : ""}
          aria-pressed={p.tool === "paint"}
          disabled={!p.status || p.busy}
          onClick={() => p.setTool(p.tool === "paint" ? "inspect" : "paint")}
        >
          {p.tool === "paint" ? "✓ Done editing" : "✎ Edit world"}
        </button>
        <button disabled={p.editHistory[0] === 0} onClick={p.onUndoEdit} title="Undo the last stroke (Ctrl+Z)">
          ↶ Undo
        </button>
        <button disabled={p.editHistory[1] === 0} onClick={p.onRedoEdit} title="Redo (Ctrl+Shift+Z)">
          ↷ Redo
        </button>
      </div>
      {p.tool === "paint" && (
        <div className="editor">
          <div className="chips" role="radiogroup" aria-label="Brush">
            {BRUSHES.map((b) => (
              <button key={b.key} role="radio" aria-checked={p.brush === b.key} className={`chip ${p.brush === b.key ? "active" : ""}`} title={b.title} onClick={() => p.setBrush(b.key)}>
                {b.label}
              </button>
            ))}
          </div>
          <label className="slider">
            <span>Size</span>
            <input type="range" min={0.5} max={12} step={0.5} value={p.brushRadius} onChange={(e) => p.setBrushRadius(Number(e.target.value))} aria-label="Brush size" />
            <span className="muted small">{p.brushRadius <= 0.5 ? "1 tile" : `${p.brushRadius} tiles`}</span>
          </label>
          <label className="slider">
            <span>Strength</span>
            <input type="range" min={0.05} max={1} step={0.05} value={p.brushStrength} onChange={(e) => p.setBrushStrength(Number(e.target.value))} aria-label="Brush strength" />
            <span className="muted small">{Math.round(p.brushStrength * 100)}%</span>
          </label>
        </div>
      )}

      {p.climate && <ClimatePanel climate={p.climate} events={p.events} />}
      {p.disease?.active && <DiseasePanel disease={p.disease} events={p.events} />}

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
            <optgroup label="Library">
              {p.catalog.species.map((s) => (
                <option key={s.key} value={s.key}>
                  {s.name}
                </option>
              ))}
            </optgroup>
            {p.designs.length > 0 && (
              <optgroup label="My species">
                {p.designs.map((d) => (
                  <option key={d.key} value={d.key}>
                    {d.name}
                  </option>
                ))}
              </optgroup>
            )}
          </select>
          <NumberInput min={2} max={5000} value={p.founderCount} onChange={p.setFounderCount} aria-label="Founder count" />
          <button className={p.tool === "founders" ? "active" : ""} onClick={() => p.setTool(p.tool === "founders" ? "inspect" : "founders")}>
            {p.tool === "founders" ? "Click the map…" : "Place"}
          </button>
          <button onClick={p.onAddFounders}>Best habitat</button>
          <button className="ghost" onClick={p.onDesignSpecies} title="Design your own species, checked against this world">
            Design…
          </button>
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
          <select value={infectious ? "pathogen" : "dieoff"} onChange={(e) => setInfectious(e.target.value === "pathogen")} aria-label="Epidemic kind">
            <option value="pathogen">Infectious disease</option>
            <option value="dieoff">Instant die-off</option>
          </select>
          {!infectious && (
            <>
              <NumberInput min={1} max={100} value={plagueRate} onChange={setPlagueRate} aria-label="Mortality percent" />
              <span className="muted">%</span>
            </>
          )}
          <button
            disabled={plagueSpecies === ""}
            onClick={() =>
              plagueSpecies !== "" &&
              p.apply(infectious ? { kind: "Outbreak", species: plagueSpecies, pathogen } : { kind: "Plague", species: plagueSpecies, mortality: plagueRate / 100 })
            }
          >
            {infectious ? "Release" : "Strike"}
          </button>
        </div>
        {infectious && (
          <div className="pathogen">
            <label>
              <span>Virulence</span>
              <input
                type="range"
                min={0.01}
                max={1}
                step={0.01}
                value={pathogen.virulence}
                onChange={(e) => setPathogen({ ...pathogen, virulence: Number(e.target.value) })}
                aria-label="Virulence"
              />
              <span className="muted small">{pathogen.virulence.toFixed(2)}</span>
            </label>
            <label>
              <span>Transmissibility</span>
              <input
                type="range"
                min={0.01}
                max={1}
                step={0.01}
                value={pathogen.transmissibility}
                onChange={(e) => setPathogen({ ...pathogen, transmissibility: Number(e.target.value) })}
                aria-label="Transmissibility"
              />
              <span className="muted small">{pathogen.transmissibility.toFixed(2)}</span>
            </label>
            <label>
              <span>Infected at first</span>
              <select value={pathogen.share} onChange={(e) => setPathogen({ ...pathogen, share: Number(e.target.value) })} aria-label="Share infected at first">
                {[0.0001, 0.001, 0.01, 0.1, 1].map((v) => (
                  <option key={v} value={v}>
                    {v * 100}%
                  </option>
                ))}
              </select>
            </label>
            <label className="row gap">
              <input type="checkbox" checked={pathogen.trade_off} onChange={(e) => setPathogen({ ...pathogen, trade_off: e.target.checked })} />
              <span>Virulence–transmission trade-off</span>
            </label>
            <p className="muted small">
              The pathogen starts where the species is most crowded, spreads by contact, kills, mutates and meets the hosts&apos; resistance genes. With the trade-off,
              strains transmit only as well as they harm, so intermediate virulence evolves.
            </p>
          </div>
        )}
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
      <h3>Template</h3>
      <p className="muted small">
        Save how this world was created (its settings, seed and founders, not what has happened since) as a template file. Others import it in Create a world and start
        the same world at year 0.
      </p>
      {savingTemplate ? (
        <TemplateForm
          initialName={p.worldName}
          initialDescription=""
          onCancel={() => setSavingTemplate(false)}
          onSave={(name, description) => {
            setSavingTemplate(false);
            p.onSaveTemplate(name, description);
          }}
        />
      ) : (
        <div className="row wrap">
          <button disabled={!p.status || p.busy} onClick={() => setSavingTemplate(true)}>
            Save as template…
          </button>
        </div>
      )}
      <h3>Export data</h3>
      <p className="muted small">
        Download the data for a spreadsheet, R or Python. The files are the same as the command-line runner writes (<code>evo run --csv --chronicle --newick --settings</code>).
      </p>
      <div className="row wrap">
        <select
          value={exportSpecies}
          onChange={(e) => setExportSpecies(e.target.value === "" ? "" : Number(e.target.value))}
          aria-label="Species to export"
        >
          <option value="">All species</option>
          {p.species.map((s) => (
            <option key={s.id} value={s.id}>
              {s.common_name} ({s.name})
            </option>
          ))}
        </select>
        <button
          disabled={!p.status || p.busy}
          onClick={() => p.onExportData("species", exportSpecies === "" ? undefined : exportSpecies)}
          title="Population, heterozygosity and each trait's mean and SD over time, one CSV file per species"
        >
          Time series (CSV)
        </button>
      </div>
      <div className="row wrap">
        <button disabled={!p.status || p.busy} onClick={() => p.onExportData("chronicle")}>
          Chronicle (CSV)
        </button>
        <button disabled={!p.status || p.busy} onClick={() => p.onExportData("newick")}>
          Phylogeny (Newick)
        </button>
        <button disabled={!p.status || p.busy} onClick={() => p.onExportData("settings")} title="Template, seed, realism, world-generator settings and founders">
          Settings (JSON)
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
