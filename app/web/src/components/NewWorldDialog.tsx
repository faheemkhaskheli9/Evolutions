import { NumberInput } from "./NumberInput";
import { useEffect, useState } from "react";
import type { AutosaveInfo, Catalog, FounderGroup, LandShape, NewWorldConfig, WorldGenParams } from "../engine/protocol";
import { rgb } from "../util";

interface Props {
  catalog: Catalog;
  onCreate: (cfg: NewWorldConfig) => void;
  onLoad: (file: File) => void;
  /** An autosaved world that can be resumed, if any. */
  autosave: AutosaveInfo | null;
  onRestore: () => void;
  onClose?: () => void;
  busy: boolean;
}

const RANDOM_FOUNDERS: FounderGroup[] = [
  { species: "hare", count: 400, diversity: 0.8 },
  { species: "deer", count: 150, diversity: 0.8 },
  { species: "wolf", count: 60, diversity: 0.8, delay_years: 40 },
];

const SIZES: { label: string; w: number; h: number }[] = [
  { label: "Small (96 × 48)", w: 96, h: 48 },
  { label: "Medium (128 × 64)", w: 128, h: 64 },
  { label: "Large (192 × 96)", w: 192, h: 96 },
  { label: "Huge (256 × 128)", w: 256, h: 128 },
];

const SHAPES: { key: LandShape; label: string }[] = [
  { key: "Plates", label: "Continents (plate tectonics)" },
  { key: "Supercontinent", label: "Supercontinent" },
  { key: "Archipelago", label: "Archipelago" },
  { key: "MainlandAndIslands", label: "Mainland and islands" },
  { key: "Rift", label: "Rift valley" },
  { key: "Flat", label: "Flat (controlled experiment)" },
];

/** Generator settings (PRD FR-W1), shown when the user customises the world. */
function WorldSettings({ gen, set }: { gen: WorldGenParams; set: (patch: Partial<WorldGenParams>) => void }) {
  const size = SIZES.findIndex((s) => s.w === gen.width && s.h === gen.height);
  const slider = (label: string, key: keyof WorldGenParams, min: number, max: number, step: number, show: (v: number) => string, title?: string) => (
    <label className="setting" title={title}>
      <span>
        {label} <b>{show(gen[key] as number)}</b>
      </span>
      <input type="range" min={min} max={max} step={step} value={gen[key] as number} onChange={(e) => set({ [key]: Number(e.target.value) })} aria-label={label} />
    </label>
  );
  return (
    <div className="settings">
      <label className="setting">
        <span>Size</span>
        <select
          value={size}
          onChange={(e) => {
            const s = SIZES[Number(e.target.value)];
            if (s) set({ width: s.w, height: s.h });
          }}
          aria-label="World size"
        >
          {size < 0 && <option value={-1}>{`Template (${gen.width} × ${gen.height})`}</option>}
          {SIZES.map((s, i) => (
            <option key={s.label} value={i}>
              {s.label}
            </option>
          ))}
        </select>
      </label>
      <label className="setting">
        <span>Land shape</span>
        <select value={gen.shape} onChange={(e) => set({ shape: e.target.value as LandShape })} aria-label="Land shape">
          {SHAPES.map((s) => (
            <option key={s.key} value={s.key}>
              {s.label}
            </option>
          ))}
        </select>
      </label>
      {slider("Land", "land_fraction", 0.05, 0.95, 0.05, (v) => `${Math.round(v * 100)}%`)}
      {slider("Roughness", "roughness", 0, 1, 0.05, (v) => v.toFixed(2), "How rugged mountains and coastlines are.")}
      {slider("Temperature", "temp_offset", -10, 10, 0.5, (v) => `${v > 0 ? "+" : ""}${v.toFixed(1)} °C`, "Offset from an Earth-like climate.")}
      {slider("Rainfall", "precip_factor", 0.2, 2, 0.05, (v) => `×${v.toFixed(2)}`)}
      {slider("Axial tilt", "axial_tilt", 0, 45, 0.5, (v) => `${v.toFixed(1)}°`, "Controls how strong the seasons are (Earth 23.4°).")}
      {slider("North edge", "lat_north", -80, 85, 1, (v) => `${v.toFixed(0)}°`, "Latitude of the top of the map.")}
      {slider("South edge", "lat_south", -85, 80, 1, (v) => `${v.toFixed(0)}°`, "Latitude of the bottom of the map.")}
    </div>
  );
}

export function NewWorldDialog({ catalog, onCreate, onLoad, autosave, onRestore, onClose, busy }: Props) {
  const [template, setTemplate] = useState(catalog.templates[0]?.key ?? "earth");
  const [seed, setSeed] = useState(() => Math.floor(Math.random() * 1e6));
  const [realism, setRealism] = useState<"Accelerated" | "Strict">("Accelerated");
  const [founders, setFounders] = useState<FounderGroup[]>([]);
  const [customise, setCustomise] = useState(false);
  const [gen, setGen] = useState<WorldGenParams>(catalog.default_gen);

  useEffect(() => {
    const t = catalog.templates.find((x) => x.key === template);
    setFounders(t ? t.founders.map((f) => ({ ...f })) : RANDOM_FOUNDERS.map((f) => ({ ...f })));
    setGen({ ...(t?.gen ?? catalog.default_gen) });
  }, [template, catalog]);
  const latOk = gen.lat_north > gen.lat_south;

  const update = (i: number, patch: Partial<FounderGroup>) => setFounders((fs) => fs.map((f, k) => (k === i ? { ...f, ...patch } : f)));
  const speciesMeta = (key: string) => catalog.species.find((s) => s.key === key);

  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true" aria-labelledby="nw-title">
      <div className="modal">
        <div className="row between">
          <h2 id="nw-title">Create a world</h2>
          {onClose && (
            <button className="ghost" onClick={onClose} aria-label="Close">
              ✕
            </button>
          )}
        </div>
        {autosave && (
          <div className="resume row between wrap gap">
            <span>
              <b>Continue where you left off?</b>{" "}
              <span className="muted small">
                {autosave.template} · seed {autosave.seed} · year {Math.floor(autosave.year).toLocaleString()} · saved {new Date(autosave.savedAt).toLocaleString()}
              </span>
            </span>
            <button className="primary" disabled={busy} onClick={onRestore}>
              Resume
            </button>
          </div>
        )}
        <p className="muted">
          Choose a starting world and the founder species. After that, nothing is scripted: genes, bodies, food, climate and chance decide what evolves.
        </p>
        <div className="templates">
          {[...catalog.templates, { key: "random", name: "Random world", description: "Continents, climate and founders generated from the seed.", founders: [], gen: catalog.default_gen }].map((t) => (
            <button key={t.key} className={`template ${template === t.key ? "active" : ""}`} onClick={() => setTemplate(t.key)} aria-pressed={template === t.key}>
              <b>{t.name}</b>
              <span>{t.description}</span>
            </button>
          ))}
        </div>
        <div className="row wrap gap">
          <label>
            Seed{" "}
            <input type="number" value={seed} onChange={(e) => setSeed(Number(e.target.value))} style={{ width: 110 }} />
          </label>
          <button className="ghost" onClick={() => setSeed(Math.floor(Math.random() * 1e6))}>
            🎲 New seed
          </button>
          <label title="Strict uses literature mutation rates (≈1e-5 per gene). Accelerated multiplies them by 20 so change is visible sooner.">
            Mutation rates{" "}
            <select value={realism} onChange={(e) => setRealism(e.target.value as "Accelerated" | "Strict")}>
              <option value="Accelerated">Accelerated ×20 (recommended)</option>
              <option value="Strict">Strict (literature)</option>
            </select>
          </label>
        </div>
        <label className="row gap customise">
          <input type="checkbox" checked={customise} onChange={(e) => setCustomise(e.target.checked)} />
          Customise world settings
          <span className="muted small">{template === "random" && !customise ? "(random settings from the seed)" : ""}</span>
        </label>
        {customise && <WorldSettings gen={gen} set={(patch) => setGen((g) => ({ ...g, ...patch }))} />}
        {customise && !latOk && <p className="small error-text">The north edge must be north of the south edge.</p>}
        <h3>Founders</h3>
        <table className="founders">
          <thead>
            <tr>
              <th>Species</th>
              <th>Count</th>
              <th>Diversity</th>
              <th>Release in year</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {founders.map((f, i) => (
              <tr key={i}>
                <td>
                  <i className="swatch" style={{ background: rgb(speciesMeta(f.species)?.color ?? [128, 128, 128]) }} />
                  <select value={f.species} onChange={(e) => update(i, { species: e.target.value })} aria-label="Founder species">
                    {catalog.species.map((s) => (
                      <option key={s.key} value={s.key}>
                        {s.name}
                      </option>
                    ))}
                  </select>
                </td>
                <td>
                  <NumberInput min={2} max={20000} value={f.count} onChange={(count) => update(i, { count })} aria-label="Count" />
                </td>
                <td>
                  <input type="range" min={0} max={1} step={0.1} value={f.diversity} onChange={(e) => update(i, { diversity: Number(e.target.value) })} aria-label="Genetic diversity" />
                </td>
                <td>
                  <NumberInput min={0} max={1000} value={f.delay_years ?? 0} onChange={(delay_years) => update(i, { delay_years })} aria-label="Release year" />
                </td>
                <td>
                  <button className="ghost" onClick={() => setFounders((fs) => fs.filter((_, k) => k !== i))} aria-label="Remove">
                    ✕
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        <div className="row between wrap gap">
          <button className="ghost" onClick={() => setFounders((fs) => [...fs, { species: "hare", count: 200, diversity: 0.8 }])}>
            + Add species
          </button>
          <span className="muted small">Tip: release predators a few decades after their prey so the prey can establish.</span>
        </div>
        <div className="row between wrap gap footer">
          <label className="ghost file">
            Open a saved world…
            <input
              type="file"
              accept=".evo,.json"
              onChange={(e) => {
                const file = e.target.files?.[0];
                e.target.value = "";
                if (file) onLoad(file);
              }}
            />
          </label>
          <button
            className="primary"
            disabled={busy || founders.length === 0 || (customise && !latOk)}
            onClick={() => onCreate({ template, seed, realism, gen: customise ? gen : null, founders: founders.map((f) => ({ ...f, region: f.region ?? null })) })}
          >
            {busy ? "Generating…" : "Create world"}
          </button>
        </div>
      </div>
    </div>
  );
}
