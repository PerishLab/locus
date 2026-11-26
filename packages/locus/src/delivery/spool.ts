import type { Diagnostic, Entry, Payload, Segment } from "../types.js";
import { measure } from "../wire/encode.js";

const store = "segments";
const maxlines = 64;
const maxbytes = 64 * 1024;
const maxage = 30_000;
const maxtotal = 2 * 1024 * 1024;
const maxsegments = 32;
const ttl = 24 * 60 * 60 * 1000;

type Pruned = {
  keep: Segment[];
  expired: number;
  rotated: number;
};

type Spec = {
  producer: string;
  session: string;
  factory: IDBFactory | null;
  notify: (diagnostic: Diagnostic) => void;
};

function connect(factory: IDBFactory, name: string): Promise<IDBDatabase> {
  return new Promise<IDBDatabase>((resolve, reject) => {
    const req = factory.open(name, 1);
    req.onupgradeneeded = () => {
      const db = req.result;
      if (!db.objectStoreNames.contains(store)) {
        db.createObjectStore(store, { keyPath: "id", autoIncrement: true });
      }
    };
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error ?? new Error("open failed"));
  });
}

function transact<T>(
  db: IDBDatabase,
  work: (object: IDBObjectStore, items: Segment[]) => T,
): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    let outcome: T | undefined;
    let done = false;
    const tx = db.transaction(store, "readwrite");
    const object = tx.objectStore(store);
    const req = object.getAll() as IDBRequest<Segment[]>;
    req.onsuccess = () => {
      try {
        const items = [...req.result].sort((a, b) => (a.id ?? 0) - (b.id ?? 0));
        outcome = work(object, items);
        done = true;
      } catch (reason) {
        reject(reason);
      }
    };
    req.onerror = () => reject(req.error ?? new Error("indexeddb read error"));
    tx.oncomplete = () => {
      if (done) resolve(outcome as T);
    };
    tx.onerror = () => reject(tx.error ?? new Error("transaction error"));
    tx.onabort = () => reject(tx.error ?? new Error("transaction aborted"));
  });
}

function fits(seg: Segment, bytes: number, now: number): boolean {
  if (seg.sealed) return false;
  if (seg.count + 1 > maxlines || seg.bytes + bytes > maxbytes) return false;
  return now - seg.created < maxage;
}

function ready(seg: Segment, seal: boolean, now: number): boolean {
  return seg.sealed || seal || now - seg.created >= maxage;
}

function prune(object: IDBObjectStore, all: Segment[], now: number): Pruned {
  const keep: Segment[] = [];
  let expired = 0;
  for (const seg of all) {
    if (now - seg.created > ttl && seg.id !== undefined) {
      object.delete(seg.id);
      expired += 1;
    } else {
      keep.push(seg);
    }
  }
  let bytes = keep.reduce((sum, seg) => sum + seg.bytes, 0);
  let rotated = 0;
  while ((keep.length > maxsegments || bytes > maxtotal) && keep.length > 0) {
    const oldest = keep.shift();
    if (oldest && oldest.id !== undefined) {
      object.delete(oldest.id);
      bytes -= oldest.bytes;
      rotated += 1;
    }
  }
  return { keep, expired, rotated };
}

function trim(
  object: IDBObjectStore,
  keep: Segment[],
  extra: { bytes: number; added: number },
): number {
  let bytes = keep.reduce((sum, seg) => sum + seg.bytes, 0) + extra.bytes;
  let total = keep.length + extra.added;
  let rotated = 0;
  while ((total > maxsegments || bytes > maxtotal) && keep.length > 0) {
    const oldest = keep.shift();
    if (oldest && oldest.id !== undefined) {
      object.delete(oldest.id);
      bytes -= oldest.bytes;
      total -= 1;
      rotated += 1;
    }
  }
  return rotated;
}

function stamp(seg: Segment, session: string, seq?: number): void {
  if (seq === undefined) return;
  seg.marks = { ...(seg.marks ?? {}), [session]: seq };
}

export class Spool {
  private readonly factory: IDBFactory | null;
  private readonly name: string;
  private readonly session: string;
  private readonly notify: (diagnostic: Diagnostic) => void;
  private db: IDBDatabase | null = null;
  private broken = false;
  private queue: Promise<unknown> = Promise.resolve();

  constructor(spec: Spec) {
    this.name = "locus:spool:" + spec.producer;
    this.session = spec.session;
    this.factory = spec.factory;
    this.notify = spec.notify;
  }

  available(): boolean {
    return this.factory !== null && !this.broken;
  }

  private serialize<T>(work: () => Promise<T>): Promise<T> {
    const next = this.queue.then(work, work);
    this.queue = next.then(
      () => undefined,
      () => undefined,
    );
    return next;
  }

  private async open(): Promise<IDBDatabase | null> {
    if (!this.available() || !this.factory) return null;
    if (this.db) return this.db;
    try {
      this.db = await connect(this.factory, this.name);
      return this.db;
    } catch (reason) {
      this.broken = true;
      this.notify({
        code: "spool.unavailable",
        message: reason instanceof Error ? reason.message : "indexeddb unavailable",
      });
      return null;
    }
  }

  private announce(expired: number, rotated: number): void {
    if (expired > 0) {
      this.notify({ code: "spool.expired", message: `purged ${expired} expired segments` });
    }
    if (rotated > 0) {
      this.notify({ code: "spool.rotated", message: `rotated ${rotated} oldest segments` });
    }
  }

  private async write(db: IDBDatabase, entry: Entry, now: number): Promise<void> {
    const counts = await transact(db, (object, all) => {
      const pruned = prune(object, all, now);
      const last = pruned.keep[pruned.keep.length - 1];
      let rotated = pruned.rotated;
      if (last && fits(last, entry.bytes, now)) {
        last.lines.push(entry.line);
        last.bytes += entry.bytes;
        last.count += 1;
        stamp(last, this.session, entry.seq);
        if (last.count >= maxlines || last.bytes >= maxbytes) last.sealed = true;
        object.put(last);
        rotated += trim(object, pruned.keep.slice(0, -1), { bytes: last.bytes, added: 1 });
      } else {
        if (last && !last.sealed) {
          last.sealed = true;
          object.put(last);
        }
        const marks = entry.seq !== undefined ? { [this.session]: entry.seq } : undefined;
        object.add({
          created: now,
          bytes: entry.bytes,
          count: 1,
          lines: [entry.line],
          sealed: entry.bytes >= maxbytes,
          marks,
        });
        rotated += trim(object, pruned.keep, { bytes: entry.bytes, added: 1 });
      }
      return { expired: pruned.expired, rotated };
    });
    this.announce(counts.expired, counts.rotated);
  }

  append(entry: Entry, now = Date.now()): Promise<boolean> {
    return this.serialize(async () => {
      const db = await this.open();
      if (!db) return false;
      try {
        await this.write(db, entry, now);
        return true;
      } catch (reason) {
        this.notify({
          code: "spool.write",
          message: reason instanceof Error ? reason.message : "spool append failed",
        });
        return false;
      }
    });
  }

  private select(
    object: IDBObjectStore,
    all: Segment[],
    seal: boolean,
    now: number,
  ): { payload: Payload | null; expired: number; rotated: number } {
    const pruned = prune(object, all, now);
    const target = pruned.keep.find((seg) => ready(seg, seal, now));
    if (!target || target.id === undefined) {
      return { payload: null, expired: pruned.expired, rotated: pruned.rotated };
    }
    if (!target.sealed) {
      target.sealed = true;
      object.put(target);
    }
    const payload: Payload = {
      raw: measure(target.lines.join("")),
      lines: target.count,
      segment: target.id,
      until: target.marks?.[this.session],
      origin: "spool",
    };
    return { payload, expired: pruned.expired, rotated: pruned.rotated };
  }

  next(seal: boolean, now = Date.now()): Promise<Payload | null> {
    return this.serialize(async () => {
      const db = await this.open();
      if (!db) return null;
      try {
        const result = await transact(db, (object, all) => this.select(object, all, seal, now));
        this.announce(result.expired, result.rotated);
        return result.payload;
      } catch {
        return null;
      }
    });
  }

  remove(id: number): Promise<void> {
    return this.serialize(async () => {
      const db = await this.open();
      if (!db) return;
      try {
        await transact(db, (object) => {
          object.delete(id);
        });
      } catch {
        return;
      }
    });
  }

  settle(): Promise<void> {
    return this.queue.then(
      () => undefined,
      () => undefined,
    );
  }

  close(): Promise<void> {
    return this.serialize(async () => {
      if (this.db) {
        this.db.close();
        this.db = null;
      }
    });
  }
}
