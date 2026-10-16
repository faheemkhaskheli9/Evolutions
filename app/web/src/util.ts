// Formatting and colour helpers shared by the UI.

export type RGB = [number, number, number];

export const rgb = (c: RGB, a = 1) => `rgba(${c[0]},${c[1]},${c[2]},${a})`;

export function formatYears(y: number): string {
  if (y < 1) return `${Math.floor(y * 365.25)} days`;
  if (y < 10_000) return `${y.toFixed(y < 100 ? 1 : 0)} years`;
  if (y < 1e6) return `${(y / 1000).toFixed(1)} thousand years`;
  return `${(y / 1e6).toFixed(2)} million years`;
}

export function formatDate(year: number, dayOfYear: number, hour: number): string {
  const y = Math.floor(year);
  const months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
  const m = Math.min(11, Math.floor(dayOfYear / 30.44));
  const d = Math.floor(dayOfYear - m * 30.44) + 1;
  return `Year ${y.toLocaleString()} · ${months[m]} ${d} · ${String(hour).padStart(2, "0")}:00`;
}

export const fmt = (n: number, digits = 2) => {
  if (!isFinite(n)) return "–";
  const a = Math.abs(n);
  if (a >= 1000) return Math.round(n).toLocaleString();
  if (a >= 100) return n.toFixed(0);
  if (a >= 10) return n.toFixed(1);
  return n.toFixed(digits);
};

/** Speed presets: simulated seconds per real second. */
export const SPEEDS: { label: string; value: number }[] = [
  { label: "1 hour/s", value: 3600 },
  { label: "1 day/s", value: 86400 },
  { label: "1 week/s", value: 7 * 86400 },
  { label: "1 month/s", value: 30.44 * 86400 },
  { label: "1 year/s", value: 365.25 * 86400 },
  { label: "10 years/s", value: 10 * 365.25 * 86400 },
  { label: "100 years/s", value: 100 * 365.25 * 86400 },
];

export function describeSpeed(simSecondsPerSecond: number): string {
  const s = simSecondsPerSecond;
  if (s < 86400) return `${(s / 3600).toFixed(1)} h/s`;
  if (s < 365 * 86400) return `${(s / 86400).toFixed(1)} days/s`;
  return `${(s / (365.25 * 86400)).toFixed(1)} years/s`;
}

// Biome colours, indexed like the engine's Biome enum.
export const BIOME_COLORS: RGB[] = [
  [28, 63, 112], // ocean
  [52, 104, 160], // lake
  [232, 238, 242], // ice
  [168, 172, 150], // tundra
  [63, 98, 78], // boreal
  [196, 184, 150], // cold desert
  [176, 186, 104], // temperate grassland
  [82, 128, 70], // temperate forest
  [52, 108, 74], // temperate rainforest
  [222, 196, 140], // hot desert
  [198, 180, 96], // savanna
  [104, 140, 62], // tropical seasonal forest
  [36, 110, 56], // tropical rainforest
];

export function ramp(stops: RGB[], t: number): RGB {
  const x = Math.max(0, Math.min(1, t)) * (stops.length - 1);
  const i = Math.min(stops.length - 2, Math.floor(x));
  const f = x - i;
  const a = stops[i];
  const b = stops[i + 1];
  return [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f, a[2] + (b[2] - a[2]) * f];
}

export const TEMP_RAMP: RGB[] = [
  [49, 54, 149],
  [116, 173, 209],
  [224, 243, 248],
  [254, 224, 144],
  [244, 109, 67],
  [165, 0, 38],
];
export const RAIN_RAMP: RGB[] = [
  [140, 81, 10],
  [216, 179, 101],
  [246, 232, 195],
  [128, 205, 193],
  [1, 102, 94],
  [0, 60, 48],
];
export const VEG_RAMP: RGB[] = [
  [120, 90, 60],
  [190, 170, 90],
  [120, 170, 70],
  [30, 110, 40],
];

export function downloadBlob(data: BlobPart, name: string, type = "application/octet-stream") {
  const url = URL.createObjectURL(new Blob([data], { type }));
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
