// Messages between the UI thread and the engine worker (System Design §7).

import type { CheckpointInfo } from "./checkpoints";

export type ModeChoice = "Auto" | "Individual" | "Population";

export interface FounderGroup {
  species: string;
  count: number;
  diversity: number;
  region?: [number, number, number] | null;
  delay_years?: number;
}

export type LandShape = "Plates" | "Archipelago" | "Supercontinent" | "Flat" | "Rift" | "MainlandAndIslands";

/** World-generator settings (PRD FR-W1); mirrors the engine's `WorldGenParams`. */
export interface WorldGenParams {
  width: number;
  height: number;
  lat_north: number;
  lat_south: number;
  shape: LandShape;
  land_fraction: number;
  plates: number;
  roughness: number;
  mountain_height_m: number;
  temp_offset: number;
  precip_factor: number;
  axial_tilt: number;
}

export interface NewWorldConfig {
  template: string;
  seed: number;
  /** Overrides the template's generator settings when present. */
  gen?: WorldGenParams | null;
  founders?: FounderGroup[];
  realism?: "Strict" | "Accelerated";
}

export interface Catalog {
  templates: { key: string; name: string; description: string; founders: FounderGroup[]; gen: WorldGenParams }[];
  default_gen: WorldGenParams;
  species: { key: string; name: string; description: string; color: [number, number, number] }[];
  traits: { key: string; label: string; unit: string }[];
  biomes: string[];
}

/** Rivers and lakes, per tile (drawn as water surfaces in the 3D world). */
export interface Hydrology {
  /** Water flowing through the tile (the engine's flow accumulation). */
  flow: Float32Array;
  /** Index of the tile it drains into, or -1 at an outlet. */
  downstream: Float32Array;
  /** Water surface in metres: a lake's level, otherwise the ground. */
  level: Float32Array;
}

export interface WorldInfo {
  w: number;
  h: number;
  tile_km: number;
  lat_north: number;
  lat_south: number;
  /** Axial tilt (degrees): how far the sun moves north and south over the year. */
  axial_tilt: number;
  template: string;
  template_key: string;
  seed: number;
  patch_size: number;
}

export interface Status {
  year: number;
  day_of_year: number;
  /** Days since the start, with the time of day as the fraction. */
  days: number;
  hour: number;
  mode: "Individual" | "Population";
  focus: [number, number, number, number] | null;
  individuals: number;
  population: number;
  species_alive: number;
  species_total: number;
  alleles: number;
  births: number;
  deaths: Record<string, number>;
  realism: string;
  mutation_rate: number;
  global_temp_offset: number;
  individual_cap: number;
}

export interface SpeciesInfo {
  id: number;
  name: string;
  common_name: string;
  color: [number, number, number];
  parent: number | null;
  origin_year: number;
  extinct_year: number | null;
  split_cause: string | null;
  n: number;
  mean: number[] | null;
  het: number | null;
  origin_mean: number[] | null;
}

export interface SimEvent {
  year: number;
  kind: "Info" | "Founders" | "Speciation" | "Extinction" | "Mode" | "Intervention" | "Crash" | "Adaptation";
  species: number | null;
  text: string;
  /** Position in the world's chronicle (added by the worker). */
  index?: number;
}

export interface Sample {
  year: number;
  n: number;
  mean: number[];
  sd: number[];
  het: number;
}

/** Population-genetics measures of a species at one metrics sample (PRD FR-O7). */
export interface GenSample {
  year: number;
  /** Expected heterozygosity of the whole species (H_T). */
  ht: number;
  /** Mean expected heterozygosity within patches (H_S). */
  hs: number;
  /** (H_T − H_S) / H_T between patches. */
  fst: number;
  /** Effective size from the adult sex ratio. */
  ne: number;
  /** Effective size from the loss of heterozygosity; null when it cannot be estimated. */
  ne_h: number | null;
  /** Generation time used for `ne_h`, years. */
  generation: number;
  /** Alleles present at loci with more than one allele. */
  segregating: number;
  /** Frequency of each requested allele, in request order; null where it was not tracked. */
  freqs: (number | null)[];
}

/** What the Species panel shows for the selected species. */
export interface SpeciesDetail {
  id: number;
  traits: TraitRow[];
  metrics: Sample[];
  popgen: GenSample[];
  /** The alleles `popgen[].freqs` follow, in order. */
  uids: number[];
}

/** A polymorphic allele of a species, for the allele-frequency layer (PRD FR-O1). */
export interface AlleleInfo {
  /** Permanent allele id. */
  uid: number;
  locus: number;
  kind: "trait" | "incompatibility" | "neutral";
  /** False for alleles the founders carried; true for mutations that arose in the simulation. */
  derived: boolean;
  origin_year: number;
  dominance: number;
  /** Effects on traits, on the latent (genetic) scale. */
  effects: { key: string; label: string; effect: number }[];
  /** Frequency in the whole species. */
  freq: number;
}

export interface TraitRow {
  key: string;
  label: string;
  unit: string;
  now: number | null;
  sd: number | null;
  origin: number | null;
}

/** The world kept in the browser's autosave slot (PRD FR-P1). */
export interface AutosaveInfo {
  template: string;
  seed: number;
  year: number;
  savedAt: number;
}

/** What the Export data menu downloads (PRD FR-O11). */
export type ExportKind = "species" | "chronicle" | "newick" | "settings";

export interface ExportFile {
  name: string;
  text: string;
  mime: string;
}

/**
 * The files for an export: every species' time series, or only `species` when given;
 * the Chronicle; the phylogeny; or the world's settings.
 */
export function exportFiles(
  engine: { species_csv_files(): string; chronicle_csv(): string; newick(): string; settings_json(): string },
  what: ExportKind,
  species?: number,
): ExportFile[] {
  switch (what) {
    case "species": {
      const all = JSON.parse(engine.species_csv_files()) as { name: string; text: string }[];
      // Names start `species_<id>_`, so one species can be picked out by id.
      const picked = species === undefined ? all : all.filter((f) => f.name.startsWith(`species_${species}_`));
      return picked.map((f) => ({ ...f, mime: "text/csv" }));
    }
    case "chronicle":
      return [{ name: "chronicle.csv", text: engine.chronicle_csv(), mime: "text/csv" }];
    case "newick":
      return [{ name: "phylogeny.nwk", text: engine.newick(), mime: "text/plain" }];
    case "settings":
      return [{ name: "settings.json", text: engine.settings_json(), mime: "application/json" }];
  }
}

/** When a "run until" fast-forward stops early (PRD FR-T3). */
export type StopCondition =
  | { kind: "speciation" }
  | { kind: "extinction" }
  | { kind: "population"; species: number | null; op: "below" | "above"; value: number }
  | { kind: "trait"; species: number; trait: number; op: "below" | "above"; value: number };

export type ToWorker =
  | { type: "create"; config: NewWorldConfig }
  | { type: "load"; bytes: ArrayBuffer }
  | { type: "restore" }
  | { type: "play" }
  | { type: "pause" }
  | { type: "speed"; simSecondsPerSecond: number }
  | { type: "mode"; mode: ModeChoice }
  | { type: "focus"; rect: [number, number, number, number] }
  | { type: "jump"; years: number; until?: StopCondition }
  | { type: "cancel" }
  | { type: "apply"; intervention: Record<string, unknown> }
  | { type: "pick"; x: number; y: number; radius: number }
  | { type: "inspect"; id: number }
  /** `uids`: alleles whose frequency trajectories to include. */
  | { type: "speciesDetail"; id: number; uids?: number[] }
  /** A map layer: an engine layer name, or `allele:<species>:<uid>` for an allele's frequency. */
  | { type: "layer"; kind: string }
  | { type: "alleles"; species: number }
  /** Whether the density map layer is shown, and for which species (null = all). */
  | { type: "density"; on: boolean; species: number | null }
  | { type: "save" }
  | { type: "newick" }
  /** Data files for analysis elsewhere (PRD FR-O11); the same files as the CLI writes. */
  | { type: "exportData"; what: ExportKind; species?: number }
  /** The current world's replay log as a JSON script (PRD FR-P2). */
  | { type: "exportReplay" }
  /** Replays a JSON script, or the current world from its start when `script` is absent. */
  | { type: "replay"; script?: string }
  /** Restores a checkpoint, discarding the history after it (PRD FR-T6). */
  | { type: "rewind"; id: number }
  /** Restores the latest checkpoint taken before chronicle event `event`. */
  | { type: "rewindBefore"; event: number }
  /** The state hash, to check that a rewound run repeats the original. */
  | { type: "hash" };

export interface FrameMsg {
  type: "frame";
  status: Status;
  playing: boolean;
  modeChoice: ModeChoice;
  achievedSpeed: number;
  capExceeded: boolean;
  positions?: Float32Array;
  species?: Uint32Array;
  actions?: Uint8Array;
  /** Animal ids, in the order of `positions`. */
  ids?: Float64Array;
  /** Four values per animal: mass (kg), leg length index, ornament (0–1), coat luminance (0–1). */
  shape?: Float32Array;
  density?: Float32Array;
  dominant?: Uint32Array;
  /** Snow cover per tile (0–1) for the current day; sent when the day changes. */
  snow?: Float32Array;
  /** Forage per tile as a fraction of its maximum (-1 where nothing grows); sent about once a second. */
  forage?: Float32Array;
  events: SimEvent[];
  speciesList?: SpeciesInfo[];
}

export type FromWorker =
  | { type: "ready"; catalog: Catalog; autosave: AutosaveInfo | null }
  | { type: "created"; world: WorldInfo; elevation: Float32Array; biome: Float32Array; water: Float32Array; hydro: Hydrology }
  | FrameMsg
  /** Long-running work: a jump, or a replay (`title` names it; a jump has none). */
  | { type: "progress"; done: number; total: number; label: string; title?: string }
  | { type: "jumpDone" }
  | { type: "inspect"; id: number; data: Record<string, unknown> | null }
  | { type: "picked"; id: number }
  | { type: "speciesDetail"; id: number; traits: TraitRow[]; metrics: Sample[]; popgen: GenSample[]; uids: number[] }
  | { type: "layer"; kind: string; data: Float32Array }
  | { type: "alleles"; species: number; list: AlleleInfo[] }
  | { type: "saved"; bytes: ArrayBuffer }
  | { type: "newick"; text: string }
  | { type: "exportedData"; files: ExportFile[] }
  | { type: "replayScript"; text: string }
  /** The checkpoints kept for rewind, oldest first, and their total size in bytes. */
  | { type: "checkpoints"; list: CheckpointInfo[]; bytes: number; budget: number }
  | { type: "hash"; year: number; hash: string }
  | { type: "notice"; message: string }
  | { type: "error"; message: string };

/**
 * What the UI does with an `inspect` reply, given the animal inspected now: replies for an
 * earlier animal, or from a world since replaced, are stale; `null` data means the animal
 * died or left the simulated area.
 */
export function inspectReply(inspected: number | null, m: { id: number; data: Record<string, unknown> | null }): "stale" | "update" | "gone" {
  if (m.id !== inspected) return "stale";
  return m.data ? "update" : "gone";
}
