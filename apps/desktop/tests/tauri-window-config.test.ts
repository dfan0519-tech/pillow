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

describe("Tauri delivery packaging configuration", () => {
  it("keeps product metadata free of placeholders", () => {
    expect(config.productName).toBe("Pillow");
    expect(config.identifier).toBe("com.pillow.desktop");
    expect(config.version).toBe("0.1.0");
    expect(desktopPackage.version).toBe("0.1.0");
    expect(config.productName).not.toMatch(/TODO|MyApp|tauri/i);
    expect(config.identifier).not.toMatch(/com\.tauri\.dev|example/i);
  });

  it("enables NSIS bundling with WebView2 downloadBootstrapper", () => {
    expect(config.bundle.active).toBe(true);
    expect(config.bundle.targets).toEqual(["nsis"]);
    expect(config.bundle.windows?.webviewInstallMode?.type).toBe(
      "downloadBootstrapper",
    );
    expect(config.bundle.windows?.nsis?.installMode).toBe("currentUser");
    // Icons are brand assets (not default Tauri blue square placeholder).
    expect(config.bundle.icon?.length).toBeGreaterThan(0);
    expect(config.bundle.icon).toEqual(
      expect.arrayContaining(["icons/icon.ico", "icons/icon.icns"]),
    );
  });
});

