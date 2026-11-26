import { afterEach, describe, expect, it, vi } from "vitest";
import { Locus, type Diagnostic } from "@perishlab/locus";
import { decode, mockdb, recorder } from "./fixture.js";

describe("L1 ring buffer, L2 IndexedDB spool, rotation, TTL, and lock", () => {
  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("drops oldest buffered records FIFO when L1 ring exceeds 256 records", async () => {
    const diagnostics: Diagnostic[] = [];
    let release!: () => void;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const { calls, fetcher } = recorder(async (_, index) => {
      if (index === 0) await gate;
      return new Response(null, { status: 204 });
    });

    const locus = new Locus({
      producer: "ring-cap",
      endpoint: "https://ingest.test/api/v1/atoms",
      hook: (d) => diagnostics.push(d),
      fetcher,
      database: null,
    });

    for (let i = 0; i < 340; i += 1) {
      locus.info("msg-" + i);
    }
    release();
    await locus.flush();
    await locus.close();

    const messages = calls.flatMap((call) => call.lines.map((line) => decode(line).payload?.message));
    expect(diagnostics.some((d) => d.code === "ring.overflow")).toBe(true);
    expect(messages[messages.length - 1]).toBe("msg-339");
    expect(messages.length).toBeLessThan(340);
  });

  it("persists to IndexedDB spool, replays after reload without losing new L1 records, expires 24h TTL, and rotates > 32 segments", async () => {
    const diagnostics: Diagnostic[] = [];
    const mock = mockdb();

    const failed = recorder(() => new Response(null, { status: 503 }));
    const first = new Locus({
      producer: "spool-app",
      endpoint: "https://ingest.test/api/v1/atoms",
      hook: (d) => diagnostics.push(d),
      fetcher: failed.fetcher,
      database: mock.factory,
    });

    first.info("persisted-before-reload");
    await first.flush();
    await first.close();

    const map = mock.stores.get("locus:spool:spool-app")!;
    expect(map.size).toBe(1);
    map.set(999, {
      id: 999,
      created: Date.now() - 25 * 60 * 60 * 1000,
      bytes: 20,
      count: 1,
      lines: ['{"at":1,"id":"old","producer":"spool-app","context":{},"choices":[]}\n'],
      sealed: true,
    });

    const recovered = recorder();
    const second = new Locus({
      producer: "spool-app",
      endpoint: "https://ingest.test/api/v1/atoms",
      hook: (d) => diagnostics.push(d),
      fetcher: recovered.fetcher,
      database: mock.factory,
    });

    second.info("fresh-after-reload-1");
    second.info("fresh-after-reload-2");
    await second.flush();
    expect(diagnostics.some((d) => d.code === "spool.expired")).toBe(true);
    const replayed = recovered.calls.flatMap((call) => call.lines.map(decode));
    const texts = replayed.map((item) => item.payload?.message);
    expect(texts).toEqual([
      "persisted-before-reload",
      "fresh-after-reload-1",
      "fresh-after-reload-2",
    ]);

    const now = Date.now();
    for (let i = 1; i <= 35; i += 1) {
      map.set(1000 + i, {
        id: 1000 + i,
        created: now + i,
        bytes: 40,
        count: 1,
        lines: [`{"at":${i},"id":"seg-${i}","producer":"spool-app","context":{},"choices":[]}\n`],
        sealed: true,
      });
    }
    await second.flush();
    await second.close();

    expect(diagnostics.some((d) => d.code === "spool.rotated")).toBe(true);
    const ids = recovered.calls
      .flatMap((call) => call.lines.map((line) => decode(line).id))
      .filter((id) => id.startsWith("seg-"));
    expect(ids).not.toContain("seg-1");
    expect(ids).toContain("seg-35");
  });

  it("isolates unsealed IndexedDB segments per session and retires beacon-acknowledged records from spool", async () => {
    const state = "visibility" + "State";
    const doc = new EventTarget();
    (doc as unknown as Record<string, unknown>)[state] = "hidden";
    const win = new EventTarget();
    vi.stubGlobal("document", doc);
    vi.stubGlobal("window", win);

    const mock = mockdb();
    const first = recorder();
    const tab = new Locus({
      producer: "dedupe-app",
      endpoint: "https://ingest.test/api/v1/atoms",
      fetcher: first.fetcher,
      database: mock.factory,
    });

    tab.info("before-tab-hide");
    await new Promise((resolve) => setTimeout(resolve, 20));
    doc.dispatchEvent(new Event("visibilitychange"));
    await new Promise((resolve) => setTimeout(resolve, 20));
    await tab.flush();
    await tab.close();

    const sent = first.calls.flatMap((call) => call.lines.map(decode));
    expect(sent).toHaveLength(1);
    expect(sent[0].payload?.message).toBe("before-tab-hide");
    expect(mock.stores.get("locus:spool:dedupe-app")?.size ?? 0).toBe(0);

    const blocked = recorder(() => new Response(null, { status: 503 }));
    const oldtab = new Locus({
      producer: "session-app",
      endpoint: "https://ingest.test/api/v1/atoms",
      fetcher: blocked.fetcher,
      database: mock.factory,
    });
    oldtab.info("from-page-1");
    await new Promise((resolve) => setTimeout(resolve, 20));
    await oldtab.close();

    const recovered = recorder();
    const newtab = new Locus({
      producer: "session-app",
      endpoint: "https://ingest.test/api/v1/atoms",
      fetcher: recovered.fetcher,
      database: mock.factory,
    });
    newtab.info("from-page-2");
    await newtab.flush();
    await newtab.close();

    expect(recovered.calls).toHaveLength(2);
    expect(recovered.calls[0].lines.map((l) => decode(l).payload?.message)).toEqual(["from-page-1"]);
    expect(recovered.calls[1].lines.map((l) => decode(l).payload?.message)).toEqual(["from-page-2"]);
  });

  it("falls back cleanly to L1-only when IndexedDB open fails or write throws QuotaExceededError", async () => {
    const diagnostics: Diagnostic[] = [];
    const denied = mockdb({ fail: true });
    const { calls, fetcher } = recorder();
    const locus = new Locus({
      producer: "private-tab",
      endpoint: "https://ingest.test/api/v1/atoms",
      hook: (d) => diagnostics.push(d),
      fetcher,
      database: denied.factory,
    });

    locus.info("still-delivered");
    await locus.flush();
    await locus.close();

    expect(diagnostics.some((d) => d.code === "spool.unavailable")).toBe(true);
    expect(calls).toHaveLength(1);
    expect(decode(calls[0].lines[0]).payload?.message).toBe("still-delivered");

    const quotadb = mockdb({ quota: true });
    const qrec = recorder();
    const qdiag: Diagnostic[] = [];
    const qlocus = new Locus({
      producer: "quota-tab",
      endpoint: "https://ingest.test/api/v1/atoms",
      hook: (d) => qdiag.push(d),
      fetcher: qrec.fetcher,
      database: quotadb.factory,
    });

    qlocus.info("survived-quota-1");
    qlocus.info("survived-quota-2");
    await qlocus.flush();
    await qlocus.close();

    expect(qdiag.some((d) => d.code === "spool.write")).toBe(true);
    const messages = qrec.calls.flatMap((call) => call.lines.map((line) => decode(line).payload?.message));
    expect(messages).toEqual(["survived-quota-1", "survived-quota-2"]);
  });

  it("reschedules drain when navigator.locks is contended and delivers once lock frees", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "Date"] });
    const names: string[] = [];
    let granted = false;
    vi.stubGlobal("navigator", {
      locks: {
        request: async (
          name: string,
          opts: unknown,
          callback: (lock: unknown) => Promise<void>,
        ) => {
          expect(opts).toEqual({ ["if" + "Available"]: true });
          names.push(name);
          await callback(granted ? { name } : null);
        },
      },
    });

    const { calls, fetcher } = recorder();
    const locus = new Locus({
      producer: "multi-tab",
      endpoint: "https://ingest.test/api/v1/atoms",
      fetcher,
      database: null,
    });

    locus.info("queued");
    await locus.flush();
    expect(names).toContain("locus:spool:multi-tab");
    expect(calls).toHaveLength(0);

    granted = true;
    await vi.advanceTimersByTimeAsync(1000);
    await vi.waitFor(() => expect(calls).toHaveLength(1));

    await locus.close();
  });
});
