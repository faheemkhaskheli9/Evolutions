// Autosave of the current world in IndexedDB (PRD FR-P1). One slot: the latest world.

import type { AutosaveInfo } from "./protocol";

const DB = "evolutions";
const STORE = "autosave";
const KEY = "latest";

interface Slot {
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
