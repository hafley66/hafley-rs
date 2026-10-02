import alloyPlugin from "@alloy-js/rollup-plugin";
import { defineConfig } from "vitest/config";
export default defineConfig({
  esbuild: { jsx: "preserve" },
  test: { include: ["twins/**/*.test.ts", "twins/**/*.test.tsx", "rust/**/*.test.tsx"], maxWorkers: 1, fileParallelism: false },
  plugins: [alloyPlugin()],
});
