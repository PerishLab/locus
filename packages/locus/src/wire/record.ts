import type { Atom, Choice, Diagnostic, Edge, Frame, Metric } from "../types.js";

function valid(kind: unknown): kind is Metric["kind"] {
  return kind === "counter" || kind === "gauge";
}

function nonempty(value: unknown): value is string {
  return typeof value === "string" && value.length > 0;
}

function epoch(value: unknown): boolean {
  if (value === undefined) return true;
  if (typeof value === "number") return Number.isSafeInteger(value);
  return nonempty(value);
}

function reserved(key: string): boolean {
  return key.startsWith("locus.");
}

function checklabels(labels: unknown): boolean {
  if (labels === undefined) return true;
  if (typeof labels !== "object" || labels === null || Array.isArray(labels)) return false;
  for (const [key, value] of Object.entries(labels as Record<string, unknown>)) {
    if (key.length === 0 || typeof value !== "string") return false;
  }
  return true;
}

export function clean(
  values?: Record<string, string>,
  report?: (diagnostic: Diagnostic) => void,
): Record<string, string> {
  if (!values || typeof values !== "object") return {};
  const out: Record<string, string> = {};
  for (const [key, value] of Object.entries(values)) {
    if (key.length === 0 || typeof value !== "string") continue;
    if (reserved(key)) {
      report?.({
        code: "context.reserved",
        message: `reserved role "${key}" cannot be set in user context; same-trace nesting is automatic via locus.trace containment and cross-trace links must use locus.fork()`,
      });
      continue;
    }
    out[key] = value;
  }
  return out;
}

export function inherited(frame?: Frame): Choice[] {
  if (!frame) return [];
  const choices: Choice[] = [];
  if (frame.trace) choices.push({ role: "locus.trace", key: frame.trace, origin: "inherited" });
  if (frame.span) choices.push({ role: "locus.span", key: frame.span, origin: "inherited" });
  if (frame.parent) choices.push({ role: "locus.parent", key: frame.parent, origin: "inherited" });
  return choices;
}

export function admit(sample: Metric, report: (diagnostic: Diagnostic) => void): boolean {
  if (!sample || typeof sample !== "object") {
    report({ code: "metric.invalid", message: "metric sample must be an object" });
    return false;
  }
  if (typeof sample.value !== "number" || !Number.isFinite(sample.value)) {
    report({ code: "metric.invalid", message: "metric value must be finite" });
    return false;
  }
  if (!nonempty(sample.name) || !nonempty(sample.unit)) {
    report({ code: "metric.invalid", message: "metric name and unit must be nonempty strings" });
    return false;
  }
  if (!nonempty(sample.series) || !valid(sample.kind)) {
    report({ code: "metric.invalid", message: "metric series and kind are invalid" });
    return false;
  }
  if (sample.kind === "counter" && sample.value < 0) {
    report({ code: "metric.invalid", message: "counter value must be nonnegative" });
    return false;
  }
  if (!epoch(sample.epoch)) {
    report({ code: "metric.invalid", message: "metric epoch must be a nonempty string or safe integer" });
    return false;
  }
  if (!checklabels(sample.labels)) {
    report({ code: "metric.invalid", message: "metric labels must map nonempty keys to strings" });
    return false;
  }
  return true;
}

export function descriptor(sample: Metric): Record<string, string> {
  const extra: Record<string, string> = {
    "locus.schema": "locus.metric",
    "locus.edge": "point",
    "metric.name": sample.name,
    "metric.unit": sample.unit,
    "metric.kind": sample.kind,
    "metric.series": sample.series,
  };
  if (sample.epoch !== undefined) extra["metric.epoch"] = String(sample.epoch);
  if (sample.labels && typeof sample.labels === "object") {
    for (const [key, value] of Object.entries(sample.labels)) {
      extra["metric.label." + key] = value;
    }
  }
  return extra;
}

export type Spec = {
  at: bigint;
  id: string;
  producer: string;
  environment: Record<string, string>;
  frame?: Frame;
  choices: Choice[];
  extra: Record<string, string>;
  source?: { file: string; line: number; column: number; module: string; function?: string; edge?: Edge };
  payload?: Record<string, unknown>;
};

export function assemble(spec: Spec): Atom {
  const context: Record<string, string> = {
    ...spec.environment,
    ...(spec.frame?.values ?? {}),
  };
  if (spec.frame?.trace) context["locus.trace"] = spec.frame.trace;
  if (spec.frame?.span) context["locus.span"] = spec.frame.span;
  if (spec.frame?.parent) context["locus.parent"] = spec.frame.parent;
  Object.assign(context, spec.extra);
  return {
    at: spec.at,
    id: spec.id,
    producer: spec.producer,
    context,
    choices: spec.choices,
    source: spec.source,
    payload: spec.payload,
  };
}
