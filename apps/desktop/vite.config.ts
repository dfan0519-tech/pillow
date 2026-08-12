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
    /**
     * Vitest defaults to 5s. Interaction-heavy suites here drive userEvent one
     * keystroke at a time (a 43-char key is 43 act() cycles), which lands close
     * enough to 5s that a loaded machine turns green tests red — observed at
     * 5037ms while two workers were building. That is a false-red generator,
     * not a signal. A genuinely hung test still fails, just later.
     */
    testTimeout: 20_000,
  },
});
