import { useState } from "react";
import type { SimEvent, SpeciesInfo } from "../engine/protocol";
import { rgb } from "../util";

const FILTERS: { key: string; label: string; kinds: SimEvent["kind"][] }[] = [
  { key: "all", label: "All", kinds: [] },
  { key: "species", label: "New species", kinds: ["Speciation"] },
  { key: "extinction", label: "Extinctions", kinds: ["Extinction", "Crash"] },
  { key: "adaptation", label: "Adaptations", kinds: ["Adaptation"] },
  { key: "world", label: "World", kinds: ["Intervention", "Founders", "Mode", "Info"] },
];

export function EventsPanel({ events, species, onSelectSpecies }: { events: SimEvent[]; species: SpeciesInfo[]; onSelectSpecies: (id: number) => void }) {
  const [filter, setFilter] = useState("all");
  const f = FILTERS.find((x) => x.key === filter)!;
  const shown = events.filter((e) => f.kinds.length === 0 || f.kinds.includes(e.kind)).slice(-400).reverse();
  return (
    <div className="panel-body">
      <h3>Chronicle</h3>
      <div className="chips" aria-label="Filter events">
        {FILTERS.map((x) => (
          <button key={x.key} className={`chip ${x.key === filter ? "active" : ""}`} onClick={() => setFilter(x.key)} aria-pressed={x.key === filter}>
            {x.label}
          </button>
        ))}
      </div>
      {shown.length === 0 && <p className="muted">Nothing yet. Run time forward to see what happens.</p>}
      <ol className="events">
        {shown.map((e, i) => {
          const sp = e.species !== null ? species.find((s) => s.id === e.species) : undefined;
          return (
            <li key={`${e.year}-${i}`} className={`ev ev-${e.kind.toLowerCase()}`}>
              <span className="ev-year">yr {e.year < 10 ? e.year.toFixed(2) : Math.floor(e.year).toLocaleString()}</span>
              <span className="ev-text">
                {sp && (
                  <button className="link" onClick={() => onSelectSpecies(sp.id)} title={sp.name}>
                    <i className="swatch" style={{ background: rgb(sp.color) }} />
                  </button>
                )}
                {e.text}
              </span>
            </li>
          );
        })}
      </ol>
    </div>
  );
}
