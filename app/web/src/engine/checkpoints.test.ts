import { describe, expect, it } from "vitest";
import { CheckpointStore, compress, decompress } from "./checkpoints";

const MB = 1024 * 1024;
const bytes = (n: number) => new Uint8Array(n);

describe("CheckpointStore", () => {
  it("stays within the budget through a 10,000-year run, keeping recent history dense", () => {
    const store = new CheckpointStore(200 * MB);
    let peak = 0;
    for (let year = 0; year <= 10_000; year += 10) {
      // Saves grow as the world fills up: 3–8 MB compressed.
      store.add(year, year / 10, year % 1000 === 0 ? "jump" : "periodic", bytes(Math.round((3 + (5 * year) / 10_000) * MB)));
      peak = Math.max(peak, store.total);
    }
    expect(peak).toBeLessThanOrEqual(200 * MB);
    const years = store.infos().map((c) => c.year);
    expect(years[0]).toBe(0);
    expect(years[years.length - 1]).toBe(10_000);
    expect(store.total).toBe(store.infos().reduce((s, c) => s + c.size, 0));
    // The last 1,000 years hold more checkpoints than the first 5,000.
    expect(years.filter((y) => y >= 9000).length).toBeGreaterThan(years.filter((y) => y < 5000).length);
  });

  it("prefers to keep checkpoints taken before an intervention", () => {
    const store = new CheckpointStore(10 * MB);
    for (let year = 0; year <= 100; year += 10) store.add(year, 0, year === 30 ? "intervention" : "periodic", bytes(MB));
    expect(store.size).toBe(10);
    expect(store.infos().some((c) => c.year === 30)).toBe(true);
  });

  it("keeps the newest even when it alone is over budget", () => {
    const store = new CheckpointStore(MB);
    store.add(0, 0, "start", bytes(MB));
    store.add(5, 0, "periodic", bytes(2 * MB));
    expect(store.infos().map((c) => c.year)).toEqual([5]);
    store.add(6, 0, "periodic", bytes(MB / 2));
    expect(store.infos().map((c) => c.year)).toEqual([6]);
  });

  it("finds the checkpoint just before an event and truncates on rewind", () => {
    const store = new CheckpointStore();
    const a = store.add(0, 0, "start", bytes(10));
    const b = store.add(10, 3, "periodic", bytes(10));
    store.add(12, 3, "intervention", bytes(10));
    store.add(20, 7, "periodic", bytes(10));
    // Event 3 happened after the checkpoints that had seen 3 events.
    expect(store.before(3)?.year).toBe(12);
    expect(store.before(2)?.id).toBe(a.id);
    expect(store.before(6)?.year).toBe(12);
    store.truncateAfter(b.id);
    expect(store.infos().map((c) => c.year)).toEqual([0, 10]);
    expect(store.total).toBe(20);
    expect(store.latest()?.id).toBe(b.id);
  });

  it("orders checkpoints by time", () => {
    const store = new CheckpointStore();
    store.add(5, 0, "periodic", bytes(1));
    store.add(2, 0, "periodic", bytes(1));
    expect(store.infos().map((c) => c.year)).toEqual([2, 5]);
  });
});

describe("compress", () => {
  it("round-trips a save", async () => {
    const save = new TextEncoder().encode("EVO1" + "x".repeat(10_000));
    const packed = await compress(save);
    expect(packed.byteLength).toBeLessThan(save.byteLength / 10);
    expect(await decompress(packed)).toEqual(save);
  });

  it("passes uncompressed saves through", async () => {
    const save = new TextEncoder().encode("EVO1abc");
    expect(await decompress(save)).toEqual(save);
  });
});
