import react from "@vitejs/plugin-react";
import { configDefaults, defineConfig } from "vitest/config";

export default defineConfig({
  clearScreen: false,
  plugins: [react()],
  server: {
    strictPort: true,
  },
  test: {
    css: true,
    environment: "jsdom",
    exclude: [...configDefaults.exclude, "tests/e2e/**"],
    setupFiles: ["./tests/setup.ts"],
  },
});
