/**
 * Rasterise the Pillow brand mark into the Tauri icon set.
 *
 * The mark lives as SVG (`icons/pillow-mark.svg`) so it stays editable and in
 * step with `BrandMark.tsx`. Tauri wants PNG/ICO, and `tauri icon` wants a
 * single square PNG source — so this renders the SVG once at 1024 and hands
 * that to the Tauri CLI.
 *
 * Rendering uses the Chrome already installed on the machine through
 * playwright-core, which is already a devDependency. No new toolchain.
 *
 * Usage: node scripts/generate-app-icons.mjs
 *
 * After running this, the next `cargo build` will NOT pick the new icon up on
 * its own: cargo does not treat the icon files as inputs to tauri-build's build
 * script, so the previously embedded Windows resource survives and the binary
 * keeps the old icon while everything reports success. Touch
 * `apps/desktop/src-tauri/build.rs` (this script does) before rebuilding, and
 * confirm by extracting the icon from the built exe rather than trusting the
 * build's exit code.
 */

import { spawn } from "node:child_process";
import { readFile, writeFile, mkdtemp, rm } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const repositoryRoot = fileURLToPath(new URL("..", import.meta.url));
const SOURCE_SVG = path.join(repositoryRoot, "apps/desktop/src-tauri/icons/pillow-mark.svg");
const SIZE = 1024;

async function renderSourcePng(outputPath) {
  const playwright = await import("playwright-core");
  const chromium = playwright.chromium ?? playwright.default?.chromium;
  if (!chromium?.launch) throw new Error("playwright-core chromium is unavailable.");

  const svg = await readFile(SOURCE_SVG, "utf8");
  // Inline the SVG rather than loading file:// so no path leaks into the page.
  const html = `<!doctype html><meta charset="utf-8">
<style>html,body{margin:0;padding:0;background:transparent}
#icon-canvas{width:${SIZE}px;height:${SIZE}px;display:block}</style>
<div id="icon-canvas">${svg}</div>`;

  const browser = await chromium.launch({ channel: "chrome" });
  try {
    const page = await browser.newPage({
      viewport: { width: SIZE, height: SIZE },
      deviceScaleFactor: 1,
    });
    await page.setContent(html, { waitUntil: "load" });
    const plate = await page.$("#icon-canvas");
    if (!plate) throw new Error("icon plate element missing after render");
    await plate.screenshot({ path: outputPath, omitBackground: true });
  } finally {
    await browser.close();
  }
}

function runTauriIcon(sourcePng) {
  return new Promise((resolve, reject) => {
    const child = spawn(
      process.platform === "win32" ? "cmd.exe" : "pnpm",
      process.platform === "win32"
        ? ["/d", "/s", "/c", "pnpm", "--filter", "@pillow/desktop", "tauri", "icon", sourcePng]
        : ["--filter", "@pillow/desktop", "tauri", "icon", sourcePng],
      { cwd: repositoryRoot, stdio: "inherit" },
    );
    child.on("exit", (code, signal) => {
      if (signal) return reject(new Error(`tauri icon killed by ${signal}`));
      if (code !== 0) return reject(new Error(`tauri icon exited ${code}`));
      resolve();
    });
  });
}

/**
 * Touch the Tauri build script so the next build re-embeds the icon resource.
 * Without this the regenerated icons land on disk and the binary keeps the old
 * one, with every command still reporting success.
 */
async function invalidateBuildScript() {
  const buildScript = path.join(repositoryRoot, "apps/desktop/src-tauri/build.rs");
  const source = await readFile(buildScript, "utf8");
  await writeFile(buildScript, source, "utf8");
}

async function main() {
  const temporaryRoot = await mkdtemp(path.join(os.tmpdir(), "pillow-icon-"));
  const sourcePng = path.join(temporaryRoot, "pillow-icon-source.png");
  try {
    await renderSourcePng(sourcePng);
    await runTauriIcon(sourcePng);
    await invalidateBuildScript();
    console.log(`PILLOW_ICONS: regenerated from ${path.basename(SOURCE_SVG)}; build script touched`);
  } finally {
    await rm(temporaryRoot, { recursive: true, force: true }).catch(() => undefined);
  }
}

await main();
