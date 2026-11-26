import type { Atom, Detail } from "../types.js";

const encoder = new TextEncoder();

export const cap = 32 * 1024;

const sensitive = [
  "password",
  "token",
  "secret",
  "authorization",
  "cookie",
  "code_verifier",
  "rescue",
  "sudo",
];

const queries = [
  "code",
  "token",
  "state",
  "secret",
  "password",
  "code_verifier",
];

function forbidden(key: string): boolean {
  const lower = key.toLowerCase();
  return sensitive.some((word) => lower.includes(word));
}

function secret(key: string): boolean {
  const lower = key.toLowerCase();
  if (queries.includes(lower)) return true;
  return forbidden(lower);
}

function record(source: Record<string, unknown>, sorted: boolean): string {
  const entries = Object.entries(source).filter(([, item]) => item !== undefined);
  if (sorted) entries.sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
  const parts = entries.map(
    ([key, item]) => JSON.stringify(key) + ":" + serialize(item, false),
  );
  return "{" + parts.join(",") + "}";
}

function serialize(value: unknown, sorted: boolean): string {
  if (value === null) return "null";
  switch (typeof value) {
    case "bigint":
      return value.toString();
    case "string":
      return JSON.stringify(value);
    case "boolean":
      return value ? "true" : "false";
    case "number":
      return Number.isFinite(value) ? String(value) : "null";
    case "object":
      return Array.isArray(value)
        ? "[" + value.map((item) => serialize(item, false)).join(",") + "]"
        : record(value as Record<string, unknown>, sorted);
    default:
      return "null";
  }
}

export function encode(atom: Atom): { line: string; bytes: number } | null {
  const parts: string[] = [
    `"at":${atom.at.toString()}`,
    `"id":${JSON.stringify(atom.id)}`,
    `"producer":${JSON.stringify(atom.producer)}`,
    `"context":${serialize(atom.context, true)}`,
    `"choices":${serialize(atom.choices, false)}`,
  ];
  if (atom.source !== undefined) {
    parts.push(`"source":${serialize(atom.source, false)}`);
  }
  if (atom.payload !== undefined) {
    parts.push(`"payload":${serialize(atom.payload, false)}`);
  }
  const line = "{" + parts.join(",") + "}\n";
  const bytes = encoder.encode(line).byteLength;
  if (bytes > cap) return null;
  return { line, bytes };
}

export function measure(text: string): Uint8Array {
  return encoder.encode(text);
}

export function truncate(text: string, limit: number): string {
  if (text.length <= limit) return text;
  return text.slice(0, limit);
}

export function scrub(detail?: Detail): Detail | undefined {
  if (!detail || typeof detail !== "object") return undefined;
  const clean: Detail = {};
  let count = 0;
  for (const [key, value] of Object.entries(detail)) {
    if (forbidden(key)) continue;
    if (typeof value === "string") {
      clean[key] = truncate(value, 512);
      count += 1;
    } else if (typeof value === "boolean") {
      clean[key] = value;
      count += 1;
    } else if (typeof value === "number" && Number.isFinite(value)) {
      clean[key] = value;
      count += 1;
    }
  }
  return count > 0 ? clean : undefined;
}

export function extract(reason: unknown): { name: string; message: string; stack?: string } {
  if (reason instanceof Error) {
    const name = truncate(reason.name || "Error", 128);
    const message = truncate(reason.message || "", 2048);
    const stack = typeof reason.stack === "string"
      ? truncate(reason.stack.split("\n").slice(0, 32).join("\n"), 4096)
      : undefined;
    return stack ? { name, message, stack } : { name, message };
  }
  if (typeof reason === "string") {
    return { name: "Fault", message: truncate(reason, 2048) };
  }
  if (reason && typeof reason === "object") {
    const item = reason as Record<string, unknown>;
    const name = typeof item.name === "string" ? truncate(item.name, 128) : "Fault";
    const message = typeof item.message === "string"
      ? truncate(item.message, 2048)
      : "unknown fault";
    const stack = typeof item.stack === "string"
      ? truncate(item.stack.split("\n").slice(0, 32).join("\n"), 4096)
      : undefined;
    return stack ? { name, message, stack } : { name, message };
  }
  return { name: "Fault", message: truncate(String(reason), 2048) };
}

export function sanitize(input: RequestInfo | URL): string {
  const raw = typeof input === "string"
    ? input
    : input instanceof URL
      ? input.href
      : input.url;
  try {
    const parsed = new URL(raw, "http://locus.invalid");
    parsed.username = "";
    parsed.password = "";
    const keys = Array.from(parsed.searchParams.keys());
    for (const key of keys) {
      if (secret(key)) parsed.searchParams.delete(key);
    }
    if (parsed.origin === "http://locus.invalid") {
      return parsed.pathname + parsed.search;
    }
    return parsed.origin + parsed.pathname + parsed.search;
  } catch {
    return raw.split(/[?#]/)[0] ?? "";
  }
}
