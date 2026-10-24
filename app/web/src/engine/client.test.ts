import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { EngineClient } from "./client";
import type { FromWorker } from "./protocol";

/** Stand-in for a module Worker: records messages and lets a test reply. */
class FakeWorker {
  static all: FakeWorker[] = [];
  sent: unknown[] = [];
  terminated = false;
  onmessage: ((e: MessageEvent<FromWorker>) => void) | null = null;
  onerror: ((e: { message: string }) => void) | null = null;
  constructor() {
    FakeWorker.all.push(this);
  }
  postMessage(msg: unknown) {
    this.sent.push(msg);
  }
  terminate() {
    this.terminated = true;
  }
  reply(data: FromWorker) {
    this.onmessage?.({ data } as MessageEvent<FromWorker>);
  }
}

beforeEach(() => {
  FakeWorker.all = [];
  vi.stubGlobal("Worker", FakeWorker);
});
afterEach(() => vi.unstubAllGlobals());

describe("EngineClient", () => {
  // StrictMode builds and throws away a client in development; it must not start a worker.
  it("starts no worker until it is used", () => {
    new EngineClient();
    expect(FakeWorker.all).toHaveLength(0);
  });

  it("starts one worker on first use and reuses it", () => {
    const c = new EngineClient();
    c.send({ type: "play" });
    c.send({ type: "pause" });
    c.on(() => {});
    expect(FakeWorker.all).toHaveLength(1);
    expect(FakeWorker.all[0].sent).toEqual([{ type: "play" }, { type: "pause" }]);
  });

  it("delivers replies to every listener until it unsubscribes", () => {
    const c = new EngineClient();
    const a: FromWorker[] = [];
    const b: FromWorker[] = [];
    const offA = c.on((m) => a.push(m));
    c.on((m) => b.push(m));
    const w = FakeWorker.all[0];
    w.reply({ type: "jumpDone" });
    offA();
    w.reply({ type: "notice", message: "hi" });
    expect(a).toEqual([{ type: "jumpDone" }]);
    expect(b).toEqual([{ type: "jumpDone" }, { type: "notice", message: "hi" }]);
  });

  it("reports a worker that fails to start as an error message", () => {
    const c = new EngineClient();
    const got: FromWorker[] = [];
    c.on((m) => got.push(m));
    FakeWorker.all[0].onerror?.({ message: "" });
    expect(got).toEqual([{ type: "error", message: "Engine worker failed to start" }]);
  });

  it("terminate stops the worker, and later use starts a fresh one", () => {
    const c = new EngineClient();
    const got: FromWorker[] = [];
    c.on((m) => got.push(m));
    const first = FakeWorker.all[0];
    c.terminate();
    expect(first.terminated).toBe(true);
    c.terminate(); // a second call is harmless
    c.send({ type: "play" });
    expect(FakeWorker.all).toHaveLength(2);
    const second = FakeWorker.all[1];
    expect(second.sent).toEqual([{ type: "play" }]);
    // Listeners carry over to the new worker.
    second.reply({ type: "jumpDone" });
    expect(got).toEqual([{ type: "jumpDone" }]);
  });

  it("terminating a client that never started is a no-op", () => {
    new EngineClient().terminate();
    expect(FakeWorker.all).toHaveLength(0);
  });
});
