import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach, beforeEach } from "vitest";

/**
 * FOUC guard in tokens.css hides `html:not([data-theme-ready])`.
 * Unit tests assert toBeVisible(); stamp ready so visibility checks remain
 * meaningful. Production App still owns the real stamp after resolve.
 */
beforeEach(() => {
  if (typeof document !== "undefined") {
    document.documentElement.setAttribute("data-theme-ready", "");
  }
});

afterEach(cleanup);
