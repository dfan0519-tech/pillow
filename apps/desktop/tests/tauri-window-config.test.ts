import { describe, expect, it } from "vitest";
import desktopPackage from "../package.json";
import capability from "../src-tauri/capabilities/default.json";
import config from "../src-tauri/tauri.conf.json";

describe("Tauri custom titlebar configuration", () => {
  it("disables native window decorations", () => {
    expect(config.app.windows[0].decorations).toBe(false);
  });

  it("allows every window action and resize subscription used by the titlebar", () => {
    expect(capability.permissions).toEqual(expect.arrayContaining([
      "core:window:allow-minimize",
      "core:window:allow-toggle-maximize",
      "core:window:allow-close",
      "core:window:allow-is-maximized",
      "core:window:allow-start-dragging",
      "core:event:allow-listen",
      "core:event:allow-unlisten",
    ]));
  });

  it("pins the dev server and Tauri probe to the same IPv4 loopback host", () => {
    expect(config.build.devUrl).toBe("http://127.0.0.1:1420");
    expect(desktopPackage.scripts.dev).toContain("--host 127.0.0.1");
  });
});
