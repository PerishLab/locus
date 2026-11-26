import { describe, expect, it } from "vitest";
import { Locus, type Diagnostic } from "@perishlab/locus";
import { decode, recorder } from "./fixture.js";

describe("context auto-inference", () => {
  it("inherits trace with distinct span and no same-trace parent across async awaits in Node ALS mode", async () => {
    const { calls, fetcher } = recorder();
    const locus = new Locus({
      producer: "node-svc",
      endpoint: "https://ingest.test/api/v1/atoms",
      environment: { env: "prod", region: "eu" },
      fetcher,
      database: null,
      mode: "auto",
    });

    const inner = locus.span("inner", async () => {
      await new Promise((resolve) => setTimeout(resolve, 5));
      locus.info("deep");
    });

    const outer = locus.trace("outer", async (tag: string) => {
      return locus.with({ request: tag }, async () => {
        await new Promise((resolve) => setTimeout(resolve, 5));
        await inner();
      });
    });

    await Promise.all([outer("r1"), outer("r2")]);
    await locus.flush();
    await locus.close();

    const atoms = calls.flatMap((call) => call.lines.map(decode));
    const logs = atoms.filter((item) => item.context["locus.schema"] === "locus.log");
    expect(logs).toHaveLength(2);
    expect(logs[0].context["locus.trace"]).toMatch(/^[0-9a-f]{32}$/);
    expect(logs[0].context["locus.span"]).toMatch(/^[0-9a-f]{16}$/);
    expect(logs[0].context["locus.parent"]).toBeUndefined();
    expect(logs[0].context["locus.trace"]).not.toBe(logs[1].context["locus.trace"]);
    expect(logs[0].context.env).toBe("prod");
    expect(logs[0].context.region).toBe("eu");

    for (const log of logs) {
      const parent = atoms.find(
        (item) =>
          item.source?.function === "outer" &&
          item.source?.edge === "enter" &&
          item.context["locus.trace"] === log.context["locus.trace"],
      );
      expect(parent).toBeDefined();
      expect(log.context["locus.span"]).not.toBe(parent?.context["locus.span"]);
      expect(log.context["locus.parent"]).toBeUndefined();
    }
  });

  it("uses sync stack and closure-captured frames without same-trace parent in browser fallback mode", async () => {
    const { calls, fetcher } = recorder();
    const locus = new Locus({
      producer: "browser-app",
      endpoint: "https://ingest.test/api/v1/atoms",
      environment: { surface: "web" },
      fetcher,
      database: null,
      mode: "stack",
    });

    const child = locus.span("child", () => {
      locus.info("sync-child");
    });

    const parent = locus.trace("parent", () => {
      child();
    });

    const deferred = locus.trace("deferred", async () => {
      await Promise.resolve();
      return "done";
    });

    parent();
    await deferred();
    await locus.flush();
    await locus.close();

    const atoms = calls.flatMap((call) => call.lines.map(decode));
    const enter = atoms.find((a) => a.source?.function === "parent" && a.source?.edge === "enter");
    const log = atoms.find((a) => a.context["locus.schema"] === "locus.log");
    expect(log?.context["locus.trace"]).toBe(enter?.context["locus.trace"]);
    expect(log?.context["locus.span"]).not.toBe(enter?.context["locus.span"]);
    expect(log?.context["locus.parent"]).toBeUndefined();
    expect(log?.context.surface).toBe("web");

    const open = atoms.find((a) => a.source?.function === "deferred" && a.source?.edge === "enter");
    const close = atoms.find((a) => a.source?.function === "deferred" && a.source?.edge === "return");
    expect(open?.context["locus.trace"]).toBe(close?.context["locus.trace"]);
    expect(open?.context["locus.span"]).toBe(close?.context["locus.span"]);
    expect(open?.context["locus.parent"]).toBeUndefined();
  });

  it("forks cross-trace links with fresh trace, cleared span, and strips reserved locus.* roles from user context", async () => {
    const diagnostics: Diagnostic[] = [];
    const { calls, fetcher } = recorder();
    const locus = new Locus({
      producer: "fork-app",
      endpoint: "https://ingest.test/api/v1/atoms",
      environment: { env: "prod", "locus.parent": "forged-env" },
      hook: (d) => diagnostics.push(d),
      fetcher,
      database: null,
    });

    const child = locus.span("child", () => {
      locus.info("forked-log");
    });

    const outer = locus.trace("outer", () => {
      locus.with({ "locus.parent": "forged-with", "locus.trace": "bad", tenant: "t1" }, () => {
        locus.info("same-trace");
      });
      locus.fork(() => {
        child();
      });
    });

    outer();
    locus.fork(() => {
      locus.info("unframed");
    });
    locus.fork("external-parent-span", () => {
      const remote = locus.span("remote", () => {
        locus.info("from-header");
      });
      remote();
    });
    locus.fork("", () => {
      locus.info("empty-parent");
    });

    await locus.flush();
    await locus.close();

    const atoms = calls.flatMap((call) => call.lines.map(decode));
    const entered = atoms.find((a) => a.source?.function === "outer" && a.source?.edge === "enter");
    const same = atoms.find((a) => a.payload?.message === "same-trace");
    const forked = atoms.find((a) => a.payload?.message === "forked-log");
    const header = atoms.find((a) => a.payload?.message === "from-header");

    expect(same?.context["locus.trace"]).toBe(entered?.context["locus.trace"]);
    expect(same?.context["locus.parent"]).toBeUndefined();
    expect(same?.context.tenant).toBe("t1");
    expect(same?.context.env).toBe("prod");

    expect(forked?.context["locus.trace"]).not.toBe(entered?.context["locus.trace"]);
    expect(forked?.context["locus.parent"]).toBe(entered?.context["locus.span"]);
    expect(forked?.context["locus.span"]).not.toBe(entered?.context["locus.span"]);
    expect(forked?.choices.some((c) => c.role === "locus.parent" && c.key === entered?.context["locus.span"])).toBe(true);

    expect(header?.context["locus.parent"]).toBe("external-parent-span");
    expect(header?.context["locus.trace"]).toMatch(/^[0-9a-f]{32}$/);
    expect(header?.context["locus.trace"]).not.toBe(entered?.context["locus.trace"]);
    expect(header?.choices.some((c) => c.role === "locus.parent" && c.key === "external-parent-span")).toBe(true);

    expect(diagnostics.filter((d) => d.code === "context.reserved")).toHaveLength(3);
    expect(diagnostics.filter((d) => d.code === "fork.no_span")).toHaveLength(1);
    expect(diagnostics.filter((d) => d.code === "fork.invalid")).toHaveLength(1);
  });
});
