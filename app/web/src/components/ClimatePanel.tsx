import type { Climate, SimEvent } from "../engine/protocol";
import { fmt } from "../util";
import { chartMarkers } from "./chartMarkers";
import { LineChart } from "./LineChart";
import { natureSummary } from "./nature";

const signed = (v: number, digits = 1) => `${v > 0 ? "+" : v < 0 ? "−" : ""}${Math.abs(v).toFixed(digits)}`;

/** The world's climate cycles and natural events (PRD FR-W7, FR-E3), with charts marked by event (FR-E5). */
export function ClimatePanel({ climate, events }: { climate: Climate; events: SimEvent[] }) {
  const markers = chartMarkers(events);
  const cycleMarkers = chartMarkers(events, { cycle: true });
  const g = climate.settings?.glacial;
  return (
    <section className="climate" aria-label="Climate">
      <h3>Climate</h3>
      <p className="muted small">{natureSummary(climate.settings)}</p>
      <div className="climate-now">
        <div>
          <span className="muted small">Global temperature</span>
          <b>{signed(climate.temp_offset)} °C</b>
        </div>
        {g && (
          <div>
            <span className="muted small">Sea level</span>
            <b>{signed(climate.sea_level, 0)} m</b>
          </div>
        )}
        {g && (
          <div>
            <span className="muted small">Large landmasses</span>
            <b>{climate.landmasses}</b>
          </div>
        )}
        <div>
          <span className="muted small">Events under way</span>
          <b>{climate.active.length}</b>
        </div>
      </div>
      {climate.series.length > 1 && (
        <>
          <LineChart
            yLabel="Global temperature offset (°C)"
            series={[{ name: "Temperature", color: "var(--nature)", points: climate.series.map((s) => [s[0], s[1]]) }]}
            markers={markers}
          />
          {g && <LineChart yLabel="Sea level (m)" series={[{ name: "Sea level", color: "var(--ocean)", points: climate.series.map((s) => [s[0], s[2]]) }]} markers={cycleMarkers} />}
        </>
      )}
      {climate.active.length > 0 && (
        <>
          <h4>Under way</h4>
          <ul className="natural-events">
            {climate.active.map((e, i) => (
              <li key={i}>
                {e.kind} around {e.place}, until year {fmt(e.until, 0)}
              </li>
            ))}
          </ul>
        </>
      )}
    </section>
  );
}
