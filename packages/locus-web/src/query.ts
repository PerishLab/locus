export type View = {
  producer: string;
  name: string;
  unit: string;
  kind: "gauge" | "counter";
  from: string;
  to: string;
  step: string;
  group: string;
};

export type Evidence = { series: string; at: string; id: string };
export type Bucket = {
  from: string;
  to: string;
  samples: number;
  quality: string;
  value: number | null;
  evidence: Evidence[];
};
export type Report = { groups: { label: string | null; buckets: Bucket[] }[] };

const nanos = 1_000_000_000n;

export function initial(now = Date.now()): View {
  return {
    producer: "", name: "", unit: "", kind: "gauge", group: "",
    from: instant(BigInt(now - 3_600_000) * 1_000_000n),
    to: instant(BigInt(now) * 1_000_000n),
    step: "15",
  };
}

export function instant(at: bigint | string): string {
  const value = BigInt(at);
  const date = new Date(Number(value / 1_000_000n)).toISOString();
  return date.slice(0, 19) + "." + (value % nanos).toString().padStart(9, "0") + "Z";
}

export function timestamp(value: string): bigint {
  const match = /^(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d)(?:\.(\d{1,9}))?Z$/.exec(value);
  if (!match) throw new Error("Enter a UTC instant ending in Z.");
  const date = new Date(match[1] + "Z");
  if (!Number.isFinite(date.getTime()) || date.getTime() < 0 || !date.toISOString().startsWith(match[1])) {
    throw new Error("Enter a valid UTC date.");
  }
  return BigInt(date.getTime()) * 1_000_000n + BigInt((match[2] ?? "").padEnd(9, "0"));
}

export function restore(url: URL): View {
  const fallback = initial();
  const get = (key: keyof View) => url.searchParams.get(key) ?? fallback[key];
  return {
    producer: get("producer"), name: get("name"), unit: get("unit"),
    kind: get("kind") === "counter" ? "counter" : "gauge",
    from: get("from"), to: get("to"), step: get("step"), group: get("group"),
  };
}

export function share(view: View): string {
  const params = new URLSearchParams(Object.entries(view).filter(([, value]) => value !== ""));
  return "/?" + params;
}

export function request(view: View): URLSearchParams {
  const from = timestamp(view.from);
  const to = timestamp(view.to);
  if (!/^[1-9]\d*$/.test(view.step) || from >= to) {
    throw new Error("Check the time window and bucket duration.");
  }
  const params = new URLSearchParams({
    producer: view.producer, name: view.name, unit: view.unit, kind: view.kind,
    from: String(from), to: String(to), step: String(BigInt(view.step) * nanos),
  });
  if (view.group) params.set("group", view.group);
  return params;
}

export function decode<T>(text: string): T {
  return JSON.parse(text, (key, value, context?: { source?: string }) => {
    if (!["at", "from", "to", "step"].includes(key) || typeof value !== "number") return value;
    if (context?.source) return context.source;
    if (Number.isSafeInteger(value)) return String(value);
    throw new Error("This browser cannot read nanosecond evidence. Update the browser.");
  }) as T;
}

export class Failure extends Error {
  readonly status: number;

  constructor(status: number) {
    super(({ 400: "Check the metric descriptor and time window.", 401: "Sign in to read this window.",
      403: "You do not have access to this source.", 422: "This query exceeds a limit or the samples do not match the metric." } as Record<number, string>)[status] ??
      "Locus could not complete the request. Try again.");
    this.status = status;
  }
}

export async function read(path: string, signal: AbortSignal): Promise<string> {
  const response = await fetch(path, { signal, cache: "no-store", credentials: "same-origin" });
  if (!response.ok) throw new Failure(response.status);
  return response.text();
}
