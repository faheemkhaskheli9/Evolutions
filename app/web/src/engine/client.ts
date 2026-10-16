import type { FromWorker, ToWorker } from "./protocol";

/** Thin wrapper around the engine worker. */
export class EngineClient {
  private worker: Worker;
  private listeners = new Set<(m: FromWorker) => void>();

  constructor() {
    this.worker = new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });
    this.worker.onmessage = (e: MessageEvent<FromWorker>) => {
      for (const l of this.listeners) l(e.data);
    };
    this.worker.onerror = (e) => {
      for (const l of this.listeners) l({ type: "error", message: e.message || "Engine worker failed to start" });
    };
  }

  send(msg: ToWorker, transfer: Transferable[] = []) {
    this.worker.postMessage(msg, transfer);
  }

  on(fn: (m: FromWorker) => void): () => void {
    this.listeners.add(fn);
    return () => {
      this.listeners.delete(fn);
    };
  }
}
