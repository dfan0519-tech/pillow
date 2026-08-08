/**
 * Wave C1 desktop visual acceptance.
 *
 * Uses the deterministic screenshot harness rather than a live Tauri profile.
 * It deliberately relies only on stable accessible names and data-testid values
 * shared with renderer tests, so it can be run before or after the UI merge.
 */
import assert from "node:assert/strict";
import { access, mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { createRequire } from "node:module";
import { fileURLToPath, pathToFileURL } from "node:url";

const repositoryRoot = fileURLToPath(new URL("..", import.meta.url));
const desktopRoot = path.join(repositoryRoot, "apps", "desktop");
const outputRoot = path.join(repositoryRoot, "artifacts", "wave-c1-visual");
const desktopRequire = createRequire(path.join(desktopRoot, "package.json"));
const rootRequire = createRequire(path.join(repositoryRoot, "package.json"));

const viewports = [
  { height: 800, width: 1280 },
  { height: 900, width: 1440 },
  { height: 1080, width: 1920 },
];
const states = ["empty", "selected-project", "active-conversation"];
const chromeCandidates = [
  process.env.CHROME_PATH,
  process.env.EDGE_PATH,
  "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
  "C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe",
  "C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe",
  "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe",
].filter(Boolean);

function resolveModule(name) {
  try {
    return desktopRequire.resolve(name);
  } catch {
    return rootRequire.resolve(name);
  }
}

async function resolveBrowser() {
  for (const candidate of chromeCandidates) {
    try {
      await access(candidate);
      return candidate;
    } catch {
      // Keep searching for an installed desktop browser.
    }
  }
  throw new Error("Wave C1 requires Chrome or Edge. Set CHROME_PATH or EDGE_PATH.");
}

async function waitForHarness(page) {
  await page.waitForFunction(
    () => document.documentElement.dataset.screenshotReady === "ready",
    { timeout: 15_000 },
  );
}

function rectInside(rect, viewport, label) {
  assert.ok(rect.width > 0 && rect.height > 0, `${label} must have a visible box.`);
  assert.ok(rect.left >= -1, `${label} starts outside the viewport.`);
  assert.ok(rect.top >= -1, `${label} starts above the viewport.`);
  assert.ok(rect.right <= viewport.width + 1, `${label} overflows horizontally.`);
  assert.ok(rect.bottom <= viewport.height + 1, `${label} overflows vertically.`);
}

function cssColorIsVisible(color) {
  const normalized = color.trim().toLowerCase();
  if (!normalized || normalized === "transparent") return false;

  const alphaMatch = normalized.match(/\/\s*([\d.]+)%?\s*\)$/u)
    ?? normalized.match(/(?:rgba|hsla)\([^)]*,\s*([\d.]+)%?\s*\)$/u);
  if (!alphaMatch) return true;

  const alpha = Number.parseFloat(alphaMatch[1]);
  if (!Number.isFinite(alpha)) return false;
  return alpha > 0;
}

function focusIndicatorIsVisible(focus) {
  const outlineWidth = Number.parseFloat(focus.outlineWidth);
  const outlineIsVisible = focus.outlineStyle !== "none"
    && Number.isFinite(outlineWidth)
    && outlineWidth > 0
    && cssColorIsVisible(focus.outlineColor);

  const shadowLengths = [...focus.boxShadow.matchAll(/(-?[\d.]+)px/gu)]
    .map((match) => Number.parseFloat(match[1]));
  const shadowColors = focus.boxShadow.match(
    /(?:rgba?|hsla?|color)\([^)]*\)|#[\da-f]{3,8}/giu,
  ) ?? [];
  const shadowIsVisible = focus.boxShadow !== "none"
    && shadowLengths.some((length) => Number.isFinite(length) && Math.abs(length) > 0)
    && shadowColors.some(cssColorIsVisible);

  return outlineIsVisible || shadowIsVisible;
}

async function assertGeometry(page, state, viewport) {
  const snapshot = await page.evaluate(() => {
    const box = (selector) => {
      const element = document.querySelector(selector);
      if (!element) return null;
      const rect = element.getBoundingClientRect();
      return { bottom: rect.bottom, height: rect.height, left: rect.left, right: rect.right, top: rect.top, width: rect.width };
    };
    const sidebar = document.querySelector("[data-testid='app-sidebar']");
    const workbench = document.querySelector("[data-testid='workbench']");
    const composer = document.querySelector("[data-testid='composer']");
    const stageBody = document.querySelector(".stage-body");
    return {
      bodyScrolls: document.body.scrollHeight > window.innerHeight + 1
        || document.body.scrollWidth > window.innerWidth + 1,
      composer: box("[data-testid='composer']"),
      duplicateRails: document.querySelectorAll("[data-testid='app-sidebar']").length,
      forbiddenAdminSurface: Boolean(document.querySelector(
        ".dashboard, .dashboard-grid, .metric-card, .stat-card, .project-launcher, .project-sidebar, .conversation-sidebar",
      )),
      sidebar: box("[data-testid='app-sidebar']"),
      sidebarOverflow: sidebar ? getComputedStyle(sidebar).overflowY : "missing",
      stageBody: box(".stage-body"),
      stageBodyOverflow: stageBody ? getComputedStyle(stageBody).overflowY : "missing",
      workbench: box("[data-testid='workbench']"),
    };
  });

  assert.equal(snapshot.duplicateRails, 1, "C1 needs exactly one continuous left rail.");
  assert.equal(snapshot.forbiddenAdminSurface, false, "Generic admin-dashboard surface detected.");
  assert.equal(snapshot.bodyScrolls, false, "The desktop shell must not create a document scroll trap.");
  rectInside(snapshot.sidebar, viewport, `${state} sidebar`);
  rectInside(snapshot.workbench, viewport, `${state} workbench`);
  rectInside(snapshot.composer, viewport, `${state} composer`);
  rectInside(snapshot.stageBody, viewport, `${state} stage body`);
  assert.ok(
    snapshot.workbench.width >= viewport.width * 0.6,
    `${state} conversation canvas is too narrow for a desktop workbench.`,
  );
  assert.ok(
    snapshot.composer.left >= snapshot.workbench.left - 1,
    `${state} composer must stay inside the conversation canvas.`,
  );
  assert.ok(
    snapshot.composer.bottom <= snapshot.workbench.bottom + 1,
    `${state} composer must stay inside the workbench.`,
  );
  assert.notEqual(snapshot.stageBodyOverflow, "visible", "Timeline scrolling must be owned by the stage body.");
}

async function assertAccessibilityAndFocus(page, state) {
  const accessibility = await page.evaluate(() => {
    const input = document.querySelector("#work-request");
    const send = document.querySelector("[data-testid='composer-send']");
    const nav = document.querySelector("[data-testid='app-sidebar']");
    return {
      inputLabel: input?.labels?.[0]?.textContent?.trim(),
      inputPresent: Boolean(input),
      navLabel: nav?.getAttribute("aria-label"),
      sendLabel: send?.getAttribute("aria-label"),
    };
  });
  assert.equal(accessibility.inputPresent, true, "Composer input is required.");
  assert.ok(accessibility.inputLabel, "Composer input needs a programmatic label.");
  assert.ok(accessibility.navLabel, "Sidebar needs an accessible navigation label.");
  assert.ok(accessibility.sendLabel, "Send control needs an accessible name.");

  const focusTarget = state === "empty"
    ? "[data-testid='nav-open-project']"
    : "#work-request";
  await page.waitForFunction((selector) => {
    const target = document.querySelector(selector);
    return target instanceof HTMLElement && !target.matches(":disabled");
  }, focusTarget, { timeout: 15_000 });
  const focus = await page.evaluate((selector) => {
    const target = document.querySelector(selector);
    if (!(target instanceof HTMLElement)) {
      return {
        boxShadow: "none",
        focused: false,
        outlineColor: "transparent",
        outlineStyle: "none",
        outlineWidth: "0px",
        rect: null,
      };
    }
    target.focus();
    const rect = target.getBoundingClientRect();
    const style = getComputedStyle(target);
    return {
      boxShadow: style?.boxShadow,
      focused: document.activeElement === target,
      outlineColor: style?.outlineColor,
      outlineStyle: style?.outlineStyle,
      outlineWidth: style?.outlineWidth,
      rect: rect ? { bottom: rect.bottom, left: rect.left, right: rect.right, top: rect.top } : null,
    };
  }, focusTarget);
  assert.equal(focus.focused, true, "Keyboard focus must reach an enabled primary control.");
  assert.ok(focus.rect && focus.rect.top >= 0 && focus.rect.bottom <= (await page.viewportSize()).height, "Focused control is clipped.");
  assert.ok(
    focusIndicatorIsVisible(focus),
    `Focused control must have a non-transparent outline or box shadow; received ${JSON.stringify({
      boxShadow: focus.boxShadow,
      outlineColor: focus.outlineColor,
      outlineStyle: focus.outlineStyle,
      outlineWidth: focus.outlineWidth,
    })}.`,
  );
}

async function assertReducedMotion(page) {
  await page.emulateMedia({ reducedMotion: "reduce" });
  const result = await page.evaluate(() => {
    const composer = document.querySelector("[data-testid='composer']");
    const style = composer ? getComputedStyle(composer) : null;
    const duration = (value) => value.split(",").reduce((max, token) => {
      const milliseconds = token.trim().endsWith("ms")
        ? Number.parseFloat(token)
        : Number.parseFloat(token) * 1000;
      return Math.max(max, Number.isFinite(milliseconds) ? milliseconds : 0);
    }, 0);
    return {
      matched: window.matchMedia("(prefers-reduced-motion: reduce)").matches,
      transitionMs: duration(style?.transitionDuration ?? "0s"),
    };
  });
  assert.equal(result.matched, true, "Reduced-motion media emulation did not apply.");
  assert.ok(result.transitionMs <= 1, "Composer motion must be disabled for reduced-motion users.");
}

async function driveActiveConversation(page) {
  await page.getByRole("button", { name: "新建对话" }).click();
  await page.locator("#work-request").fill("C1 visual acceptance conversation");
  await page.locator("[data-testid='composer-send']").click();
  await page.waitForSelector("[data-testid='timeline-user-message']", { timeout: 15_000 });
  await page.waitForSelector("[data-testid='timeline-assistant-message']", { timeout: 15_000 });
}

async function prepareState(page, state) {
  const screenshotState = state === "empty" ? "empty" : "selected";
  await page.goto(`http://127.0.0.1:4182/screenshot.html?state=${screenshotState}`, { waitUntil: "networkidle" });
  await waitForHarness(page);
  if (state === "empty") {
    await page.waitForSelector("[data-testid='empty-workbench']");
  } else {
    await page.waitForSelector("[data-testid='selected-workbench']");
    if (state === "active-conversation") await driveActiveConversation(page);
  }
}

/**
 * Read the resolved surface token so a skin switch can be proven, not assumed.
 */
async function readAppSurface(page) {
  return page.evaluate(() =>
    getComputedStyle(document.documentElement).getPropertyValue("--bg-app").trim(),
  );
}

/** Rough channel mean of `#rrggbb`; enough to tell a light skin from a dark one. */
function channelMean(color) {
  const hex = color.replace("#", "");
  assert.equal(hex.length, 6, `Expected #rrggbb surface token, got "${color}".`);
  const value = Number.parseInt(hex, 16);
  return (((value >> 16) & 0xff) + ((value >> 8) & 0xff) + (value & 0xff)) / 3;
}

/**
 * Light-skin capture.
 *
 * The dual-theme work shipped with dark-skin screenshots only, so a light-skin
 * regression had nothing to fail against. Flipping the attribute is not enough
 * on its own: if the light token set failed to apply, the page would still
 * screenshot cleanly and the file would be a dark shot named "light" — the
 * false-evidence shape this repository keeps producing. So the surface token is
 * read before and after, and a capture is only recorded once it has actually
 * changed and actually got lighter.
 */
async function captureLightSkin(page, state, viewport, captures) {
  const dark = await readAppSurface(page);
  await page.evaluate(() => {
    document.documentElement.setAttribute("data-theme", "light");
  });
  await page.waitForFunction(
    (previous) =>
      getComputedStyle(document.documentElement).getPropertyValue("--bg-app").trim() !== previous,
    dark,
    { timeout: 5_000 },
  );
  const light = await readAppSurface(page);
  assert.ok(
    channelMean(light) > channelMean(dark) + 40,
    `Light skin did not lighten the app surface (dark ${dark} -> light ${light}).`,
  );

  await assertGeometry(page, state, viewport);
  const file = `${state}-light-${viewport.width}x${viewport.height}.png`;
  await page.screenshot({ path: path.join(outputRoot, file), fullPage: false });
  captures.push({
    file,
    state,
    skin: "light",
    surface: light,
    viewport: `${viewport.width}x${viewport.height}`,
  });
}

/**
 * Settings surface capture. It is reachable from the shell but no spec opened
 * it, so the whole settings screen shipped without visual evidence in either
 * skin.
 */
async function captureSettings(page, viewport, captures) {
  await page.locator("[data-testid='nav-open-settings']").click();
  await page.waitForSelector("[data-testid='settings-panel']", { timeout: 10_000 });
  const file = `settings-${viewport.width}x${viewport.height}.png`;
  await page.screenshot({ path: path.join(outputRoot, file), fullPage: false });
  captures.push({ file, state: "settings", viewport: `${viewport.width}x${viewport.height}` });
}

async function main() {
  await mkdir(outputRoot, { recursive: true });
  const browserPath = await resolveBrowser();
  const { createServer } = await import(pathToFileURL(resolveModule("vite")).href);
  const reactModule = await import(pathToFileURL(resolveModule("@vitejs/plugin-react")).href);
  const playwrightModule = await import(pathToFileURL(resolveModule("playwright-core")).href);
  const react = reactModule.default;
  const playwright = playwrightModule.default ?? playwrightModule;
  const server = await createServer({
    configFile: false,
    plugins: [react()],
    root: desktopRoot,
    server: { host: "127.0.0.1", port: 4182, strictPort: true },
  });
  await server.listen();
  const browser = await playwright.chromium.launch({ executablePath: browserPath, headless: true });
  const captures = [];
  try {
    for (const state of states) {
      for (const viewport of viewports) {
        const page = await browser.newPage({ deviceScaleFactor: 1, viewport });
        try {
          await prepareState(page, state);
          await assertGeometry(page, state, viewport);
          await assertAccessibilityAndFocus(page, state);
          await assertReducedMotion(page);
          const file = `${state}-${viewport.width}x${viewport.height}.png`;
          await page.screenshot({ path: path.join(outputRoot, file), fullPage: false });
          captures.push({ file, state, skin: "dark", viewport: `${viewport.width}x${viewport.height}` });

          // Light skin last: it mutates the document, so geometry and a11y
          // assertions above still run against the shipped default.
          await captureLightSkin(page, state, viewport, captures);
          if (state === "selected-project") await captureSettings(page, viewport, captures);
        } finally {
          await page.close();
        }
      }
    }
  } finally {
    await browser.close();
    await server.close();
  }
  const report = { captures, status: "passed" };
  await writeFile(path.join(outputRoot, "report.json"), `${JSON.stringify(report, null, 2)}\n`);
  console.log(`KEEPER_WAVE_C1_VISUAL_ACCEPTANCE ${JSON.stringify(report)}`);
}

main().catch((error) => {
  console.error(`KEEPER_WAVE_C1_VISUAL_ACCEPTANCE ${JSON.stringify({ status: "failed", error: error.message })}`);
  process.exitCode = 1;
});
