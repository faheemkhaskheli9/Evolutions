// Autosave of the current world in IndexedDB (PRD FR-P1). One slot: the latest world.

import type { AutosaveInfo } from "./protocol";

const DB = "evolutions";
const STORE = "autosave";
const KEY = "latest";

export interface Slot {
  info: AutosaveInfo;
  bytes: ArrayBuffer;
}

function open(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB, 1);
    req.onupgradeneeded = () => req.result.createObjectStore(STORE);
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

async function run<T>(mode: IDBTransactionMode, fn: (s: IDBObjectStore) => IDBRequest<T>): Promise<T> {
  const db = await open();
  try {
    return await new Promise<T>((resolve, reject) => {
      const tx = db.transaction(STORE, mode);
      const req = fn(tx.objectStore(STORE));
      tx.oncomplete = () => resolve(req.result);
      tx.onerror = tx.onabort = () => reject(tx.error ?? req.error);
    });
  } finally {
    db.close();
  }
}

export const writeAutosave = (info: AutosaveInfo, bytes: ArrayBuffer) => run("readwrite", (s) => s.put({ info, bytes } satisfies Slot, KEY));

export const readAutosave = () => run<Slot | undefined>("readonly", (s) => s.get(KEY));

/**
 * Decides when to write the autosave: only after the world changed (`dirty`), never two
 * writes at once, and a save asked for during a write runs when it finishes, so a new
 * world is not skipped. Write errors are ignored: autosave is best-effort (private
 * windows, quota).
 */
export class Autosaver {
  /** The world changed since the last autosave (time ran, an intervention, a new world). */
  dirty = false;
  private writing: Promise<void> | null = null;
  private pending = false;

  constructor(
    private write: (info: AutosaveInfo, bytes: ArrayBuffer) => Promise<unknown>,
    /** The world to save, or null when there is none. */
    private snapshot: () => Slot | null,
  ) {}

  /** Saves if the world changed. Resolves when this save (and any queued after it) is written. */
  save(): Promise<void> {
    if (!this.dirty) return this.writing ?? Promise.resolve();
    if (this.writing) {
      this.pending = true;
      return this.writing;
    }
    const slot = this.snapshot();
    if (!slot) return Promise.resolve();
    this.dirty = false;
    this.writing = this.write(slot.info, slot.bytes)
      .then(
        () => {},
        () => {},
      )
      .then(() => {
        this.writing = null;
        if (this.pending) {
          this.pending = false;
          return this.save();
        }
      });
    return this.writing;
  }
}
