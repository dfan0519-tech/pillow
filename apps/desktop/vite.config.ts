import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

export default defineConfig({
  clearScreen: false,
  plugins: [react()],
  server: {
    strictPort: true,
  },
  test: {
    css: true,
    environment: "jsdom",
    setupFiles: ["./tests/setup.ts"],
  },
});
