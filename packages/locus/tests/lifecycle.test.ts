import { afterEach, describe, expect, it, vi } from "vitest";
import { capture, Locus, request, type Diagnostic } from "@perishlab/locus";
import { decode, mockdb, recorder } from "./fixture.js";

describe("page lifecycle, disabled no-op, semantic transparency, and adapters", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("flushes with keepalive on visibilitychange hidden and pagehide, slices prefix <= 48 KiB, and cleans up", async () => {
    const state = "visibility" + "State";
    const doc = new EventTarget();
    (doc as unknown as Record<string, unknown>)[state] = "visible";
    const win = new EventTarget();
    vi.stubGlobal("document", doc);
    vi.stubGlobal("window", win);

    const { calls, fetcher } = recorder();
    const locus = new Locus({
      producer: "page-app",
      endpoint: "https://ingest.test/api/v1/atoms",
      fetcher,
      database: null,
    });

    locus.info("before-hide");
    doc.dispatchEvent(new Event("visibilitychange"));
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(calls).toHaveLength(0);

    (doc as unknown as Record<string, unknown>)[state] = "hidden";
    doc.dispatchEvent(new Event("visibilitychange"));
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(calls).toHaveLength(1);
    expect(calls[0].keepalive).toBe(true);
    expect(calls[0].headers["idempotency-key"]).toMatch(/^[0-9a-f]{64}$/);

    locus.info("on-pagehide");
    win.dispatchEvent(new Event("pagehide"));
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(calls).toHaveLength(2);
    expect(calls[1].keepalive).toBe(true);

    const saved = globalThis.CompressionStream;
    vi.stubGlobal("CompressionStream", undefined);
    try {
      locus.info("a".repeat(25 * 1024));
      locus.info("b".repeat(25 * 1024));
      win.dispatchEvent(new Event("pagehide"));
      await new Promise((resolve) => setTimeout(resolve, 20));
      expect(calls).toHaveLength(3);
      expect(calls[2].keepalive).toBe(true);
      expect(calls[2].raw.byteLength).toBeLessThanOrEqual(48 * 1024);
      expect(calls[2].lines).toHaveLength(1);
      await locus.flush();
      expect(calls).toHaveLength(4);
      expect(calls[3].lines).toHaveLength(1);
    } finally {
      vi.stubGlobal("CompressionStream", saved);
    }

    await locus.dispose();
    const count = calls.length;
    win.dispatchEvent(new Event("pagehide"));
    doc.dispatchEvent(new Event("visibilitychange"));
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(calls).toHaveLength(count);
  });

  it("does not double-drop records when visibilitychange and pagehide fire back-to-back with 70 records", async () => {
    const state = "visibility" + "State";
    const doc = new EventTarget();
    (doc as unknown as Record<string, unknown>)[state] = "hidden";
    const win = new EventTarget();
    vi.stubGlobal("document", doc);
    vi.stubGlobal("window", win);

    const { calls, fetcher } = recorder();
    const locus = new Locus({
      producer: "double-hide",
      endpoint: "https://ingest.test/api/v1/atoms",
      fetcher,
      database: null,
    });

    for (let i = 0; i < 63; i += 1) {
      locus.info("rec-" + i);
    }
    doc.dispatchEvent(new Event("visibilitychange"));
    win.dispatchEvent(new Event("pagehide"));
    for (let i = 63; i < 70; i += 1) {
      locus.info("rec-" + i);
    }
    await new Promise((resolve) => setTimeout(resolve, 30));
    await locus.flush();
    await locus.close();

    const received = new Set(
      calls.flatMap((call) => call.lines.map((line) => decode(line).payload?.message)),
    );
    expect(received.size).toBe(70);
    for (let i = 0; i < 70; i += 1) {
      expect(received.has("rec-" + i)).toBe(true);
    }
  });

  it("skips redundant visibilitychange beacon while an inflight flush is active", async () => {
    const state = "visibility" + "State";
    const doc = new EventTarget();
    (doc as unknown as Record<string, unknown>)[state] = "hidden";
    const win = new EventTarget();
    vi.stubGlobal("document", doc);
    vi.stubGlobal("window", win);

    let release!: () => void;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const { calls, fetcher } = recorder(async () => {
      await gate;
      return new Response(null, { status: 204 });
    });
    const locus = new Locus({
      producer: "inflight-app",
      endpoint: "https://ingest.test/api/v1/atoms",
      fetcher,
      database: null,
    });

    locus.info("only-once");
    const flushing = locus.flush();
    await new Promise((resolve) => setTimeout(resolve, 10));
    doc.dispatchEvent(new Event("visibilitychange"));
    await new Promise((resolve) => setTimeout(resolve, 10));
    expect(calls).toHaveLength(1);
    release();
    await flushing;
    await locus.close();
    expect(calls).toHaveLength(1);
  });

  it("is a complete silent no-op when disabled or unconfigured and never throws into callers", async () => {
    const db = mockdb();
    const { calls, fetcher } = recorder();
    const timer = vi.spyOn(globalThis, "setTimeout");

    const disabled = new Locus({
      producer: "off",
      endpoint: "https://ingest.test/api/v1/atoms",
      enabled: false,
      fetcher,
      database: db.factory,
    });
    const unconfigured = new Locus({
      producer: "empty",
      fetcher,
      database: db.factory,
    });

    const fn = disabled.trace("work", (n: number) => n * 2);
    expect(fn(21)).toBe(42);
    expect(unconfigured.with({ a: "1" }, () => "ok")).toBe("ok");
    disabled.info("ignored");
    disabled.fault(new Error("ignored"));
    unconfigured.metric({ name: "m", unit: "u", kind: "gauge", series: "s", value: 1 });
    await disabled.flush();
    await disabled.close();
    await unconfigured.close();

    expect(calls).toHaveLength(0);
    expect(db.opens()).toBe(0);
    expect(timer).not.toHaveBeenCalled();

    const broken = new Locus({
      producer: "transparent",
      endpoint: "https://ingest.test/api/v1/atoms",
      hook: () => {
        throw new Error("hook exploded");
      },
      fetcher: () => {
        throw new Error("network exploded");
      },
      database: null,
    });

    expect(() => broken.info(123 as unknown as string)).not.toThrow();
    expect(() => broken.fault(new Error("boom"))).not.toThrow();
    await expect(broken.flush()).resolves.toBeUndefined();
    await expect(broken.close()).resolves.toBeUndefined();
  });

  it("provides opt-in capture and request adapters with strict query, fragment, and header secret scrubbing", async () => {
    const { calls, fetcher } = recorder();
    const locus = new Locus({
      producer: "adapter-app",
      endpoint: "https://ingest.test/api/v1/atoms",
      fetcher,
      database: null,
    });

    const target = new EventTarget();
    const stop = capture(locus, target);

    const errored = new Event("error") as Event & { error?: Error; filename?: string; lineno?: number };
    errored.error = new Error("uncaught");
    errored.filename = "app.ts";
    errored.lineno = 12;
    target.dispatchEvent(errored);

    const rejected = new Event("unhandledrejection") as Event & { reason?: unknown };
    rejected.reason = new TypeError("promise failed");
    target.dispatchEvent(rejected);

    stop();
    target.dispatchEvent(errored);

    const wrapped = request(locus, async (input) => {
      const url = String(input);
      if (url.includes("/fail")) return new Response("err", { status: 502 });
      if (url.includes("/throw")) throw new Error("offline");
      return new Response("ok", { status: 200 });
    });

    const ok = await wrapped(
      "https://user:pass@api.test/callback?code=c1&token=t1&state=s1&secret=x&password=p&code_verifier=v&page=2#frag=secret",
      {
        method: "POST",
        headers: {
          Authorization: "Bearer super-secret-jwt",
          Cookie: "session=super-secret-cookie",
        },
      },
    );
    expect(ok.status).toBe(200);

    const five = await wrapped("https://api.test/fail?token=hide&ok=1");
    expect(five.status).toBe(502);

    await expect(wrapped("https://api.test/throw?secret=hide")).rejects.toThrow("offline");

    await locus.flush();
    await locus.close();

    const rawtext = calls.map((call) => call.text).join("");
    expect(rawtext).not.toContain("super-secret-jwt");
    expect(rawtext).not.toContain("super-secret-cookie");
    expect(rawtext).not.toContain("code=c1");
    expect(rawtext).not.toContain("token=t1");
    expect(rawtext).not.toContain("state=s1");
    expect(rawtext).not.toContain("secret=x");
    expect(rawtext).not.toContain("password=p");
    expect(rawtext).not.toContain("code_verifier=v");
    expect(rawtext).not.toContain("user:pass");
    expect(rawtext).not.toContain("frag=secret");
    expect(rawtext).toContain("page=2");

    const atoms = calls.flatMap((call) => call.lines.map(decode));
    const faults = atoms.filter((a) => a.context["locus.schema"] === "locus.fault");
    expect(faults).toHaveLength(4);
    expect(faults.map((f) => f.payload?.message)).toEqual([
      "uncaught",
      "promise failed",
      "HTTP 502",
      "offline",
    ]);
  });
});
