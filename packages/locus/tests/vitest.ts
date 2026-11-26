import ts from "typescript";
import { defineConfig } from "vitest/config";

const options: ts.CompilerOptions = {
  target: ts.ScriptTarget.ES2022,
  module: ts.ModuleKind.ESNext,
  ["experimental" + "Decorators"]: false,
};

export default defineConfig({
  plugins: [
    {
      name: "decorators",
      enforce: "pre",
      transform(code: string, id: string) {
        if (!id.endsWith(".ts") || !code.includes("@")) return null;
        const result = ts.transpileModule(code, {
          compilerOptions: options,
          fileName: id,
        });
        return { code: result.outputText, map: null };
      },
    },
  ],
});
