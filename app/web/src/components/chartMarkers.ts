import type { SimEvent } from "../engine/protocol";
import type { Marker } from "./LineChart";

/** Natural events too frequent or too local to mark on every chart (they are still in the Chronicle). */
const ROUTINE = /^(Wildfire|Harsh winter|Drought|The drought|The volcanic haze)/;
/** The glacial cycle's own events: land bridges, glacial maxima, interglacials. */
const CYCLE = /seas \(sea level|^Glacial maximum|^Interglacial/;

/**
 * Chart markers for the events that change conditions (PRD FR-E5): your interventions,
 * and the notable climate events (land bridges, glacial maxima and interglacials, volcanic
 * eruptions). `all` keeps the routine natural events too; `cycle` keeps only interventions
 * and the glacial cycle's events. At most `max`, the latest.
 */
export function chartMarkers(events: SimEvent[], { all = false, cycle = false, max = 40 } = {}): Marker[] {
  const out: Marker[] = [];
  for (const e of events) {
    if (e.kind === "Intervention") out.push({ x: e.year, label: e.text, kind: "intervention" });
    else if (e.kind === "Nature" && (cycle ? CYCLE.test(e.text) : all || !ROUTINE.test(e.text))) out.push({ x: e.year, label: e.text, kind: "nature" });
  }
  return out.slice(-max);
}
