import { describe, expect, it } from "vitest";
import { Locus } from "@perishlab/locus";
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
});
