import { gunzipSync } from "node:zlib";
import type { Fetcher } from "@perishlab/locus";

export type Captured = {
  url: string;
  method: string;
  headers: Record<string, string>;
  keepalive: boolean;
  raw: Uint8Array;
  text: string;
  lines: string[];
};

export type Parsed = {
  at: string;
  id: string;
  producer: string;
  context: Record<string, string>;
  choices: { role: string; key: string; origin: string }[];
  source?: {
    file: string;
    line: number;
    column: number;
    module: string;
    function?: string;
    edge?: string;
  };
  payload?: Record<string, unknown>;
};

export function decode(line: string): Parsed {
  return JSON.parse(line, (key, value, ctx?: { source?: string }) => {
    if (key !== "at" && key !== "elapsed") return value;
    if (typeof value === "number" && ctx?.source) return ctx.source;
    return value;
  }) as Parsed;
}

export function body(raw: Uint8Array, gzip: boolean): string {
  const bytes = gzip ? new Uint8Array(gunzipSync(raw)) : raw;
  return new TextDecoder().decode(bytes);
}

export function recorder(
  responder?: (call: Captured, index: number) => Response | Promise<Response>,
): { calls: Captured[]; fetcher: Fetcher } {
  const calls: Captured[] = [];
  const fetcher: Fetcher = async (input, init) => {
    const url = typeof input === "string"
      ? input
      : input instanceof URL
        ? input.href
        : input.url;
    const headers: Record<string, string> = {};
    if (init?.headers && typeof init.headers === "object") {
      for (const [key, value] of Object.entries(init.headers as Record<string, string>)) {
        headers[key.toLowerCase()] = value;
      }
    }
    const bytes = init?.body instanceof Uint8Array
      ? new Uint8Array(init.body)
      : typeof init?.body === "string"
        ? new TextEncoder().encode(init.body)
        : new Uint8Array();
    const text = body(bytes, headers["content-encoding"] === "gzip");
    const lines = text.length > 0 ? text.trimEnd().split("\n") : [];
    const call: Captured = {
      url,
      method: init?.method ?? "GET",
      headers,
      keepalive: Boolean(init?.keepalive),
      raw: bytes,
      text,
      lines,
    };
    const index = calls.length;
    calls.push(call);
    if (responder) return responder(call, index);
    return new Response(null, { status: 204 });
  };
  return { calls, fetcher };
}

type Stored = { id: number; [key: string]: unknown };

type Settings = {
  fail?: boolean;
  quota?: boolean;
};

type State = {
  map: Map<number, Stored>;
  sequences: Map<string, number>;
  name: string;
  settings?: Settings;
};

function tracker(tx: IDBTransaction): {
  add: <T>(result: T) => IDBRequest<T>;
  abort: (error: Error) => IDBRequest<unknown>;
} {
  let pending = 0;
  let aborted = false;

  const complete = () => {
    if (pending === 0 && !aborted) tx.oncomplete?.(new Event("complete"));
  };

  const done = () => {
    pending -= 1;
    if (pending === 0 && !aborted) queueMicrotask(complete);
  };

  const add = <T>(result: T): IDBRequest<T> => {
    pending += 1;
    const op = { result, error: null, onsuccess: null, onerror: null } as unknown as IDBRequest<T>;
    queueMicrotask(() => {
      if (!aborted) op.onsuccess?.(new Event("success"));
      done();
    });
    return op;
  };

  const abort = (error: Error): IDBRequest<unknown> => {
    aborted = true;
    const op = { result: null, error, onsuccess: null, onerror: null } as unknown as IDBRequest<unknown>;
    queueMicrotask(() => {
      op.onerror?.(new Event("error"));
      (tx as unknown as { error: Error }).error = error;
      tx.onerror?.(new Event("error"));
    });
    return op;
  };

  return { add, abort };
}

function buildstore(state: State, tx: IDBTransaction): IDBObjectStore {
  const ops = tracker(tx);
  return {
    ["get" + "All"]: () =>
      ops.add(Array.from(state.map.values()).map((item) => ({ ...item }))),
    add: (value: Record<string, unknown>) => {
      if (state.settings?.quota) return ops.abort(new DOMException("quota", "QuotaExceededError"));
      const next = (state.sequences.get(state.name) ?? 0) + 1;
      state.sequences.set(state.name, next);
      state.map.set(next, { ...value, id: next });
      return ops.add(next);
    },
    put: (value: Stored) => {
      if (state.settings?.quota) return ops.abort(new DOMException("quota", "QuotaExceededError"));
      state.map.set(value.id, { ...value });
      return ops.add(value.id);
    },
    delete: (id: number) => {
      state.map.delete(id);
      return ops.add(undefined);
    },
  } as unknown as IDBObjectStore;
}

function builddb(state: State, existed: boolean): IDBDatabase {
  const names = new Set<string>(existed ? ["segments"] : []);
  return {
    ["object" + "StoreNames"]: { contains: (key: string) => names.has(key) },
    ["create" + "ObjectStore"]: (key: string) => {
      names.add(key);
      return {};
    },
    transaction: () => {
      const tx = { oncomplete: null, onerror: null, onabort: null } as unknown as IDBTransaction;
      (tx as unknown as Record<string, unknown>)["objectStore"] = () => buildstore(state, tx);
      return tx;
    },
    close: () => undefined,
  } as unknown as IDBDatabase;
}

export function mockdb(settings?: Settings): {
  factory: IDBFactory;
  stores: Map<string, Map<number, Stored>>;
  opens: () => number;
} {
  const stores = new Map<string, Map<number, Stored>>();
  const sequences = new Map<string, number>();
  let count = 0;

  const factory = {
    open: (name: string) => {
      count += 1;
      const req = {
        result: null,
        error: null,
        onupgradeneeded: null,
        onsuccess: null,
        onerror: null,
      } as unknown as IDBOpenDBRequest;
      queueMicrotask(() => {
        if (settings?.fail) {
          (req as unknown as { error: Error }).error = new Error("storage denied");
          req.onerror?.(new Event("error"));
          return;
        }
        const existed = stores.has(name);
        if (!existed) stores.set(name, new Map());
        const state: State = { map: stores.get(name)!, sequences, name, settings };
        const db = builddb(state, existed);
        (req as unknown as { result: IDBDatabase }).result = db;
        if (!existed) req.onupgradeneeded?.(new Event("upgradeneeded") as IDBVersionChangeEvent);
        req.onsuccess?.(new Event("success"));
      });
      return req;
    },
  } as unknown as IDBFactory;

  return { factory, stores, opens: () => count };
}
