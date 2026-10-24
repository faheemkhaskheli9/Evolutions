// In-session checkpoints for rewind (PRD FR-T6): compressed `.evo` saves taken along the
// run, thinned to stay within a memory budget.

/** Why a checkpoint was taken. */
export type CheckpointReason = "start" | "periodic" | "jump" | "intervention";

/** A checkpoint as the UI sees it (without its bytes). */
export interface CheckpointInfo {
  id: number;
  /** Simulated years since the start, with the time of year as the fraction. */
  year: number;
  /** Chronicle events that had happened when it was taken. */
  events: number;
  reason: CheckpointReason;
  /** Compressed size in bytes. */
  size: number;
}

export interface Checkpoint extends CheckpointInfo {
  bytes: Uint8Array;
}

export const DEFAULT_BUDGET = 200 * 1024 * 1024;

/**
 * Checkpoints in time order, within `budget` bytes. When over budget, the checkpoint
 * whose neighbours are closest together relative to its age is dropped, so recent
 * history stays dense and older history gets sparser (roughly log-spaced). The first
 * is kept while there are more than two, and the latest always; checkpoints taken
 * before an intervention count double.
 */
export class CheckpointStore {
  private list: Checkpoint[] = [];
  private nextId = 1;
  total = 0;

  constructor(public budget = DEFAULT_BUDGET) {}

  get size() {
    return this.list.length;
  }

  infos(): CheckpointInfo[] {
    return this.list.map(({ bytes: _bytes, ...info }) => info);
  }

  get(id: number): Checkpoint | undefined {
    return this.list.find((c) => c.id === id);
  }

  latest(): Checkpoint | undefined {
    return this.list[this.list.length - 1];
  }

  /** The latest checkpoint taken before chronicle event `index` happened. */
  before(index: number): Checkpoint | undefined {
    let found: Checkpoint | undefined;
    for (const c of this.list) if (c.events <= index) found = c;
    return found;
  }

  add(year: number, events: number, reason: CheckpointReason, bytes: Uint8Array): CheckpointInfo {
    const cp: Checkpoint = { id: this.nextId++, year, events, reason, size: bytes.byteLength, bytes };
    let i = this.list.length;
    while (i > 0 && this.list[i - 1].year > year) i--;
    this.list.splice(i, 0, cp);
    this.total += cp.size;
    this.thin(Math.max(year, this.list[this.list.length - 1].year));
    return cp;
  }

  /** Drops checkpoints until the total is within the budget; `now` is the current year. */
  thin(now: number) {
    while (this.total > this.budget && this.list.length > 1) {
      let drop = 0;
      if (this.list.length > 2) {
        let best = Infinity;
        for (let i = 1; i < this.list.length - 1; i++) {
          const c = this.list[i];
          const gap = this.list[i + 1].year - this.list[i - 1].year;
          const score = (gap / (now - c.year + 1)) * (c.reason === "intervention" ? 2 : 1);
          if (score < best) {
            best = score;
            drop = i;
          }
        }
      }
      this.total -= this.list[drop].size;
      this.list.splice(drop, 1);
    }
  }

  /** Discards the history after checkpoint `id` (a rewind to it). */
  truncateAfter(id: number) {
    const i = this.list.findIndex((c) => c.id === id);
    if (i < 0) return;
    for (const c of this.list.splice(i + 1)) this.total -= c.size;
  }

  clear() {
    this.list = [];
    this.total = 0;
  }
}

async function pipe(bytes: Uint8Array, stream: TransformStream<Uint8Array, Uint8Array>): Promise<Uint8Array> {
  const out = new Response(new Blob([bytes]).stream().pipeThrough(stream));
  return new Uint8Array(await out.arrayBuffer());
}

/** Gzips a save; without CompressionStream the bytes are kept as they are. */
export function compress(bytes: Uint8Array): Promise<Uint8Array> {
  if (typeof CompressionStream === "undefined") return Promise.resolve(bytes);
  return pipe(bytes, new CompressionStream("gzip"));
}

/** Undoes `compress`. Saves start with "EVO", gzip with 0x1f 0x8b. */
export function decompress(bytes: Uint8Array): Promise<Uint8Array> {
  if (bytes[0] !== 0x1f || bytes[1] !== 0x8b) return Promise.resolve(bytes);
  return pipe(bytes, new DecompressionStream("gzip"));
}
