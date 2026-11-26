import type { Clock } from "./clock.js";
import type { Scope } from "./scope.js";
import type { Apply, Choice, Edge, Frame, Wrapper } from "./types.js";

export type Schema = "locus.trace" | "locus.span";

export type Step = {
  at: bigint;
  frame: Frame;
  choices: Choice[];
  schema: Schema;
  name: string;
  edge: Edge;
  elapsed?: bigint;
  status?: "ok" | "error";
};

export type Runner = {
  active: boolean;
  clock: Clock;
  scope: Scope;
  emit: (step: Step) => void;
};

type Entered = {
  schema: Schema;
  name: string;
  frame: Frame;
  choices: Choice[];
  start: bigint;
};

function promise(value: unknown): value is PromiseLike<unknown> {
  if (value === null) return false;
  if (typeof value !== "object" && typeof value !== "function") return false;
  return typeof (value as { then?: unknown }).then === "function";
}

function method(value: unknown): value is ClassMethodDecoratorContext {
  return (
    typeof value === "object" &&
    value !== null &&
    (value as { kind?: unknown }).kind === "method"
  );
}

export class Binder {
  private readonly runner: Runner;

  constructor(runner: Runner) {
    this.runner = runner;
  }

  make(schema: Schema): Wrapper {
    const dispatch = (first: unknown, second?: unknown): unknown => {
      if (typeof first === "function" && method(second)) {
        const label = typeof second.name === "symbol"
          ? second.name.toString()
          : String(second.name || first.name || "anonymous");
        return this.wrap(schema, label, first as (...args: unknown[]) => unknown);
      }
      if (typeof first === "string" && second === undefined) {
        const apply: Apply = (target) => this.wrap(schema, first, target);
        return apply;
      }
      if (typeof first === "string" && typeof second === "function") {
        return this.wrap(schema, first, second as (...args: unknown[]) => unknown);
      }
      if (typeof first === "function") {
        const label = first.name || "anonymous";
        return this.wrap(schema, label, first as (...args: unknown[]) => unknown);
      }
      return first;
    };
    return dispatch as Wrapper;
  }

  private begin(schema: Schema, name: string): Entered | null {
    try {
      const prior = this.runner.scope.current();
      const trace = prior?.trace ?? this.runner.clock.trace();
      const span = this.runner.clock.span();
      const parent = prior?.parent;
      const frame: Frame = {
        trace,
        span,
        parent,
        values: { ...(prior?.values ?? {}) },
      };
      const choices: Choice[] = [
        { role: "locus.trace", key: trace, origin: prior?.trace ? "inherited" : "generated" },
        { role: "locus.span", key: span, origin: "generated" },
      ];
      if (parent) {
        choices.push({ role: "locus.parent", key: parent, origin: "inherited" });
      }
      const start = this.runner.clock.now();
      this.runner.emit({ at: start, frame, choices, schema, name, edge: "enter" });
      return { schema, name, frame, choices, start };
    } catch {
      return null;
    }
  }

  private complete(state: Entered | null, status: "ok" | "error"): void {
    if (!state) return;
    try {
      const end = this.runner.clock.now();
      const elapsed = end >= state.start ? end - state.start : 0n;
      this.runner.emit({
        at: end,
        frame: state.frame,
        choices: state.choices,
        schema: state.schema,
        name: state.name,
        edge: "return",
        elapsed,
        status,
      });
    } catch {
      return;
    }
  }

  private wrap<This, Args extends unknown[], Ret>(
    schema: Schema,
    name: string,
    target: (this: This, ...args: Args) => Ret,
  ): (this: This, ...args: Args) => Ret {
    const self = this;
    return function wrapped(this: This, ...args: Args): Ret {
      if (!self.runner.active) return target.apply(this, args);
      const state = self.begin(schema, name);
      if (!state) return target.apply(this, args);
      let result: Ret;
      try {
        result = self.runner.scope.run(state.frame, () => target.apply(this, args));
      } catch (reason) {
        self.complete(state, "error");
        throw reason;
      }
      if (promise(result)) {
        return result.then(
          (value) => {
            self.complete(state, "ok");
            return value;
          },
          (reason) => {
            self.complete(state, "error");
            throw reason;
          },
        ) as unknown as Ret;
      }
      self.complete(state, "ok");
      return result;
    };
  }
}
