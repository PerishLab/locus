import { describe, expect, it } from "vitest";
import { Locus, type Diagnostic } from "@perishlab/locus";
import { decode, recorder } from "./fixture.js";

describe("records, logger, fault, metric, and wire format", () => {
  it("enforces string-only logs and reports non-string rejections to hook", async () => {
    const diagnostics: Diagnostic[] = [];
    const { calls, fetcher } = recorder();
    const locus = new Locus({
      producer: "web",
      endpoint: "https://ingest.test/api/v1/atoms",
      hook: (d) => diagnostics.push(d),
      fetcher,
      database: null,
    });

    locus.info("hello");
    locus.warn("careful");
    locus.error("broken");
    locus.info({ secret: "nope" } as unknown as string);
    locus.warn(42 as unknown as string);
    locus.error(null as unknown as string);

    await locus.flush();
    await locus.close();

    const atoms = calls.flatMap((call) => call.lines.map(decode));
    expect(atoms).toHaveLength(3);
    expect(atoms.map((a) => a.context.level)).toEqual(["info", "warn", "error"]);
    expect(atoms.map((a) => a.payload?.message)).toEqual(["hello", "careful", "broken"]);
    expect(diagnostics.filter((d) => d.code === "log.invalid")).toHaveLength(3);
  });

  it("emits scrubbed structured faults with truncated stacks", async () => {
    const { calls, fetcher } = recorder();
    const locus = new Locus({
      producer: "api",
      endpoint: "https://ingest.test/api/v1/atoms",
      fetcher,
      database: null,
    });

    const err = new RangeError("bad range");
    err.stack = Array.from({ length: 60 }, (_, i) => `at frame${i} (file.ts:${i})`).join("\n");
    locus.fault(err, {
      route: "/login",
      attempts: 3,
      retryable: true,
      password: "p",
      token: "t",
      secret: "s",
      authorization: "Bearer x",
      cookie: "sid=1",
      code_verifier: "v",
      rescue: "r",
      sudo: "yes",
    });

    await locus.flush();
    await locus.close();

    const atom = decode(calls[0].lines[0]);
    expect(atom.context["locus.schema"]).toBe("locus.fault");
    expect(atom.context["locus.edge"]).toBe("point");
    expect(atom.context["locus.fault"]).toBe("RangeError");
    expect(atom.payload?.name).toBe("RangeError");
    expect(atom.payload?.message).toBe("bad range");
    expect(String(atom.payload?.stack).split("\n").length).toBeLessThanOrEqual(32);
    expect(atom.payload?.detail).toEqual({
      route: "/login",
      attempts: 3,
      retryable: true,
    });
  });

  it("emits metric Atoms matching locus-derivation contract and rejects non-finite values", async () => {
    const diagnostics: Diagnostic[] = [];
    const { calls, fetcher } = recorder();
    const locus = new Locus({
      producer: "metrics-node",
      endpoint: "https://ingest.test/api/v1/atoms",
      hook: (d) => diagnostics.push(d),
      fetcher,
      database: null,
    });

    locus.metric({
      name: "requests",
      unit: "count",
      kind: "counter",
      series: "pod-1",
      value: 42,
      epoch: 7,
      labels: { method: "GET", status: "200" },
    });
    locus.metric({
      name: "latency",
      unit: "ms",
      kind: "gauge",
      series: "pod-1",
      value: Number.NaN,
    });
    locus.metric({
      name: "latency",
      unit: "ms",
      kind: "histogram",
      series: "pod-1",
      value: Number.POSITIVE_INFINITY,
    });
    locus.metric({
      name: "requests",
      unit: "count",
      kind: "counter",
      series: "pod-1",
      value: -5,
    });
    locus.metric({
      name: 123 as unknown as string,
      unit: "count",
      kind: "gauge",
      series: "pod-1",
      value: 1,
    });
    locus.metric({
      name: "requests",
      unit: "count",
      kind: "gauge",
      series: "pod-1",
      value: 1,
      labels: { bad: 99 as unknown as string },
    });

    await locus.flush();
    await locus.close();

    const atoms = calls.flatMap((call) => call.lines.map(decode));
    expect(atoms).toHaveLength(1);
    const atom = atoms[0];
    expect(atom.context["locus.schema"]).toBe("locus.metric");
    expect(atom.context["metric.name"]).toBe("requests");
    expect(atom.context["metric.unit"]).toBe("count");
    expect(atom.context["metric.kind"]).toBe("counter");
    expect(atom.context["metric.series"]).toBe("pod-1");
    expect(atom.context["metric.epoch"]).toBe("7");
    expect(atom.context["metric.label.method"]).toBe("GET");
    expect(atom.context["metric.label.status"]).toBe("200");
    expect(atom.payload).toEqual({ value: 42 });
    expect(diagnostics.filter((d) => d.code === "metric.invalid")).toHaveLength(5);
  });

  it("serializes BigInt nanoseconds as unquoted JSON integers and drops > 32 KiB records", async () => {
    const diagnostics: Diagnostic[] = [];
    const { calls, fetcher } = recorder();
    const locus = new Locus({
      producer: "wire-check",
      endpoint: "https://ingest.test/api/v1/atoms",
      hook: (d) => diagnostics.push(d),
      fetcher,
      database: null,
    });

    locus.info("normal");
    locus.info("x".repeat(33 * 1024));

    await locus.flush();
    await locus.close();

    expect(calls).toHaveLength(1);
    expect(calls[0].text.endsWith("\n")).toBe(true);
    const line = calls[0].lines[0];
    const match = /^\{"at":(\d{19,20}),"id":"([0-9a-f]{32})","producer":"wire-check","context":\{/.exec(line);
    expect(match).not.toBeNull();
    expect(BigInt(match![1])).toBeGreaterThan(BigInt(Number.MAX_SAFE_INTEGER));
    const parsed = decode(line);
    expect(Array.isArray(parsed.choices)).toBe(true);
    expect(diagnostics.some((d) => d.code === "record.oversize")).toBe(true);
  });
});
