// Messages between the UI thread and the engine worker (System Design §7).

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
  template: string;
  template_key: string;
  seed: number;
  patch_size: number;
}

export interface Status {
  year: number;
  day_of_year: number;
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
}

export interface Sample {
  year: number;
  n: number;
  mean: number[];
  sd: number[];
  het: number;
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
  | { type: "speciesDetail"; id: number }
  | { type: "layer"; kind: string }
  /** Whether the density map layer is shown, and for which species (null = all). */
  | { type: "density"; on: boolean; species: number | null }
  | { type: "save" }
  | { type: "newick" };

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
  events: SimEvent[];
  speciesList?: SpeciesInfo[];
}

export type FromWorker =
  | { type: "ready"; catalog: Catalog; autosave: AutosaveInfo | null }
  | { type: "created"; world: WorldInfo; elevation: Float32Array; biome: Float32Array; water: Float32Array; hydro: Hydrology }
  | FrameMsg
  | { type: "progress"; done: number; total: number; label: string }
  | { type: "jumpDone" }
  | { type: "inspect"; id: number; data: Record<string, unknown> | null }
  | { type: "picked"; id: number }
  | { type: "speciesDetail"; id: number; traits: TraitRow[]; metrics: Sample[] }
  | { type: "layer"; kind: string; data: Float32Array }
  | { type: "saved"; bytes: ArrayBuffer }
  | { type: "newick"; text: string }
  | { type: "notice"; message: string }
  | { type: "error"; message: string };
