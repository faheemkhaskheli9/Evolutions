// User templates (PRD FR-W9, FR-P3), kept in IndexedDB. The engine checks every file
// before it is stored, so what is kept here is always a valid template.

import type { StoredTemplate, UserTemplate } from "./protocol";

// A database of its own, so adding it does not change the autosave database's version.
const DB = "evolutions-templates";
const STORE = "templates";

function open(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB, 1);
    req.onupgradeneeded = () => req.result.createObjectStore(STORE, { keyPath: "id" });
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

/** The stored templates, newest first. */
export async function listTemplates(): Promise<StoredTemplate[]> {
  const all = await run<StoredTemplate[]>("readonly", (s) => s.getAll());
  return all.sort((a, b) => b.savedAt - a.savedAt);
}

/**
 * Stores a checked template (`text` is the engine's normal form). Storing the same file
 * again replaces the earlier copy instead of adding a duplicate.
 */
export async function storeTemplate(text: string, now = Date.now()): Promise<StoredTemplate> {
  const template = JSON.parse(text) as UserTemplate;
  const same = (await listTemplates()).find((t) => t.text === text);
  const entry: StoredTemplate = { id: same?.id ?? `${now}-${Math.random().toString(36).slice(2, 8)}`, savedAt: now, text, template };
  await run("readwrite", (s) => s.put(entry));
  return entry;
}

export const removeTemplate = (id: string) => run("readwrite", (s) => s.delete(id));

/** A file name for a template: its name, made safe for file systems. */
export function templateFileName(name: string): string {
  const base = name
    .normalize("NFKD")
    .replace(/[^\w\- ]+/g, "")
    .trim()
    .replace(/\s+/g, "-")
    .toLowerCase()
    .slice(0, 60);
  return `${base || "template"}.evotemplate`;
}
