import { useEffect, useState } from "react";
import type { Catalog, SpeciesInfo, StopCondition } from "../engine/protocol";

interface Props {
  catalog: Catalog;
  species: SpeciesInfo[];
  disabled: boolean;
  onRun: (years: number, until: StopCondition) => void;
}

type Kind = StopCondition["kind"];

const KINDS: { key: Kind; label: string }[] = [
  { key: "speciation", label: "a new species arises" },
  { key: "extinction", label: "a species goes extinct" },
  { key: "population", label: "population crosses…" },
  { key: "trait", label: "a trait crosses…" },
];

/** "Run until" fast-forward (PRD FR-T3): jump ahead until an event or threshold. */
export function RunUntil({ catalog, species, disabled, onRun }: Props) {
  const [open, setOpen] = useState(false);
  const [kind, setKind] = useState<Kind>("speciation");
  const [sp, setSp] = useState<number | "all">("all");
  const [trait, setTrait] = useState(0);
  const [op, setOp] = useState<"below" | "above">("below");
  const [value, setValue] = useState(1000);
  const [years, setYears] = useState(10000);
  const alive = species.filter((s) => s.extinct_year === null);
  const speciesId = sp === "all" ? (alive[0]?.id ?? null) : sp;
  const needsSpecies = kind === "trait" && speciesId === null;

  const run = () => {
    let until: StopCondition;
    if (kind === "population") until = { kind, species: sp === "all" ? null : sp, op, value };
    else if (kind === "trait") until = { kind, species: speciesId ?? 0, trait, op, value };
    else until = { kind };
    setOpen(false);
    onRun(years, until);
  };

  const traitInfo = catalog.traits[trait];
  const current = kind === "trait" ? alive.find((s) => s.id === speciesId)?.mean?.[trait] : undefined;
  // Start a trait threshold from the species' current value, which is easier to adjust than a blank.
  useEffect(() => {
    if (current !== undefined) setValue(Number(current.toPrecision(3)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [kind, trait, speciesId]);

  return (
    <div className="run-until">
      <button disabled={disabled} onClick={() => setOpen((o) => !o)} aria-expanded={open}>
        Run until…
      </button>
      {open && (
        <div className="popover" role="dialog" aria-label="Run until">
          <label className="setting">
            <span>Stop when</span>
            <select value={kind} onChange={(e) => setKind(e.target.value as Kind)} aria-label="Stop condition">
              {KINDS.map((k) => (
                <option key={k.key} value={k.key}>
                  {k.label}
                </option>
              ))}
            </select>
          </label>
          {(kind === "population" || kind === "trait") && (
            <>
              <label className="setting">
                <span>Species</span>
                <select value={sp === "all" && kind === "trait" ? (speciesId ?? "") : sp} onChange={(e) => setSp(e.target.value === "all" ? "all" : Number(e.target.value))} aria-label="Species">
                  {kind === "population" && <option value="all">All life</option>}
                  {alive.map((s) => (
                    <option key={s.id} value={s.id}>
                      {s.common_name}
                    </option>
                  ))}
                </select>
              </label>
              {kind === "trait" && (
                <label className="setting">
                  <span>Trait</span>
                  <select value={trait} onChange={(e) => setTrait(Number(e.target.value))} aria-label="Trait">
                    {catalog.traits.map((t, i) => (
                      <option key={t.key} value={i}>
                        {t.label}
                      </option>
                    ))}
                  </select>
                </label>
              )}
              <div className="row gap">
                <select value={op} onChange={(e) => setOp(e.target.value as "below" | "above")} aria-label="Direction">
                  <option value="below">falls below</option>
                  <option value="above">rises above</option>
                </select>
                <input type="number" value={value} step="any" onChange={(e) => setValue(Number(e.target.value))} aria-label="Threshold" style={{ width: 100 }} />
                {kind === "trait" && traitInfo?.unit && <span className="muted small">{traitInfo.unit}</span>}
              </div>
              {current !== undefined && <span className="muted small">Now {current.toPrecision(3)}</span>}
            </>
          )}
          <label className="setting">
            <span>For at most</span>
            <select value={years} onChange={(e) => setYears(Number(e.target.value))} aria-label="Maximum years">
              {[100, 1000, 10000, 100000].map((y) => (
                <option key={y} value={y}>
                  {y.toLocaleString()} years
                </option>
              ))}
            </select>
          </label>
          <button className="primary" disabled={needsSpecies} onClick={run}>
            Run
          </button>
        </div>
      )}
    </div>
  );
}
