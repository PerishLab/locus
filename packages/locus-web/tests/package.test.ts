import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { asset, read } from "@perishlab/crest/assets";

test("compiled output contains and verifies the official Locus mark", async () => {
  const catalog = read(JSON.parse(await readFile(new URL(import.meta.resolve("@perishlab/crest/media/manifest.json")), "utf8")));
  for (const surface of ["light", "dark"] as const) {
    const item = asset(catalog, { product: "locus", surface, form: "sign", step: "full", medium: "svg", size: 100 });
    const digest = item.digest.slice(7);
    const bytes = await readFile("dist/crest/" + digest + ".svg");
    assert.equal(item.kind, "shared");
    assert.equal(createHash("sha256").update(bytes).digest("hex"), digest);
  }
});
