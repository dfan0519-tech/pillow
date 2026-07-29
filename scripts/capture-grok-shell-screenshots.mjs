/**
 * Capture shell screenshots at supported Windows sizes.
 * States: empty, selected (Slice A), streaming, cancelled (B1 conversation).
 * Uses system Edge/Chrome + Vite preview of the screenshot harness.
 */
import { createRequire } from "node:module";
import { mkdir, access } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const repositoryRoot = fileURLToPath(new URL("..", import.meta.url));
const desktopRoot = path.join(repositoryRoot, "apps", "desktop");
const outDir = path.join(repositoryRoot, "artifacts", "grok-shell-prototype");
const desktopRequire = createRequire(path.join(desktopRoot, "package.json"));
const rootRequire = createRequire(path.join(repositoryRoot, "package.json"));

const chromeCandidates = [
  process.env.CHROME_PATH,
  process.env.EDGE_PATH,
  "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
  "C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe",
  "C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe",
  "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe",
].filter(Boolean);

async function resolveBrowser() {
  for (const candidate of chromeCandidates) {
    try {
      await access(candidate);
      return candidate;
    } catch {
      /* try next */
    }
  }
  throw new Error("Chrome or Edge not found for screenshot capture.");
}

async function waitForReady(page, timeoutMs = 15_000) {
  const started = Date.now();
  while (Date.now() - started < timeoutMs) {
    const ready = await page.evaluate(
      () => document.documentElement.dataset.screenshotReady === "ready",
    );
    if (ready) return;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  throw new Error("Screenshot harness did not become ready.");
}

function resolveModule(name) {
  try {
    return desktopRequire.resolve(name);
  } catch {
    return rootRequire.resolve(name);
  }
}

async function driveStreaming(page, mode) {
  await page.waitForSelector("[data-testid='active-project-name']");
  await page.click('button:has-text("新建对话")');
  await page.waitForSelector("#work-request:not([disabled])");
  await page.fill("#work-request", mode === "cancelled" ? "cancel capture" : "stream capture");
  await page.click("[data-testid='composer-send']");
  await page.waitForSelector("[data-testid='timeline-user-message']", {
    timeout: 10_000,
  });
  if (mode === "cancelled") {
    await page.waitForSelector("[data-testid='composer-cancel']:not([disabled])", {
      timeout: 10_000,
    });
    await page.click("[data-testid='composer-cancel']");
    await page.waitForFunction(
      () => {
        const status = document.querySelector("[data-testid='run-status']");
        return status && /已取消|cancelled/i.test(status.textContent ?? "");
      },
      { timeout: 15_000 },
    );
    // Ensure child activity is not left as "running".
    await page.waitForFunction(
      () => {
        const activity = document.querySelector("[data-testid='run-activity']");
        if (!activity) return true;
        return !/\brunning\b|进行中/i.test(activity.textContent ?? "");
      },
      { timeout: 5_000 },
    );
  } else {
    // Capture while the run is still active — not after turn_completed.
    await page.waitForFunction(
      () => {
        const status = document.querySelector("[data-testid='run-status']");
        return status && /运行中|running/i.test(status.textContent ?? "");
      },
      { timeout: 15_000 },
    );
    await page.waitForSelector("[data-testid='timeline-assistant-message']", {
      timeout: 10_000,
    });
    await page.waitForSelector("[data-testid='run-activity']", { timeout: 10_000 });
    // Confirm still active at capture time (slow fixture keeps stream open).
    const stillActive = await page.evaluate(() => {
      const status = document.querySelector("[data-testid='run-status']");
      return status && /运行中|running|取消中|cancelling/i.test(status.textContent ?? "");
    });
    if (!stillActive) {
      throw new Error(
        "Streaming screenshot would capture a completed run; increase stepDelayMs.",
      );
    }
  }
  await page.waitForTimeout(120);
}

async function main() {
  await mkdir(outDir, { recursive: true });
  const browserPath = await resolveBrowser();

  const { createServer } = await import(pathToFileURL(resolveModule("vite")).href);
  const reactPluginModule = await import(
    pathToFileURL(resolveModule("@vitejs/plugin-react")).href
  );
  const react = reactPluginModule.default;
  const playwrightModule = await import(
    pathToFileURL(resolveModule("playwright-core")).href
  );
  const playwright = playwrightModule.default ?? playwrightModule;
  const { chromium } = playwright;
  if (!chromium) {
    throw new Error("playwright-core chromium export is unavailable.");
  }

  const server = await createServer({
    configFile: false,
    root: desktopRoot,
    plugins: [react()],
    server: { host: "127.0.0.1", port: 4181, strictPort: true },
  });
  await server.listen();

  const base = "http://127.0.0.1:4181/screenshot.html";
  const shots = [
    { state: "empty", width: 1440, height: 960, file: "empty-1440x960.png" },
    { state: "empty", width: 1024, height: 720, file: "empty-1024x720.png" },
    { state: "selected", width: 1440, height: 960, file: "selected-1440x960.png" },
    { state: "selected", width: 1024, height: 720, file: "selected-1024x720.png" },
    {
      state: "selected",
      width: 1440,
      height: 960,
      file: "streaming-1440x960.png",
      drive: "streaming",
    },
    {
      state: "selected",
      width: 1024,
      height: 720,
      file: "streaming-1024x720.png",
      drive: "streaming",
    },
    {
      state: "selected",
      width: 1440,
      height: 960,
      file: "cancelled-1440x960.png",
      drive: "cancelled",
    },
    {
      state: "selected",
      width: 1024,
      height: 720,
      file: "cancelled-1024x720.png",
      drive: "cancelled",
    },
  ];

  const browser = await chromium.launch({
    executablePath: browserPath,
    headless: true,
  });

  try {
    for (const shot of shots) {
      const page = await browser.newPage({
        viewport: { width: shot.width, height: shot.height },
        deviceScaleFactor: 1,
      });
      await page.goto(`${base}?state=${shot.state}`, {
        waitUntil: "networkidle",
      });
      await waitForReady(page);
      if (shot.drive) {
        await driveStreaming(page, shot.drive);
      } else if (shot.state === "selected") {
        await page.waitForSelector("[data-testid='active-project-name']");
      } else {
        await page.waitForSelector("[data-testid='empty-workbench']");
      }
      // Allow layout paint after selection effect.
      await page.waitForTimeout(150);
      const target = path.join(outDir, shot.file);
      await page.screenshot({ path: target, fullPage: false });
      console.log(`wrote ${target}`);
      await page.close();
    }
  } finally {
    await browser.close();
    await server.close();
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
