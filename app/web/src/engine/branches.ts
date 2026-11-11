// Branch timelines (PRD FR-T7): alternative histories that share their past. Only the
// branch on screen is a live simulation; the others are kept as saves of
// where they were left (their head) plus their own checkpoints.

import { CheckpointStore } from "./checkpoints";
import type { SimEvent, SpeciesInfo } from "./protocol";

/** A branch as the UI sees it. */
export interface BranchInfo {
  id: number;
  name: string;
  /** The branch it was split from (null for the first). */
  parent: number | null;
  /** The year it was split from its parent. */
  fromYear: number;
  /** Where it was left (the live year for the active branch). */
  year: number;
  active: boolean;
}

export interface Branch extends Omit<BranchInfo, "active"> {
  store: CheckpointStore;
  /** Save of where it was left (a checkpoint's compressed bytes, or an uncompressed save so switching back is quick); null while it is the live branch. */
  head: Uint8Array | null;
}

/** At most this many branches are kept (each holds its own checkpoints). */
export const MAX_BRANCHES = 8;

export class BranchSet {
  private list: Branch[] = [];
  private nextId = 1;
  activeId = 0;

  constructor() {
    this.reset(new CheckpointStore(), 0);
  }

  /** Starts over with one branch: a new, opened or replayed world. */
  reset(store: CheckpointStore, year: number) {
    this.nextId = 1;
    const main: Branch = { id: this.nextId++, name: "Main", parent: null, fromYear: year, year, store, head: null };
    this.list = [main];
    this.activeId = main.id;
  }

  get size() {
    return this.list.length;
  }

  get active(): Branch {
    return this.list.find((b) => b.id === this.activeId)!;
  }

  get(id: number): Branch | undefined {
    return this.list.find((b) => b.id === id);
  }

  infos(): BranchInfo[] {
    return this.list.map(({ store: _s, head: _h, ...b }) => ({ ...b, active: b.id === this.activeId }));
  }

  /** The first free "Branch N" name. */
  nextName(): string {
    for (let n = 2; ; n++) {
      const name = `Branch ${n}`;
      if (!this.list.some((b) => b.name === name)) return name;
    }
  }

  /** Adds a branch split from the active one in `year`; it does not become active. */
  add(fromYear: number, store: CheckpointStore, head: Uint8Array | null, name = this.nextName()): Branch {
    if (this.list.length >= MAX_BRANCHES) throw new Error(`At most ${MAX_BRANCHES} branches are kept; remove one first.`);
    const b: Branch = { id: this.nextId++, name, parent: this.activeId, fromYear, year: fromYear, store, head };
    this.list.push(b);
    return b;
  }

  rename(id: number, name: string) {
    const b = this.get(id);
    const clean = name.trim().slice(0, 40);
    if (b && clean) b.name = clean;
  }

  /** Removes a branch other than the active one; its children are re-attached to its parent. */
  remove(id: number): boolean {
    const b = this.get(id);
    if (!b || id === this.activeId) return false;
    for (const c of this.list) if (c.parent === id) c.parent = b.parent;
    this.list = this.list.filter((x) => x !== b);
    return true;
  }
}

/** A branch's history, for the comparison view. */
export interface BranchHistory {
  id: number;
  name: string;
  year: number;
  species: SpeciesInfo[];
  events: SimEvent[];
}

/** Species present in both histories: the same id and name (they share the past up to the branch point). */
export function sharedSpecies(a: BranchHistory, b: BranchHistory): SpeciesInfo[] {
  return a.species.filter((s) => b.species.some((t) => t.id === s.id && t.name === s.name));
}

/** The kinds of event the comparison lists. */
export const COMPARED_KINDS = new Set(["Speciation", "Extinction", "Intervention", "Founders", "Crash"]);

const sameEvent = (x: SimEvent, y: SimEvent) => x.year === y.year && x.kind === y.kind && x.text === y.text && x.species === y.species;

/**
 * Splits two chronicles into their shared start and what happened only in each
 * afterwards. Only speciations, extinctions, interventions, introductions and crashes
 * count, so a mode switch in one branch does not end the shared part. After the shared
 * start, an event both branches have (the same year, kind and text) is not listed.
 */
export function diffEvents(a: SimEvent[], b: SimEvent[]): { shared: number; onlyA: SimEvent[]; onlyB: SimEvent[] } {
  const fa = a.filter((e) => COMPARED_KINDS.has(e.kind));
  const fb = b.filter((e) => COMPARED_KINDS.has(e.kind));
  let k = 0;
  while (k < fa.length && k < fb.length && sameEvent(fa[k], fb[k])) k++;
  const restB = fb.slice(k);
  const matched = new Set<number>();
  const onlyA = fa.slice(k).filter((e) => {
    const j = restB.findIndex((x, i) => !matched.has(i) && sameEvent(e, x));
    if (j < 0) return true;
    matched.add(j);
    return false;
  });
  return { shared: k, onlyA, onlyB: restB.filter((_, i) => !matched.has(i)) };
}
/**
 * The year up to which branches `a` and `b` share their history: where the earlier of
 * the two paths from their common ancestor leaves it. Null if they are not related.
 */
export function splitYear(branches: Pick<BranchInfo, "id" | "parent" | "fromYear">[], a: number, b: number): number | null {
  const byId = new Map(branches.map((x) => [x.id, x]));
  // Each ancestor of `a` (itself included), with the year a's path leaves it.
  const leave = new Map<number, number>();
  let year = Infinity;
  for (let x = byId.get(a); x; x = x.parent === null ? undefined : byId.get(x.parent)) {
    leave.set(x.id, year);
    year = x.fromYear;
  }
  year = Infinity;
  for (let x = byId.get(b); x; x = x.parent === null ? undefined : byId.get(x.parent)) {
    const fromA = leave.get(x.id);
    if (fromA !== undefined) {
      const y = Math.min(fromA, year);
      return isFinite(y) ? y : null;
    }
    year = x.fromYear;
  }
  return null;
}
