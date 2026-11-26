import { Clock } from "./clock.js";
import { Binder, type Step } from "./decorate.js";
import { attach } from "./delivery/lifecycle.js";
import { Transport } from "./delivery/transport.js";
import { Scope } from "./scope.js";
import type {
  Atom,
  Detail,
  Diagnostic,
  Fetcher,
  Frame,
  Hook,
  Level,
  Metric,
  Options,
  Wrapper,
} from "./types.js";
import { encode, extract, scrub } from "./wire/encode.js";
import { admit, assemble, clean, descriptor, inherited } from "./wire/record.js";

function database(option?: IDBFactory | null): IDBFactory | null {
  if (option !== undefined) return option;
  const candidate = (globalThis as Record<string, unknown>)["indexedDB"] as IDBFactory | undefined;
  return candidate && typeof candidate.open === "function" ? candidate : null;
}

function defaultfetch(): Fetcher {
  return (input, init) => globalThis.fetch(input, init);
}

export class Locus {
  private readonly producer: string;
  private readonly active: boolean;
  private readonly environment: Record<string, string>;
  private readonly hook?: Hook;
  private readonly clock: Clock;
  private readonly scope: Scope;
  private readonly transport: Transport | null;
  private readonly detach: (() => void) | null;
  private observing = false;

  readonly trace: Wrapper;
  readonly span: Wrapper;
  readonly with: <Ret>(context: Record<string, string>, fn: () => Ret) => Ret;
  readonly info: (text: string) => void;
  readonly warn: (text: string) => void;
  readonly error: (text: string) => void;
  readonly fault: (reason: unknown, detail?: Detail) => void;
  readonly metric: (sample: Metric) => void;
  readonly flush: () => Promise<void>;
  readonly close: () => Promise<void>;
  readonly dispose: () => Promise<void>;

  constructor(options: Options) {
    this.producer = typeof options?.producer === "string" ? options.producer : "";
    const endpoint = typeof options?.endpoint === "string" ? options.endpoint.trim() : "";
    this.active = options?.enabled !== false && endpoint.length > 0;
    this.environment = clean(options?.environment);
    this.hook = typeof options?.hook === "function" ? options.hook : undefined;
    this.clock = new Clock();
    this.scope = new Scope(this.active ? (options?.mode ?? "auto") : "stack");
    if (this.active) {
      this.transport = new Transport({
        producer: this.producer,
        endpoint,
        token: options?.token,
        headers: options?.headers,
        fetcher: options?.fetcher ?? defaultfetch(),
        database: database(options?.database),
        notify: (diagnostic) => this.report(diagnostic),
      });
      this.detach = attach(() => this.transport?.beacon());
    } else {
      this.transport = null;
      this.detach = null;
    }
    const binder = new Binder({
      active: this.active,
      clock: this.clock,
      scope: this.scope,
      emit: (step: Step) => this.record(step),
    });
    this.trace = binder.make("locus.trace");
    this.span = binder.make("locus.span");
    this.with = (context, fn) => this.scoped(context, fn);
    this.info = (text) => this.log("info", text);
    this.warn = (text) => this.log("warn", text);
    this.error = (text) => this.log("error", text);
    this.fault = (reason, detail) => this.failure(reason, detail);
    this.metric = (sample) => this.measure(sample);
    this.flush = () => this.sync();
    this.close = () => this.shutdown();
    this.dispose = () => this.shutdown();
  }

  private report(diagnostic: Diagnostic): void {
    if (!this.hook || this.observing) return;
    this.observing = true;
    try {
      this.hook(diagnostic);
    } catch {
      return;
    } finally {
      this.observing = false;
    }
  }

  private safe(work: () => void): void {
    try {
      work();
    } catch (reason) {
      this.report({
        code: "locus.internal",
        message: reason instanceof Error ? reason.message : "internal error",
      });
    }
  }

  private record(step: Step): void {
    this.safe(() => {
      const payload: Record<string, unknown> = { name: step.name };
      if (step.elapsed !== undefined) payload.elapsed = step.elapsed;
      if (step.status !== undefined) payload.status = step.status;
      const atom = assemble({
        at: step.at,
        id: this.clock.id(),
        producer: this.producer,
        environment: this.environment,
        frame: step.frame,
        choices: step.choices,
        extra: { "locus.schema": step.schema, "locus.edge": step.edge },
        source: {
          file: "",
          line: 0,
          column: 0,
          module: this.producer,
          function: step.name,
          edge: step.edge,
        },
        payload,
      });
      this.emit(atom, false);
    });
  }

  private scoped<Ret>(context: Record<string, string>, fn: () => Ret): Ret {
    if (!this.active) return fn();
    const prior = this.scope.current();
    const frame: Frame = {
      trace: prior?.trace,
      span: prior?.span,
      parent: prior?.parent,
      values: { ...(prior?.values ?? {}), ...clean(context) },
    };
    return this.scope.run(frame, fn);
  }

  private log(level: Level, text: string): void {
    if (!this.active) return;
    this.safe(() => {
      if (typeof text !== "string") {
        this.report({ code: "log.invalid", message: "log message must be a string" });
        return;
      }
      const frame = this.scope.current();
      const atom = assemble({
        at: this.clock.now(),
        id: this.clock.id(),
        producer: this.producer,
        environment: this.environment,
        frame,
        choices: inherited(frame),
        extra: { "locus.schema": "locus.log", "locus.edge": "point", level },
        payload: { message: text },
      });
      this.emit(atom, false);
    });
  }

  private failure(reason: unknown, detail?: Detail): void {
    if (!this.active) return;
    this.safe(() => {
      const info = extract(reason);
      const cleaned = scrub(detail);
      const payload: Record<string, unknown> = { name: info.name, message: info.message };
      if (info.stack) payload.stack = info.stack;
      if (cleaned) payload.detail = cleaned;
      const frame = this.scope.current();
      const atom = assemble({
        at: this.clock.now(),
        id: this.clock.id(),
        producer: this.producer,
        environment: this.environment,
        frame,
        choices: inherited(frame),
        extra: {
          "locus.schema": "locus.fault",
          "locus.edge": "point",
          "locus.fault": info.name,
        },
        payload,
      });
      this.emit(atom, true);
    });
  }

  private measure(sample: Metric): void {
    if (!this.active) return;
    this.safe(() => {
      if (!admit(sample, (diagnostic) => this.report(diagnostic))) return;
      const frame = this.scope.current();
      const atom = assemble({
        at: this.clock.now(),
        id: this.clock.id(),
        producer: this.producer,
        environment: this.environment,
        frame,
        choices: inherited(frame),
        extra: descriptor(sample),
        payload: { value: sample.value },
      });
      this.emit(atom, false);
    });
  }

  private emit(atom: Atom, urgent: boolean): void {
    if (!this.transport) return;
    const encoded = encode(atom);
    if (!encoded) {
      this.report({ code: "record.oversize", message: "record exceeds 32 KiB limit" });
      return;
    }
    this.transport.enqueue({ line: encoded.line, bytes: encoded.bytes, urgent });
  }

  private async sync(): Promise<void> {
    if (!this.transport) return;
    try {
      await this.transport.flush();
    } catch (reason) {
      this.report({
        code: "locus.flush",
        message: reason instanceof Error ? reason.message : "flush failed",
      });
    }
  }

  private async shutdown(): Promise<void> {
    if (this.detach) {
      try {
        this.detach();
      } catch {
        undefined;
      }
    }
    if (!this.transport) return;
    try {
      await this.transport.close();
    } catch (reason) {
      this.report({
        code: "locus.close",
        message: reason instanceof Error ? reason.message : "close failed",
      });
    }
  }
}
