import { DEFAULT_GLACIAL, type NatureSettings } from "../engine/protocol";

/** Nothing on: the world's climate changes only through interventions. */
export const NO_NATURE: NatureSettings = { glacial: null, droughts: false, fires: false, harsh_winters: false, eruptions: false };

/** Everything on, with Earth's glacial cycle. */
export const ALL_NATURE: NatureSettings = { glacial: { ...DEFAULT_GLACIAL }, droughts: true, fires: true, harsh_winters: true, eruptions: true };

export function anyNature(n: NatureSettings | null | undefined): n is NatureSettings {
  return !!n && (!!n.glacial || n.droughts || n.fires || n.harsh_winters || n.eruptions);
}

/** The settings for a new world: null when nothing is on, so the world is the same as one made without them. */
export function natureConfig(n: NatureSettings): NatureSettings | null {
  return anyNature(n) ? { ...n, glacial: n.glacial ? { ...n.glacial } : null } : null;
}

/** "Glacial cycle (100,000 years, −6 °C, sea −120 m), droughts, wildfires" or "None". */
export function natureSummary(n: NatureSettings | null | undefined): string {
  if (!anyNature(n)) return "None";
  const parts: string[] = [];
  if (n.glacial) parts.push(`glacial cycle (${n.glacial.period_years.toLocaleString("en-US")} years, −${n.glacial.cooling_c} °C, sea −${n.glacial.sea_drop_m} m)`);
  if (n.droughts) parts.push("droughts");
  if (n.fires) parts.push("wildfires");
  if (n.harsh_winters) parts.push("harsh winters");
  if (n.eruptions) parts.push("volcanic eruptions");
  const s = parts.join(", ");
  return s[0].toUpperCase() + s.slice(1);
}
