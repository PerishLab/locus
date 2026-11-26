import { createHash } from "node:crypto";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Locus, type Diagnostic } from "@perishlab/locus";
import { decode, recorder } from "./fixture.js";

function sha256(text: string): string {
  return createHash("sha256").update(Buffer.from(text, "utf8")).digest("hex");
}

describe("transport batching, compression, idempotency, and retries", () => {
  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it("flushes on 64 records, 64 KiB bytes, 2000ms normal timer, and 250ms urgent fault timer", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "Date"] });
    const { calls, fetcher } = recorder();
    const locus = new Locus({
      producer: "batch-test",
      endpoint: "https://ingest.test/api/v1/atoms",
      fetcher,
      database: null,
    });

    locus.info("one");
    await vi.advanceTimersByTimeAsync(1999);
    expect(calls).toHaveLength(0);
    await vi.advanceTimersByTimeAsync(1);
    await vi.waitFor(() => expect(calls).toHaveLength(1));
    expect(calls[0].lines).toHaveLength(1);

    locus.fault(new Error("urgent"));
    await vi.advanceTimersByTimeAsync(249);
    expect(calls).toHaveLength(1);
    await vi.advanceTimersByTimeAsync(1);
    await vi.waitFor(() => expect(calls).toHaveLength(2));
    expect(decode(calls[1].lines[0]).context["locus.schema"]).toBe("locus.fault");

    for (let i = 0; i < 64; i += 1) {
      locus.info("row-" + i);
    }
    await vi.waitFor(() => expect(calls).toHaveLength(3));
    expect(calls[2].lines).toHaveLength(64);

    for (let i = 0; i < 5; i += 1) {
      locus.info("b".repeat(16 * 1024));
    }
    await vi.waitFor(() => expect(calls.length).toBeGreaterThanOrEqual(4));

    await locus.close();
  });

  it("computes SHA-256 Idempotency-Key over raw NDJSON and gzips batches >= 1024 bytes", async () => {
    const { calls, fetcher } = recorder();
    const locus = new Locus({
      producer: "codec-test",
      endpoint: "https://ingest.test/api/v1/atoms",
      token: "secret-key",
      fetcher,
      database: null,
    });

    locus.info("small");
    await locus.flush();
    expect(calls[0].headers["content-encoding"]).toBeUndefined();
    expect(calls[0].headers["content-type"]).toBe("application/x-ndjson");
    expect(calls[0].headers["authorization"]).toBe("Bearer secret-key");
    expect(calls[0].headers["idempotency-key"]).toBe(sha256(calls[0].text));

    locus.info("z".repeat(2048));
    await locus.flush();
    expect(calls[1].headers["content-encoding"]).toBe("gzip");
    expect(calls[1].headers["idempotency-key"]).toBe(sha256(calls[1].text));
    expect(calls[1].raw.byteLength).toBeLessThan(calls[1].text.length);

    const saved = globalThis.crypto;
    vi.stubGlobal("crypto", { getRandomValues: saved.getRandomValues.bind(saved) });
    try {
      locus.info("fallback-sha256");
      await locus.flush();
      expect(calls[2].headers["idempotency-key"]).toBe(sha256(calls[2].text));
    } finally {
      vi.stubGlobal("crypto", saved);
    }

    await locus.close();
  });

  it("quarantines 400 and 413 batches without blocking subsequent deliveries", async () => {
    const diagnostics: Diagnostic[] = [];
    const statuses = [400, 413, 204];
    const { calls, fetcher } = recorder((_, index) => new Response(null, { status: statuses[index] ?? 204 }));
    const locus = new Locus({
      producer: "quarantine",
      endpoint: "https://ingest.test/api/v1/atoms",
      hook: (d) => diagnostics.push(d),
      fetcher,
      database: null,
    });

    locus.info("bad-400");
    await locus.flush();
    locus.info("bad-413");
    await locus.flush();
    locus.info("good-204");
    await locus.flush();
    await locus.close();

    expect(calls).toHaveLength(3);
    expect(decode(calls[2].lines[0]).payload?.message).toBe("good-204");
    const quarantined = diagnostics.filter((d) => d.code === "transport.quarantine");
    expect(quarantined.map((d) => d.status)).toEqual([400, 413]);
  });

  it("pauses sender on 401 and 403 while retaining batch", async () => {
    const diagnostics: Diagnostic[] = [];
    const { calls, fetcher } = recorder(() => new Response(null, { status: 401 }));
    const locus = new Locus({
      producer: "auth-pause",
      endpoint: "https://ingest.test/api/v1/atoms",
      hook: (d) => diagnostics.push(d),
      fetcher,
      database: null,
    });

    locus.info("blocked");
    await locus.flush();
    locus.info("after-pause");
    await locus.flush();
    await locus.close();

    expect(calls).toHaveLength(1);
    expect(diagnostics.some((d) => d.code === "transport.paused" && d.status === 401)).toBe(true);
  });

  it("retries 429 and 5xx with Retry-After and stable Idempotency-Key while keeping in-flight at 1", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "Date"] });
    const diagnostics: Diagnostic[] = [];
    let active = 0;
    let peak = 0;
    const { calls, fetcher } = recorder(async (_, index) => {
      active += 1;
      peak = Math.max(peak, active);
      await Promise.resolve();
      active -= 1;
      switch (index) {
        case 0:
          return new Response(null, { status: 429, headers: { "Retry-After": "2" } });
        case 1:
          return new Response(null, { status: 503 });
        default:
          return new Response(null, { status: 204 });
      }
    });

    const locus = new Locus({
      producer: "retry-test",
      endpoint: "https://ingest.test/api/v1/atoms",
      hook: (d) => diagnostics.push(d),
      fetcher,
      database: null,
    });

    locus.info("retry-me");
    const first = locus.flush();
    const second = locus.flush();
    await Promise.all([first, second]);
    expect(calls).toHaveLength(1);
    expect(peak).toBe(1);

    await vi.advanceTimersByTimeAsync(1999);
    expect(calls).toHaveLength(1);
    await vi.advanceTimersByTimeAsync(1);
    await vi.waitFor(() => expect(calls).toHaveLength(2));
    expect(calls[1].headers["idempotency-key"]).toBe(calls[0].headers["idempotency-key"]);

    await vi.advanceTimersByTimeAsync(30_000);
    await vi.waitFor(() => expect(calls).toHaveLength(3));
    expect(calls[2].headers["idempotency-key"]).toBe(calls[0].headers["idempotency-key"]);
    expect(diagnostics.filter((d) => d.code === "transport.retry")).toHaveLength(2);

    await locus.close();
  });

  it("holds active 429 Retry-After window even when normal, urgent fault, and 64 records enqueue", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "Date"] });
    const { calls, fetcher } = recorder((_, index) => {
      if (index === 0) {
        return new Response(null, { status: 429, headers: { "Retry-After": "10" } });
      }
      return new Response(null, { status: 204 });
    });

    const locus = new Locus({
      producer: "backoff-hold",
      endpoint: "https://ingest.test/api/v1/atoms",
      fetcher,
      database: null,
    });

    locus.info("initial");
    await locus.flush();
    expect(calls).toHaveLength(1);

    locus.info("during-backoff");
    locus.fault(new Error("urgent-during-backoff"));
    for (let i = 0; i < 64; i += 1) {
      locus.info("flood-" + i);
    }

    await vi.advanceTimersByTimeAsync(9999);
    expect(calls).toHaveLength(1);

    await vi.advanceTimersByTimeAsync(1);
    await vi.waitFor(() => expect(calls.length).toBeGreaterThanOrEqual(2));
    expect(calls[1].headers["idempotency-key"]).toBe(calls[0].headers["idempotency-key"]);

    await locus.close();
  });
});
