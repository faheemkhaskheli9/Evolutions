import { useEffect, useState } from "react";
import type { Catalog, FounderGroup, NewWorldConfig } from "../engine/protocol";
import { rgb } from "../util";

interface Props {
  catalog: Catalog;
  onCreate: (cfg: NewWorldConfig) => void;
  onLoad: (file: File) => void;
  onClose?: () => void;
  busy: boolean;
}

const RANDOM_FOUNDERS: FounderGroup[] = [
  { species: "hare", count: 400, diversity: 0.8 },
  { species: "deer", count: 150, diversity: 0.8 },
  { species: "wolf", count: 60, diversity: 0.8, delay_years: 40 },
];

export function NewWorldDialog({ catalog, onCreate, onLoad, onClose, busy }: Props) {
  const [template, setTemplate] = useState(catalog.templates[0]?.key ?? "earth");
  const [seed, setSeed] = useState(() => Math.floor(Math.random() * 1e6));
  const [realism, setRealism] = useState<"Accelerated" | "Strict">("Accelerated");
  const [founders, setFounders] = useState<FounderGroup[]>([]);

  useEffect(() => {
    const t = catalog.templates.find((x) => x.key === template);
    setFounders(t ? t.founders.map((f) => ({ ...f })) : RANDOM_FOUNDERS.map((f) => ({ ...f })));
  }, [template, catalog]);

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
        <p className="muted">
          Choose a starting world and the founder species. After that, nothing is scripted: genes, bodies, food, climate and chance decide what evolves.
        </p>
        <div className="templates">
          {[...catalog.templates, { key: "random", name: "Random world", description: "Continents, climate and founders generated from the seed.", founders: [] }].map((t) => (
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
                  <input type="number" min={2} max={20000} value={f.count} onChange={(e) => update(i, { count: Math.max(2, Number(e.target.value)) })} aria-label="Count" />
                </td>
                <td>
                  <input type="range" min={0} max={1} step={0.1} value={f.diversity} onChange={(e) => update(i, { diversity: Number(e.target.value) })} aria-label="Genetic diversity" />
                </td>
                <td>
                  <input type="number" min={0} max={1000} value={f.delay_years ?? 0} onChange={(e) => update(i, { delay_years: Math.max(0, Number(e.target.value)) })} aria-label="Release year" />
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
            <input type="file" accept=".evo" onChange={(e) => e.target.files?.[0] && onLoad(e.target.files[0])} />
          </label>
          <button
            className="primary"
            disabled={busy || founders.length === 0}
            onClick={() => onCreate({ template, seed, realism, founders: founders.map((f) => ({ ...f, region: f.region ?? null })) })}
          >
            {busy ? "Generating…" : "Create world"}
          </button>
        </div>
      </div>
    </div>
  );
}
