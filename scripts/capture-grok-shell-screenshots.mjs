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

/** DOM probe used immediately before streaming screenshots. */
async function readRunStatus(page) {
  return page.evaluate(() => {
    const status = document.querySelector("[data-testid='run-status']");
    return (status?.textContent ?? "").trim();
  });
}

async function readRunActivityText(page) {
  return page.evaluate(() => {
    const activity = document.querySelector("[data-testid='run-activity']");
    return activity?.textContent ?? "";
  });
}

/**
 * Final-moment gate: must still show 运行中 the instant before capture.
 * Throws if the run already terminalized (the previous race).
 */
async function assertStreamingActiveAtCapture(page) {
  const statusText = await readRunStatus(page);
  if (!/运行中|running/i.test(statusText)) {
    throw new Error(
      `Streaming screenshot refused: expected 运行中 at capture time, got "${statusText || "(missing)"}". Increase screenshot stepDelayMs.`,
    );
  }
  // Status chip itself must not already read completed/cancelled.
  if (/已完成|completed|已取消|cancelled|失败|failed/i.test(statusText)) {
    throw new Error(
      `Streaming screenshot refused: terminal status at capture time ("${statusText}").`,
    );
  }
}

/**
 * Final-moment gate: cancelled terminal with no live child activity labels.
 */
async function assertCancelledAtCapture(page) {
  const statusText = await readRunStatus(page);
  if (!/已取消|cancelled/i.test(statusText)) {
    throw new Error(
      `Cancelled screenshot refused: expected 已取消 at capture time, got "${statusText || "(missing)"}".`,
    );
  }
  const activity = await readRunActivityText(page);
  if (/\brunning\b|进行中/i.test(activity)) {
    throw new Error(
      "Cancelled screenshot refused: child activity still shows running/进行中.",
    );
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
    // Drive into an active mid-stream surface (status + assistant + activity).
    await page.waitForFunction(
      () => {
        const status = document.querySelector("[data-testid='run-status']");
        return status && /运行中|running/i.test(status.textContent ?? "");
      },
      { timeout: 15_000 },
    );
    await page.waitForSelector("[data-testid='timeline-assistant-message']", {
      timeout: 15_000,
    });
    await page.waitForSelector("[data-testid='run-activity']", { timeout: 10_000 });
    // Do not sleep here: any post-check delay is a race against turn_completed.
    // Final assert happens immediately before page.screenshot().
  }
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
        // Brief paint settle only — then re-assert at the final moment so the
        // PNG cannot claim a state the DOM no longer shows.
        await page.evaluate(
          () =>
            new Promise((resolve) => {
              requestAnimationFrame(() => requestAnimationFrame(resolve));
            }),
        );
        if (shot.drive === "streaming") {
          await assertStreamingActiveAtCapture(page);
        } else {
          await assertCancelledAtCapture(page);
        }
      } else if (shot.state === "selected") {
        await page.waitForSelector("[data-testid='active-project-name']");
        await page.waitForTimeout(150);
      } else {
        await page.waitForSelector("[data-testid='empty-workbench']");
        await page.waitForTimeout(150);
      }
      const target = path.join(outDir, shot.file);
      await page.screenshot({ path: target, fullPage: false });
      // Post-shot DOM claim for the report: re-read what the page still shows.
      if (shot.drive === "streaming") {
        const statusText = await readRunStatus(page);
        console.log(`wrote ${target} (status=${JSON.stringify(statusText)})`);
        if (!/运行中|running/i.test(statusText)) {
          throw new Error(
            `Post-shot streaming status drifted off 运行中: "${statusText}". PNG may be invalid.`,
          );
        }
      } else if (shot.drive === "cancelled") {
        const statusText = await readRunStatus(page);
        const activity = await readRunActivityText(page);
        console.log(`wrote ${target} (status=${JSON.stringify(statusText)})`);
        if (!/已取消|cancelled/i.test(statusText)) {
          throw new Error(
            `Post-shot cancelled status drifted: "${statusText}". PNG may be invalid.`,
          );
        }
        if (/\brunning\b|进行中/i.test(activity)) {
          throw new Error(
            "Post-shot cancelled activity still shows running/进行中. PNG may be invalid.",
          );
        }
      } else {
        console.log(`wrote ${target}`);
      }
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
