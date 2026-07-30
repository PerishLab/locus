import { cli, flags } from "@perish/sealkit/cli";
import { bin } from "@perish/sealkit/cmd";
import { io } from "@perish/sealkit/io";

const INDEX = "https://git.perish.top/api/packages/PerishLab/cargo";
const CRATES = ["locus-macro", "locus"];

const args = cli.parse(Deno.args, { boolean: ["help", "h"] });
if (flags(args).help()) {
  flags(args).positionals("release", { allowHelp: true });
  io.print("Usage: runseal :release");
  io.print("");
  io.print("Publish the coupled locus crates from a clean main.");
  Deno.exit(0);
}
flags(args).positionals("release");

const root = await bin("git").text(["rev-parse", "--show-toplevel"]);
const branch = await bin("git").text(["branch", "--show-current"]);
if (branch !== "main") {
  io.fail(`release: publish from main, not ${branch}`);
}
if ((await bin("git").text(["status", "--short"])).trim() !== "") {
  io.fail("release: working tree must be clean");
}

const version = await current(root);
io.print(`==> release v${version}`);
for (const crate of CRATES) {
  if (await published(crate, version)) {
    io.print(`==> ${crate} v${version} already in the registry`);
    continue;
  }
  await bin("cargo").run(
    ["publish", "-p", crate, "--registry", "perish"],
    { cwd: root },
  );
  await readable(crate, version);
}
io.print("release: clean");

async function current(root: string): Promise<string> {
  const text = await Deno.readTextFile(`${root}/Cargo.toml`);
  const hit = text.match(/^version = "([^"]+)"$/m);
  if (!hit) {
    return io.fail("release: missing workspace version");
  }
  return hit[1];
}

async function published(crate: string, version: string): Promise<boolean> {
  const response = await fetch(`${INDEX}/${shelf(crate)}/${crate}`);
  if (!response.ok) {
    await response.body?.cancel();
    return false;
  }
  return (await response.text()).trim().split("\n").some((row) => {
    try {
      return JSON.parse(row).vers === version;
    } catch {
      return false;
    }
  });
}

async function readable(crate: string, version: string): Promise<void> {
  const deadline = Date.now() + 60_000;
  while (Date.now() < deadline) {
    if (await published(crate, version)) {
      io.print(`==> ${crate} v${version} readable from the registry`);
      return;
    }
    await new Promise((resolve) => setTimeout(resolve, 1_000));
  }
  io.fail(`release: ${crate} v${version} was not readable after 60s`);
}

function shelf(crate: string): string {
  if (crate.length === 1) return "1";
  if (crate.length === 2) return "2";
  if (crate.length === 3) return `3/${crate[0]}`;
  return `${crate.slice(0, 2)}/${crate.slice(2, 4)}`;
}
