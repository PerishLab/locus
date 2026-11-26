import type { Locus } from "./locus.js";
import type { Detail, Fetcher } from "./types.js";
import { sanitize } from "./wire/encode.js";

type Failure = {
  error?: unknown;
  message?: string;
  filename?: string;
  lineno?: number;
};

type Rejection = {
  reason?: unknown;
};

export function capture(locus: Locus, target?: EventTarget): () => void {
  const host = target ?? (typeof globalThis.addEventListener === "function" ? globalThis : undefined);
  if (!host || typeof host.addEventListener !== "function") {
    return () => undefined;
  }
  const onerror = (event: Event) => {
    const item = event as unknown as Failure;
    const detail: Detail = { source: "window.error" };
    if (typeof item.filename === "string" && item.filename.length > 0) {
      detail.file = item.filename;
    }
    if (typeof item.lineno === "number" && Number.isFinite(item.lineno)) {
      detail.line = item.lineno;
    }
    locus.fault(item.error ?? item.message ?? "window error", detail);
  };
  const onreject = (event: Event) => {
    const item = event as unknown as Rejection;
    locus.fault(item.reason ?? "unhandled rejection", { source: "unhandledrejection" });
  };
  host.addEventListener("error", onerror);
  host.addEventListener("unhandledrejection", onreject);
  return () => {
    host.removeEventListener("error", onerror);
    host.removeEventListener("unhandledrejection", onreject);
  };
}

function verb(input: RequestInfo | URL, init?: RequestInit): string {
  if (init?.method) return init.method.toUpperCase();
  if (typeof input !== "object" || input === null) return "GET";
  if (input instanceof URL || !("method" in input)) return "GET";
  return String(input.method || "GET").toUpperCase();
}

async function perform(
  locus: Locus,
  base: Fetcher,
  input: RequestInfo | URL,
  init?: RequestInit,
): Promise<Response> {
  const method = verb(input, init);
  const url = sanitize(input);
  try {
    const response = await base(input, init);
    if (response.status >= 500) {
      locus.fault(new Error(`HTTP ${response.status}`), {
        method,
        url,
        status: response.status,
      });
    }
    return response;
  } catch (reason) {
    locus.fault(reason, { method, url });
    throw reason;
  }
}

export function request(locus: Locus, fetcher?: Fetcher): Fetcher {
  const base: Fetcher = fetcher ?? ((input, init) => globalThis.fetch(input, init));
  return async (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
    const method = verb(input, init);
    const url = sanitize(input);
    const run = locus.span("http.request", () => perform(locus, base, input, init));
    return locus.with({ method, url }, () => run());
  };
}
