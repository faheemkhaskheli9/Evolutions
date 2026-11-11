import { useEffect, useMemo, useRef, useState } from "react";
import type { Catalog, DesignReport, SpeciesDesign, TraitMeta } from "../engine/protocol";
import { designFromLibrary, designKey, formatTrait, fromSlider, SLIDER_STEPS, toSlider } from "../engine/designs";
import { BodyPreview } from "../render/bodyPreview";
import { fmt, rgb, type RGB } from "../util";

interface Props {
  catalog: Catalog;
  /** The design to edit; a new design when its key is not in `designs`. */
  initial: SpeciesDesign;
  /** The user's designs, to start from and to keep keys unique. */
  designs: SpeciesDesign[];
  /** Runs the engine's checks (against the current world, when there is one). */
  check: (d: SpeciesDesign) => Promise<DesignReport>;
  /** Whether the checks include a world. */
  withWorld: boolean;
  onSave: (d: SpeciesDesign) => void;
  onCancel: () => void;
}

const GROUPS: { cls: TraitMeta["class"]; title: string }[] = [
  { cls: "Morphology", title: "Body" },
  { cls: "LifeHistory", title: "Life history" },
  { cls: "Behaviour", title: "Behaviour" },
];

const hex = (c: RGB) => `#${c.map((v) => v.toString(16).padStart(2, "0")).join("")}`;
const fromHex = (s: string): RGB => [1, 3, 5].map((i) => parseInt(s.slice(i, i + 2), 16) || 0) as RGB;

/** 3D body of the design, turning slowly. */
function Body({ design }: { design: SpeciesDesign }) {
  const ref = useRef<HTMLCanvasElement>(null);
  const preview = useRef<BodyPreview | null>(null);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    if (!ref.current) return;
    try {
      preview.current = new BodyPreview(ref.current);
    } catch {
      setFailed(true);
    }
    return () => {
      preview.current?.dispose();
      preview.current = null;
    };
  }, []);
  useEffect(() => {
    preview.current?.set({ legs: design.traits.leg_length ?? 1, ornament: design.traits.ornament ?? 0.3, coat: design.traits.coat ?? 0.5, color: design.color });
  }, [design]);
  return failed ? <div className="body-preview muted small">No 3D preview (WebGL is unavailable).</div> : <canvas ref={ref} className="body-preview" aria-label="3D preview of the body" />;
}

/** The species designer (PRD FR-S2, FR-S3): trait sliders, a live preview and the engine's checks. */
export function SpeciesDesigner({ catalog, initial, designs, check, withWorld, onSave, onCancel }: Props) {
  const isNew = !designs.some((d) => d.key === initial.key);
  const [design, setDesign] = useState<SpeciesDesign>(initial);
  const [report, setReport] = useState<DesignReport | null>(null);
  const [checkError, setCheckError] = useState<string | null>(null);
  const [checking, setChecking] = useState(true);
  const latest = useRef(0);
  const otherKeys = useMemo(() => designs.filter((d) => d.key !== initial.key).map((d) => d.key), [designs, initial.key]);

  useEffect(() => {
    const id = ++latest.current;
    setChecking(true);
    const t = setTimeout(() => {
      check(design).then(
        (r) => {
          if (id !== latest.current) return;
          setReport(r);
          setCheckError(null);
          setChecking(false);
        },
        (e: unknown) => {
          if (id !== latest.current) return;
          setReport(null);
          setCheckError(e instanceof Error ? e.message : String(e));
          setChecking(false);
        },
      );
    }, 120);
    return () => clearTimeout(t);
  }, [design, check]);

  const set = (patch: Partial<SpeciesDesign>) => setDesign((d) => ({ ...d, ...patch }));
  const setTrait = (key: string, v: number) => setDesign((d) => ({ ...d, traits: { ...d.traits, [key]: v } }));
  const rename = (name: string) => set(isNew ? { name, key: designKey(name, otherKeys) } : { name });
  /** Copies another species' values, keeping this design's name and key. */
  const startFrom = (value: string) => {
    const [kind, key] = value.split(":");
    const src = kind === "lib" ? designFromLibrary(catalog, key, []) : designs.find((d) => d.key === key);
    if (!src) return;
    setDesign((d) => ({ ...d, traits: { ...src.traits }, color: src.color, variation: src.variation }));
  };

  const problems = report?.problems ?? [];
  const errors = problems.filter((p) => p.level === "Error");
  const flagged = (key: string) => problems.find((p) => p.trait_key === key && p.level !== "Note")?.level;
  const pv = report?.preview;
  const canSave = !!report?.ok && !checking && design.name.trim().length > 0;

  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true" aria-labelledby="sd-title">
      <div className="modal designer">
        <div className="row between">
          <h2 id="sd-title">{isNew ? "Design a species" : `Edit ${initial.name}`}</h2>
          <button className="ghost" onClick={onCancel} aria-label="Close">
            ✕
          </button>
        </div>
        <p className="muted small">
          Set the founders' average adult traits. The checks use the same physiology as the simulation: designs that cannot balance their energy budget, breed, or keep warm
          are refused; risky ones get a warning.
        </p>
        <div className="designer-grid">
          <div className="designer-traits">
            <div className="row wrap gap">
              <label>
                Name <input value={design.name} maxLength={60} onChange={(e) => rename(e.target.value)} aria-label="Species name" />
              </label>
              <label>
                Colour <input type="color" value={hex(design.color)} onChange={(e) => set({ color: fromHex(e.target.value) })} aria-label="Species colour" />
              </label>
              <label>
                Start from{" "}
                <select value="" onChange={(e) => startFrom(e.target.value)} aria-label="Start from">
                  <option value="">Copy values from…</option>
                  <optgroup label="Library">
                    {catalog.species.map((s) => (
                      <option key={s.key} value={`lib:${s.key}`}>
                        {s.name}
                      </option>
                    ))}
                  </optgroup>
                  {designs.length > 0 && (
                    <optgroup label="My species">
                      {designs.map((d) => (
                        <option key={d.key} value={`own:${d.key}`}>
                          {d.name}
                        </option>
                      ))}
                    </optgroup>
                  )}
                </select>
              </label>
            </div>
            <label className="setting wide">
              <span>Description</span>
              <input value={design.description ?? ""} maxLength={500} onChange={(e) => set({ description: e.target.value })} placeholder="What is it, and what do you want to test?" aria-label="Description" />
            </label>
            {GROUPS.map((g) => (
              <fieldset key={g.cls} className="trait-group">
                <legend>{g.title}</legend>
                {catalog.traits
                  .filter((t) => t.class === g.cls)
                  .map((t) => {
                    const v = design.traits[t.key] ?? t.default;
                    const flag = flagged(t.key);
                    return (
                      <label key={t.key} className={`setting trait ${flag ? `flag-${flag.toLowerCase()}` : ""}`}>
                        <span>
                          {t.label} <b>{formatTrait(t, v)}</b>
                        </span>
                        <input type="range" min={0} max={SLIDER_STEPS} step={1} value={toSlider(t, v)} onChange={(e) => setTrait(t.key, fromSlider(t, Number(e.target.value)))} aria-label={t.label} />
                      </label>
                    );
                  })}
              </fieldset>
            ))}
            <label className="setting" title="How much founders differ from each other, relative to a library species (1 = the same). More variation gives selection more to work with.">
              <span>
                Founder variation <b>×{design.variation.toFixed(2)}</b>
              </span>
              <input type="range" min={0.1} max={3} step={0.05} value={design.variation} onChange={(e) => set({ variation: Number(e.target.value) })} aria-label="Founder variation" />
            </label>
          </div>
          <div className="designer-preview">
            <Body design={design} />
            <div className="row gap center">
              <i className="swatch" style={{ background: rgb(design.color) }} />
              <b>{design.name || "Unnamed"}</b>
              {pv && <span className="muted small">{formatTrait(catalog.traits[0], pv.mass_kg)}</span>}
            </div>
            <ul className="design-problems" aria-live="polite">
              {checkError && <li className="level-error">{checkError}</li>}
              {problems.map((p, i) => (
                <li key={i} className={`level-${p.level.toLowerCase()}`}>
                  <b>{p.level === "Error" ? "Rejected" : p.level === "Warning" ? "Warning" : "Note"}:</b> {p.message}
                </li>
              ))}
              {report && problems.length === 0 && <li className="level-ok">No problems found.</li>}
            </ul>
            {pv && report?.ok !== undefined && (
              <table className="design-preview">
                <tbody>
                  <tr>
                    <th>Energy budget</th>
                    <td className={pv.energy_margin < 1 ? "bad" : pv.energy_margin < 1.1 ? "warn" : ""}>
                      needs {fmt(pv.fmr_kj_day + pv.breeding_kj_day, 0)} of {fmt(pv.intake_kj_day, 0)} kJ/day ({Math.round(pv.energy_margin * 100)}%)
                    </td>
                  </tr>
                  <tr>
                    <th>Resting metabolism</th>
                    <td>
                      {fmt(pv.bmr_w)} W{pv.brain_share > 0.005 ? ` · brain ${Math.round(pv.brain_share * 100)}%` : ""}
                    </td>
                  </tr>
                  <tr>
                    <th>Top speed</th>
                    <td>
                      {fmt(pv.top_speed_kmh, 0)} km/h · cruise {fmt(pv.cruise_kmh, 1)}
                    </td>
                  </tr>
                  <tr>
                    <th>Water need</th>
                    <td>{pv.water_l_day < 1 ? `${Math.round(pv.water_l_day * 1000)} ml` : `${fmt(pv.water_l_day, 1)} L`}/day at rest</td>
                  </tr>
                  <tr>
                    <th>Comfortable in</th>
                    <td>
                      {pv.t_min_c <= -80 ? "≤ −80" : fmt(pv.t_min_c, 0)} to {fmt(pv.t_max_c, 0)} °C
                    </td>
                  </tr>
                  <tr>
                    <th>Gestation · weaning</th>
                    <td>
                      {fmt(pv.gestation_days, 0)} · {fmt(pv.weaning_days, 0)} days
                    </td>
                  </tr>
                  <tr>
                    <th>Mature at</th>
                    <td>{pv.maturity_days < 365 ? `${fmt(pv.maturity_days, 0)} days` : `${fmt(pv.maturity_days / 365.25, 1)} years`}</td>
                  </tr>
                  <tr>
                    <th>Lifespan (median)</th>
                    <td>{fmt(pv.lifespan_years, 1)} years</td>
                  </tr>
                  <tr>
                    <th>Young</th>
                    <td>
                      {fmt(pv.litters_per_year, 1)} litters/year · {fmt(pv.lifetime_young, 0)} in a lifetime
                    </td>
                  </tr>
                  <tr>
                    <th>Digests</th>
                    <td>
                      plants {Math.round(pv.plant_efficiency * 100)}% · meat {Math.round(pv.meat_efficiency * 100)}%{pv.roughage_use < 0.1 ? " · no coarse forage" : ""}
                    </td>
                  </tr>
                  {withWorld && pv.habitable_land !== undefined && (
                    <tr>
                      <th>This world</th>
                      <td>{Math.round(pv.habitable_land * 100)}% of land within its limits all year</td>
                    </tr>
                  )}
                </tbody>
              </table>
            )}
          </div>
        </div>
        <div className="row between wrap gap footer">
          <span className="muted small">{checking ? "Checking…" : errors.length > 0 ? `Can't save: ${errors[0].message}` : ""}</span>
          <span className="row gap">
            <button onClick={onCancel}>Cancel</button>
            <button className="primary" disabled={!canSave} onClick={() => onSave({ ...design, name: design.name.trim() })}>
              Save species
            </button>
          </span>
        </div>
      </div>
    </div>
  );
}
