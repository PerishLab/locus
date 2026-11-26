import { describe, expect, it } from "vitest";
import { Locus } from "@perishlab/locus";
import { decode, recorder } from "./fixture.js";

describe("decorators and wrappers", () => {
  it("supports Stage 3 @locus.trace and @locus.span decorators for sync and async methods", async () => {
    const { calls, fetcher } = recorder();
    const locus = new Locus({
      producer: "demo",
      endpoint: "https://ingest.test/api/v1/atoms",
      fetcher,
      database: null,
    });

    class Service {
      factor = 10;

      @locus.trace
      compute(value: number): number {
        return this.step(value * this.factor);
      }

      @locus.span("service.step")
      step(value: number): number {
        return value + 1;
      }

      @locus.trace("service.fetch")
      async fetch(id: string): Promise<string> {
        await Promise.resolve();
        return this.load(id);
      }

      @locus.span
      async load(id: string): Promise<string> {
        await Promise.resolve();
        return "item:" + id;
      }

      @locus.trace("service.boom")
      fail(): never {
        throw new Error("boom");
      }
    }

    const service = new Service();
    expect(service.compute(3)).toBe(31);
    expect(await service.fetch("abc")).toBe("item:abc");
    expect(() => service.fail()).toThrow("boom");

    await locus.flush();
    await locus.close();

    const atoms = calls.flatMap((call) => call.lines.map(decode));
    const names = atoms.map((item) => `${item.context["locus.schema"]}:${item.source?.function}:${item.source?.edge}`);
    expect(names).toEqual([
      "locus.trace:compute:enter",
      "locus.span:service.step:enter",
      "locus.span:service.step:return",
      "locus.trace:compute:return",
      "locus.trace:service.fetch:enter",
      "locus.span:load:enter",
      "locus.span:load:return",
      "locus.trace:service.fetch:return",
      "locus.trace:service.boom:enter",
      "locus.trace:service.boom:return",
    ]);

    const ok = atoms[3];
    expect(ok.payload?.status).toBe("ok");
    expect(BigInt(String(ok.payload?.elapsed))).toBeGreaterThanOrEqual(0n);

    const errored = atoms[9];
    expect(errored.payload?.status).toBe("error");
  });

  it("supports callable trace, span, and with wrappers for sync and async functions", async () => {
    const { calls, fetcher } = recorder();
    const locus = new Locus({
      producer: "svelte-app",
      endpoint: "https://ingest.test/api/v1/atoms",
      fetcher,
      database: null,
    });

    const add = locus.trace("math.add", (a: number, b: number) => a + b);
    function multiply(a: number, b: number): number {
      return a * b;
    }
    const wrapped = locus.trace(multiply);
    const asyncspan = locus.span("async.work", async (label: string) => {
      await Promise.resolve();
      locus.info("inside " + label);
      return label.toUpperCase();
    });
    const rejecting = locus.span(async function reject() {
      await Promise.resolve();
      throw new TypeError("rejected");
    });

    expect(add(2, 3)).toBe(5);
    expect(wrapped(4, 5)).toBe(20);
    const result = await locus.with({ route: "/orders", tenant: "t1" }, () => asyncspan("ok"));
    expect(result).toBe("OK");
    await expect(rejecting()).rejects.toThrow(TypeError);

    await locus.flush();
    await locus.close();

    const atoms = calls.flatMap((call) => call.lines.map(decode));
    const log = atoms.find((item) => item.context["locus.schema"] === "locus.log");
    expect(log?.context.route).toBe("/orders");
    expect(log?.context.tenant).toBe("t1");
    expect(log?.payload?.message).toBe("inside ok");

    const failed = atoms.find(
      (item) => item.source?.function === "reject" && item.source?.edge === "return",
    );
    expect(failed?.payload?.status).toBe("error");
  });
});
