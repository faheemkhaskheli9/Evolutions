// Messages between the UI thread and the engine worker (System Design §7).

export type ModeChoice = "Auto" | "Individual" | "Population";

export interface FounderGroup {
  species: string;
  count: number;
  diversity: number;
  region?: [number, number, number] | null;
  delay_years?: number;
}

export interface NewWorldConfig {
  template: string;
  seed: number;
  founders?: FounderGroup[];
  realism?: "Strict" | "Accelerated";
}

export interface Catalog {
  templates: { key: string; name: string; description: string; founders: FounderGroup[] }[];
  species: { key: string; name: string; description: string; color: [number, number, number] }[];
  traits: { key: string; label: string; unit: string }[];
  biomes: string[];
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

export type ToWorker =
  | { type: "create"; config: NewWorldConfig }
  | { type: "load"; bytes: ArrayBuffer }
  | { type: "play" }
  | { type: "pause" }
  | { type: "speed"; simSecondsPerSecond: number }
  | { type: "mode"; mode: ModeChoice }
  | { type: "focus"; rect: [number, number, number, number] }
  | { type: "jump"; years: number }
  | { type: "cancel" }
  | { type: "apply"; intervention: Record<string, unknown> }
  | { type: "pick"; x: number; y: number; radius: number }
  | { type: "inspect"; id: number }
  | { type: "speciesDetail"; id: number }
  | { type: "layer"; kind: string }
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
  density?: Float32Array;
  dominant?: Uint32Array;
  events: SimEvent[];
  speciesList?: SpeciesInfo[];
}

export type FromWorker =
  | { type: "ready"; catalog: Catalog }
  | { type: "created"; world: WorldInfo; elevation: Float32Array; biome: Float32Array; water: Float32Array }
  | FrameMsg
  | { type: "progress"; done: number; total: number; label: string }
  | { type: "jumpDone" }
  | { type: "inspect"; data: Record<string, unknown> | null }
  | { type: "picked"; id: number }
  | { type: "speciesDetail"; id: number; traits: TraitRow[]; metrics: Sample[] }
  | { type: "layer"; kind: string; data: Float32Array }
  | { type: "saved"; bytes: ArrayBuffer }
  | { type: "newick"; text: string }
  | { type: "notice"; message: string }
  | { type: "error"; message: string };
