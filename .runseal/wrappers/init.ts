import { cli, flags } from "@perish/sealkit/cli";
import { init } from "@perish/sealkit/init";
import { io } from "@perish/sealkit/io";

const args = cli.parse(Deno.args, { boolean: ["help", "h"] });
flags(args).positionals("init", { allowHelp: true });
if (flags(args).help()) {
  io.print("Usage: runseal :init");
  io.print("");
  io.print("Validate the repository and install versioned git hooks.");
  Deno.exit(0);
}

await init({
  tools: ["git", "tea", "deno", "cargo", "ectropy", "plumb", "runseal", "sh"],
  paths: [
    "Cargo.toml",
    "Cargo.lock",
    "ectropy.toml",
    "plumb.toml",
    "runseal.toml",
    "AGENTS.md",
    "README.md",
    "docs/model/laws.md",
    "docs/model/vocabulary.md",
    "docs/run/verify.md",
    "crates/locus/Cargo.toml",
    "crates/locus/src/lib.rs",
    "crates/macro/Cargo.toml",
    "crates/macro/src/lib.rs",
    ".runseal/deno.json",
    ".runseal/deno.lock",
    ".runseal/hooks/pre-commit",
    ".runseal/hooks/commit-msg",
    ".runseal/wrappers/guard.ts",
    ".runseal/wrappers/init.ts",
    ".runseal/wrappers/land.ts",
    ".runseal/wrappers/release.ts",
    ".forgejo/workflows/guard.yml",
  ],
});
