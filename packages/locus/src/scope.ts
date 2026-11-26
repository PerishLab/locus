import type { AsyncLocalStorage } from "node:async_hooks";
import type { Frame } from "./types.js";

type Storage = AsyncLocalStorage<Frame>;

type Constructor = new () => Storage;

type Loader = (name: string) => unknown;

function specifier(): string {
  return ["node", "async" + "_" + "hooks"].join(":");
}

function node(): Record<string, unknown> | undefined {
  const candidate = (globalThis as Record<string, unknown>)["process"];
  if (typeof candidate !== "object" || candidate === null) return undefined;
  const record = candidate as Record<string, unknown>;
  const versions = record["versions"] as Record<string, unknown> | undefined;
  return versions && typeof versions["node"] === "string" ? record : undefined;
}

function constructor(mod: unknown): Constructor | undefined {
  if (typeof mod !== "object" || mod === null) return undefined;
  const candidate = (mod as Record<string, unknown>)["AsyncLocalStorage"];
  return typeof candidate === "function" ? (candidate as Constructor) : undefined;
}

function synchronous(runtime: Record<string, unknown>): Storage | undefined {
  const builtin = runtime["getBuiltinModule"];
  if (typeof builtin !== "function") return undefined;
  try {
    const ctor = constructor((builtin as Loader)(specifier()));
    return ctor ? new ctor() : undefined;
  } catch {
    return undefined;
  }
}

export class Scope {
  private readonly stack: Frame[] = [];
  private carrier: Storage | undefined;
  private loading = false;

  constructor(mode: "auto" | "stack" = "auto") {
    if (mode === "stack") return;
    const runtime = node();
    if (!runtime) return;
    this.carrier = synchronous(runtime);
    if (!this.carrier) this.load();
  }

  private load(): void {
    if (this.loading) return;
    this.loading = true;
    void import(specifier())
      .then((mod: unknown) => {
        const ctor = constructor(mod);
        if (ctor) this.carrier = new ctor();
      })
      .catch(() => undefined);
  }

  current(): Frame | undefined {
    if (this.carrier) {
      return this.carrier.getStore() ?? this.stack[this.stack.length - 1];
    }
    return this.stack[this.stack.length - 1];
  }

  run<Ret>(frame: Frame, fn: () => Ret): Ret {
    if (this.carrier) {
      return this.carrier.run(frame, fn);
    }
    this.stack.push(frame);
    try {
      return fn();
    } finally {
      this.stack.pop();
    }
  }
}
