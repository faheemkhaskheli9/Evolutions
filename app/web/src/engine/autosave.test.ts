import "fake-indexeddb/auto";
import { describe, expect, it } from "vitest";
import { Autosaver, readAutosave, writeAutosave, type Slot } from "./autosave";
import type { AutosaveInfo } from "./protocol";

const info = (year: number): AutosaveInfo => ({ template: "earth", seed: 1, year, savedAt: 1000 + year });
const bytes = (...v: number[]) => new Uint8Array(v).buffer;

describe("IndexedDB slot", () => {
  it("is empty at first, then holds the latest world", async () => {
    expect(await readAutosave()).toBeUndefined();
    await writeAutosave(info(10), bytes(1, 2, 3));
    await writeAutosave(info(20), bytes(4, 5));
    const slot = await readAutosave();
    expect(slot?.info).toEqual(info(20));
    expect(Array.from(new Uint8Array(slot!.bytes))).toEqual([4, 5]);
  });
});

/** A write the test finishes by hand. */
function controlledWriter() {
  const writes: { info: AutosaveInfo; done: () => void; fail: () => void }[] = [];
  const write = (i: AutosaveInfo) =>
    new Promise<void>((resolve, reject) => writes.push({ info: i, done: resolve, fail: () => reject(new Error("quota")) }));
  return { writes, write };
}

describe("Autosaver", () => {
  let year = 0;
  const world = (): Slot => ({ info: info(++year), bytes: bytes(year) });

  it("writes only after the world changed", async () => {
    const { writes, write } = controlledWriter();
    const a = new Autosaver(write, world);
    void a.save();
    expect(writes).toHaveLength(0);
    a.dirty = true;
    const p = a.save();
    expect(writes).toHaveLength(1);
    expect(a.dirty).toBe(false);
    writes[0].done();
    await p;
    void a.save();
    expect(writes).toHaveLength(1);
  });

  it("writes nothing when there is no world", async () => {
    const { writes, write } = controlledWriter();
    const a = new Autosaver(write, () => null);
    a.dirty = true;
    await a.save();
    expect(writes).toHaveLength(0);
  });

  // Phase 5 web bug 9: a new world could go unsaved for a minute if an older save was
  // still being written.
  it("saves a change made during a write once that write finishes", async () => {
    const { writes, write } = controlledWriter();
    const a = new Autosaver(write, world);
    a.dirty = true;
    void a.save();
    a.dirty = true; // a new world is created while the first write runs
    const queued = a.save();
    expect(writes).toHaveLength(1);
    writes[0].done();
    await Promise.resolve();
    await new Promise((r) => setTimeout(r));
    expect(writes).toHaveLength(2);
    expect(writes[1].info.year).toBeGreaterThan(writes[0].info.year);
    writes[1].done();
    await queued;
    expect(a.dirty).toBe(false);
  });

  it("ignores write errors and keeps saving later changes", async () => {
    const { writes, write } = controlledWriter();
    const a = new Autosaver(write, world);
    a.dirty = true;
    const p = a.save();
    writes[0].fail();
    await expect(p).resolves.toBeUndefined();
    a.dirty = true;
    void a.save();
    expect(writes).toHaveLength(2);
  });
});
