import { design } from "@perishlab/design/vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [design({ login: false, serve: false }), svelte()],
});
