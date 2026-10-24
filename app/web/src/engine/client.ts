import type { FromWorker, ToWorker } from "./protocol";

/**
 * Thin wrapper around the engine worker. The worker starts on first use and stops on
 * `terminate()`, so a client React creates and throws away (StrictMode renders twice
 * in development) never starts one, and a terminated client starts afresh if used again.
 */
export class EngineClient {
  private worker: Worker | null = null;
  private listeners = new Set<(m: FromWorker) => void>();

  private start(): Worker {
    if (this.worker) return this.worker;
    const w = new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });
    w.onmessage = (e: MessageEvent<FromWorker>) => {
      for (const l of this.listeners) l(e.data);
    };
    w.onerror = (e) => {
      for (const l of this.listeners) l({ type: "error", message: e.message || "Engine worker failed to start" });
    };
    this.worker = w;
    return w;
  }

  send(msg: ToWorker, transfer: Transferable[] = []) {
    this.start().postMessage(msg, transfer);
  }

  on(fn: (m: FromWorker) => void): () => void {
    this.start();
    this.listeners.add(fn);
    return () => {
      this.listeners.delete(fn);
    };
  }

  /** Stops the worker and the simulation in it. */
  terminate() {
    this.worker?.terminate();
    this.worker = null;
  }
}
