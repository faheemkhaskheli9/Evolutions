import { describe, expect, it } from "vitest";
import { BranchSet, diffEvents, MAX_BRANCHES, sharedSpecies, splitYear, type BranchHistory } from "./branches";
import { CheckpointStore } from "./checkpoints";
import type { SimEvent, SpeciesInfo } from "./protocol";

const bytes = (n: number) => new Uint8Array(n);

describe("CheckpointStore.fork", () => {
  it("keeps the checkpoints up to the branch point and shares their bytes", () => {
    const s = new CheckpointStore();
    const a = s.add(0, 0, "start", bytes(10));
    const b = s.add(10, 1, "periodic", bytes(20));
    s.add(20, 2, "periodic", bytes(30));
    const f = s.fork(b.id);
    expect(f.infos().map((c) => c.year)).toEqual([0, 10]);
    expect(f.total).toBe(30);
    expect(f.get(a.id)!.bytes).toBe(s.get(a.id)!.bytes);
    // New checkpoints get ids that do not clash with the parent's.
    const c = f.add(15, 1, "intervention", bytes(5));
    expect(s.get(c.id)).toBeUndefined();
    // Changing the fork leaves the original alone.
    f.truncateAfter(a.id);
    expect(s.size).toBe(3);
    expect(s.fork().size).toBe(3);
  });
});

describe("BranchSet", () => {
  it("adds, names, renames and removes branches", () => {
    const set = new BranchSet();
    set.reset(new CheckpointStore(), 0);
    expect(set.infos()).toEqual([{ id: 1, name: "Main", parent: null, fromYear: 0, year: 0, active: true }]);
    const b = set.add(50, new CheckpointStore(), bytes(1));
    expect(b.name).toBe("Branch 2");
    expect(b.parent).toBe(1);
    set.activeId = b.id;
    const c = set.add(80, new CheckpointStore(), bytes(1));
    expect(c.parent).toBe(b.id);
    set.rename(b.id, "  Wolves  ");
    expect(set.get(b.id)!.name).toBe("Wolves");
    set.rename(b.id, "   ");
    expect(set.get(b.id)!.name).toBe("Wolves");
    // The live branch cannot be removed; removing another re-attaches its children.
    expect(set.remove(b.id)).toBe(false);
    set.activeId = 1;
    expect(set.remove(b.id)).toBe(true);
    expect(set.get(c.id)!.parent).toBe(1);
    expect(set.nextName()).toBe("Branch 2");
  });

  it("keeps at most MAX_BRANCHES", () => {
    const set = new BranchSet();
    for (let i = 1; i < MAX_BRANCHES; i++) set.add(i, new CheckpointStore(), null);
    expect(() => set.add(99, new CheckpointStore(), null)).toThrow(/At most/);
  });
});

describe("splitYear", () => {
  const tree = [
    { id: 1, parent: null, fromYear: 0 },
    { id: 2, parent: 1, fromYear: 100 },
    { id: 3, parent: 1, fromYear: 50 },
    { id: 4, parent: 2, fromYear: 150 },
  ];
  it("is the branch point of a child and its parent", () => {
    expect(splitYear(tree, 1, 2)).toBe(100);
    expect(splitYear(tree, 2, 1)).toBe(100);
    expect(splitYear(tree, 2, 4)).toBe(150);
  });
  it("is the earlier branch point of two siblings", () => {
    expect(splitYear(tree, 2, 3)).toBe(50);
    expect(splitYear(tree, 4, 3)).toBe(50);
    expect(splitYear(tree, 1, 4)).toBe(100);
  });
  it("is null for a branch and itself", () => {
    expect(splitYear(tree, 2, 2)).toBeNull();
  });
});

const ev = (year: number, kind: SimEvent["kind"], text: string): SimEvent => ({ year, kind, text, species: null });
const sp = (id: number, name: string) => ({ id, name, common_name: name }) as SpeciesInfo;
const hist = (species: SpeciesInfo[], events: SimEvent[] = []): BranchHistory => ({ id: 1, name: "x", year: 0, species, events });

describe("diffEvents", () => {
  it("splits the shared start from what happened only in each", () => {
    const shared = [ev(0, "Founders", "deer"), ev(10, "Speciation", "A splits")];
    const a = [...shared, ev(12, "Mode", "Population mode"), ev(20, "Extinction", "B dies")];
    const b = [...shared, ev(15, "Intervention", "Meteor"), ev(20, "Speciation", "C splits"), ev(21, "Info", "note")];
    const d = diffEvents(a, b);
    expect(d.shared).toBe(2);
    expect(d.onlyA.map((e) => e.text)).toEqual(["B dies"]);
    expect(d.onlyB.map((e) => e.text)).toEqual(["Meteor", "C splits"]);
  });
  it("finds nothing different in identical histories", () => {
    const a = [ev(0, "Founders", "deer"), ev(5, "Crash", "crash")];
    expect(diffEvents(a, [...a])).toEqual({ shared: 2, onlyA: [], onlyB: [] });
  });
});

describe("sharedSpecies", () => {
  it("matches species by id and name", () => {
    const a = hist([sp(1, "Cervus"), sp(2, "Lupus"), sp(3, "Novus a")]);
    const b = hist([sp(1, "Cervus"), sp(3, "Novus b")]);
    expect(sharedSpecies(a, b).map((s) => s.id)).toEqual([1]);
  });
});
