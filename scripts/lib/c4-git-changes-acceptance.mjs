/**
 * Wave C4 D3B4 Round 3 — Git Changes WebView2 UI helpers.
 *
 * Public accessible roles/names and stable data-testid values only.
 * Does not launch/close processes, call the Git executor, inject renderer
 * state, or mutate project fixtures. Returns JSON-serializable evidence only.
 */

import assert from "node:assert/strict";

const DEFAULT_TIMEOUT_MS = 60_000;

/** Fixed path-free public error copy from gitGateway.ts (never host-echoed). */
export const GIT_PUBLIC_ERROR_TEXT = Object.freeze({
  git_project_not_found: "Git project was not found.",
  git_path_rejected: "Git path was rejected.",
  git_repository_not_found: "Git repository was not found.",
  git_unavailable: "Git read capability is unavailable.",
  git_generation_stale: "Git result is stale; refresh changes.",
  git_generation_mismatch: "Git repository identity changed; refresh changes.",
});

export const GIT_KIND_LABEL = Object.freeze({
  modified: "修改",
  added: "新增",
  deleted: "删除",
  untracked: "未跟踪",
  renamed: "重命名",
  copied: "复制",
  typechange: "类型变更",
  conflict: "冲突",
  submodule: "子模块",
  ignored: "已忽略",
  unknown: "未知",
});

export const GIT_TEST_IDS = Object.freeze({
  view: "git-changes-view",
  pane: "git-changes-pane",
  branch: "git-changes-branch",
  head: "git-changes-head",
  dirtyCount: "git-changes-dirty-count",
  error: "git-changes-error",
  stale: "git-changes-stale",
  truncated: "git-changes-truncated",
  refresh: "git-changes-refresh",
  loading: "git-changes-loading",
  empty: "git-changes-empty",
  clean: "git-changes-clean",
  fileList: "git-changes-file-list",
  repoSelector: "git-changes-repo-selector",
  diff: "git-changes-diff",
  diffPath: "git-changes-diff-path",
  diffBinary: "git-changes-diff-binary",
  diffUnsupported: "git-changes-diff-unsupported",
  diffTruncated: "git-changes-diff-truncated",
  diffUnified: "git-changes-diff-unified",
  kind: "git-changes-kind",
});

const FORBIDDEN_SECRET_TOKENS = Object.freeze([
  "approvalGrant",
  "approval_grant",
  "raw_grant",
  "base64",
  "sha256:",
  "proposalId",
  "proposal_id",
  "sessionId",
  "session_id",
  "runId",
  "run_id",
  "\"grant\"",
  "jsonrpc",
  "pillow/process_tool",
  "discover_git_repositories",
  "get_git_status",
  "open_git_diff",
  "bearer",
  "Bearer ",
  "token=",
  "api_key",
  "apiKey",
  "GIT_DIR",
  "GIT_WORK_TREE",
  "argv",
  "executablePath",
]);

const FORBIDDEN_HOST_PATH_TOKENS = Object.freeze([
  "file://",
  "F:\\",
  "C:\\",
  "D:\\",
  "/Users/",
  "/home/",
  "/tmp/",
  "\\Temp\\",
  "\\tmp\\",
  "AppData\\",
  "Local\\Temp",
]);

const UUID_RE =
  /[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}/iu;

const GIT_PRIVACY_TEST_IDS = Object.freeze([
  GIT_TEST_IDS.view,
  GIT_TEST_IDS.pane,
  "resource-pane",
  "ctx-resource-pane",
]);

/**
 * @param {import('playwright-core').Page} page
 */
function assertPage(page) {
  assert.ok(page, "page is required");
}

/**
 * @param {string} value
 * @param {string} label
 */
function assertNonEmptyString(value, label) {
  assert.equal(typeof value, "string", `${label} must be a string`);
  assert.ok(value.trim().length > 0, `${label} must be non-empty`);
}

/**
 * @param {import('playwright-core').Locator} locator
 * @param {number} timeoutMs
 * @param {string} label
 */
async function waitForEnabled(locator, timeoutMs, label) {
  const deadline = performance.now() + timeoutMs;
  while (performance.now() < deadline) {
    if (await locator.isEnabled().catch(() => false)) return;
    await locator.page().waitForTimeout(50);
  }
  throw new Error(`${label} did not become enabled within ${timeoutMs} ms`);
}

/**
 * Privacy gate: no grant/id/path/raw RPC in body text or HTML.
 *
 * @param {import('playwright-core').Page} page
 * @param {string} label
 * @param {string[]} [extraForbidden]
 */
export async function assertGitPrivacyClean(page, label, extraForbidden = []) {
  assertPage(page);
  const text = await page.locator("body").innerText();
  const html = await page.locator("body").innerHTML();

  for (const needle of FORBIDDEN_SECRET_TOKENS) {
    assert.ok(
      !text.includes(needle),
      `${label}: body text must not contain ${JSON.stringify(needle)}`,
    );
    assert.ok(
      !html.includes(needle),
      `${label}: body HTML must not contain ${JSON.stringify(needle)}`,
    );
  }

  for (const needle of FORBIDDEN_HOST_PATH_TOKENS) {
    assert.ok(
      !text.includes(needle),
      `${label}: visible text must not contain host path ${JSON.stringify(needle)}`,
    );
    assert.ok(
      !html.includes(needle),
      `${label}: body HTML must not contain host path ${JSON.stringify(needle)}`,
    );
  }

  for (const needle of extraForbidden) {
    if (!needle) continue;
    assert.ok(
      !text.includes(needle),
      `${label}: body text must not contain fixture token ${JSON.stringify(needle)}`,
    );
    assert.ok(
      !html.includes(needle),
      `${label}: body HTML must not contain fixture token ${JSON.stringify(needle)}`,
    );
  }

  assert.ok(
    !UUID_RE.test(text),
    `${label}: body text must not contain UUID-shaped identifiers`,
  );

  for (const testId of GIT_PRIVACY_TEST_IDS) {
    const region = page.getByTestId(testId);
    const count = await region.count();
    for (let i = 0; i < count; i += 1) {
      const regionHtml = await region.nth(i).innerHTML().catch(() => "");
      const regionText = await region.nth(i).innerText().catch(() => "");
      for (const needle of FORBIDDEN_HOST_PATH_TOKENS) {
        assert.ok(
          !regionText.includes(needle),
          `${label}: ${testId} text must not contain host path ${JSON.stringify(needle)}`,
        );
        assert.ok(
          !regionHtml.includes(needle),
          `${label}: ${testId} HTML must not contain host path ${JSON.stringify(needle)}`,
        );
      }
      for (const needle of FORBIDDEN_SECRET_TOKENS) {
        assert.ok(
          !regionText.includes(needle),
          `${label}: ${testId} text must not contain ${JSON.stringify(needle)}`,
        );
        assert.ok(
          !regionHtml.includes(needle),
          `${label}: ${testId} HTML must not contain ${JSON.stringify(needle)}`,
        );
      }
      assert.ok(
        !UUID_RE.test(regionText),
        `${label}: ${testId} text must not contain UUID-shaped identifiers`,
      );
    }
  }
}

/**
 * Select a project by its public sidebar accessible name.
 *
 * @param {import('playwright-core').Page} page
 * @param {string} projectName
 */
export async function selectProject(page, projectName) {
  assertPage(page);
  assertNonEmptyString(projectName, "projectName");

  const sidebar = page.getByTestId("app-sidebar");
  await sidebar.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });

  const projectButton = sidebar.getByRole("button", {
    name: projectName,
    exact: true,
  });
  await projectButton.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });
  await projectButton.click();

  await page.waitForFunction(
    (name) => {
      const sidebarEl = document.querySelector('[data-testid="app-sidebar"]');
      if (!sidebarEl) return false;
      const buttons = [...sidebarEl.querySelectorAll("button[aria-label]")];
      const match = buttons.find(
        (btn) => btn.getAttribute("aria-label") === name,
      );
      if (!match || match.getAttribute("aria-pressed") !== "true") return false;
      const heading = document.querySelector(
        '[data-testid="active-project-name"]',
      );
      return Boolean(heading?.textContent?.includes(name));
    },
    projectName,
    { timeout: DEFAULT_TIMEOUT_MS },
  );

  return { selectedProjectName: projectName };
}

/**
 * Collapse the contextual resource pane when open.
 *
 * @param {import('playwright-core').Page} page
 */
export async function collapseResourcePaneIfOpen(page) {
  assertPage(page);
  const toggle = page.getByTestId("resource-toggle");
  if ((await toggle.count()) === 0) return { collapsed: true, wasOpen: false };
  if ((await toggle.getAttribute("aria-pressed")) === "true") {
    await toggle.focus();
    const focused = await toggle.evaluate((el) => el === document.activeElement);
    if (focused) {
      await page.keyboard.press("Enter");
    } else {
      await toggle.click({ force: true });
    }
    const deadline = performance.now() + DEFAULT_TIMEOUT_MS;
    while (performance.now() < deadline) {
      const pressed = await toggle.getAttribute("aria-pressed").catch(() => "false");
      const paneVisible =
        (await page.getByTestId("resource-pane").count()) > 0
        && (await page.getByTestId("resource-pane").first().isVisible().catch(() => false));
      if (pressed !== "true" && !paneVisible) {
        return { collapsed: true, wasOpen: true };
      }
      await page.waitForTimeout(50);
    }
    return { collapsed: true, wasOpen: true };
  }
  return { collapsed: true, wasOpen: false };
}

/**
 * Open the contextual resource pane and explicitly select Changes mode.
 * Default body is always Resources (C2); Changes is never an implicit default.
 *
 * @param {import('playwright-core').Page} page
 * @param {number} [timeoutMs]
 */
export async function openGitChangesPane(page, timeoutMs = DEFAULT_TIMEOUT_MS) {
  assertPage(page);
  const toggle = page.getByTestId("resource-toggle");
  await toggle.waitFor({ state: "visible", timeout: timeoutMs });
  assert.equal(
    await toggle.isDisabled(),
    false,
    "resource-toggle must be enabled for an active project",
  );

  if ((await toggle.getAttribute("aria-pressed")) !== "true") {
    // Window chrome can intercept pointer hits on the stage header toggle
    // (same pattern as c2 resource acceptance). Prefer keyboard activation.
    await toggle.focus();
    const focused = await toggle.evaluate((el) => el === document.activeElement);
    if (focused) {
      await page.keyboard.press("Enter");
    } else {
      await toggle.click({ force: true });
    }
  }

  const pane = page.getByTestId("resource-pane");
  await pane.waitFor({ state: "visible", timeout: timeoutMs });

  // Explicit Changes selection — keyboard-reachable compact mode switch.
  const changesTab = page.getByTestId("context-body-mode-changes");
  await changesTab.waitFor({ state: "visible", timeout: timeoutMs });
  if ((await changesTab.getAttribute("aria-selected")) !== "true") {
    await changesTab.focus();
    const tabFocused = await changesTab.evaluate(
      (el) => el === document.activeElement,
    );
    if (tabFocused) {
      await page.keyboard.press("Enter");
    } else {
      await changesTab.click({ force: true });
    }
  }

  // Wait for Changes body (or error/loading) — App may still be discovering.
  const view = page.getByTestId(GIT_TEST_IDS.view);
  const gitPane = page.getByTestId(GIT_TEST_IDS.pane);
  const deadline = performance.now() + timeoutMs;
  while (performance.now() < deadline) {
    if ((await view.count()) > 0 && (await view.isVisible().catch(() => false))) {
      break;
    }
    if (
      (await gitPane.count()) > 0
      && (await gitPane.isVisible().catch(() => false))
    ) {
      break;
    }
    await page.waitForTimeout(50);
  }
  // Prefer settled GitChangesView; accept loading shell in git-changes-pane.
  if ((await view.count()) > 0) {
    await view.waitFor({ state: "visible", timeout: Math.min(timeoutMs, 15_000) });
  } else {
    await gitPane.waitFor({ state: "visible", timeout: Math.min(timeoutMs, 15_000) });
  }

  return { open: true };
}

/**
 * Wait until Git Changes finished loading (no loading marker).
 *
 * @param {import('playwright-core').Page} page
 * @param {number} [timeoutMs]
 */
export async function waitForGitSettled(page, timeoutMs = DEFAULT_TIMEOUT_MS) {
  assertPage(page);
  await page.getByTestId(GIT_TEST_IDS.view).waitFor({
    state: "visible",
    timeout: timeoutMs,
  });
  const deadline = performance.now() + timeoutMs;
  while (performance.now() < deadline) {
    const loading = page.getByTestId(GIT_TEST_IDS.loading);
    const loadingVisible =
      (await loading.count()) > 0
      && (await loading.isVisible().catch(() => false));
    if (!loadingVisible) {
      return { settled: true };
    }
    await page.waitForTimeout(50);
  }
  throw new Error(`git changes still loading after ${timeoutMs} ms`);
}

/**
 * @param {import('playwright-core').Page} page
 * @param {number} [timeoutMs]
 */
export async function waitForGitClean(page, timeoutMs = DEFAULT_TIMEOUT_MS) {
  assertPage(page);
  await waitForGitSettled(page, timeoutMs);
  try {
    await page.getByTestId(GIT_TEST_IDS.clean).waitFor({
      state: "visible",
      timeout: timeoutMs,
    });
  } catch (cause) {
    const viewText = await page
      .getByTestId(GIT_TEST_IDS.view)
      .innerText()
      .catch(() => "");
    const errText = await page
      .getByTestId(GIT_TEST_IDS.error)
      .innerText()
      .catch(() => "");
    throw new Error(
      `git-changes-clean not visible; error=${JSON.stringify(errText)}; `
      + `view=${JSON.stringify(viewText.replace(/\s+/gu, " ").slice(0, 400))}; `
      + `cause=${cause instanceof Error ? cause.message : String(cause)}`,
    );
  }
  const dirty = (await page.getByTestId(GIT_TEST_IDS.dirtyCount).innerText()).trim();
  assert.ok(
    dirty.includes("0 项变更"),
    `expected clean dirty count, got ${JSON.stringify(dirty)}`,
  );
  return { clean: true, dirtyCountText: dirty };
}

/**
 * @param {import('playwright-core').Page} page
 * @param {{ minDirty?: number, timeoutMs?: number }} [options]
 */
export async function waitForGitDirty(page, options = {}) {
  assertPage(page);
  const minDirty = options.minDirty ?? 1;
  const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  await waitForGitSettled(page, timeoutMs);
  const deadline = performance.now() + timeoutMs;
  let last = "";
  while (performance.now() < deadline) {
    const dirtyEl = page.getByTestId(GIT_TEST_IDS.dirtyCount);
    if ((await dirtyEl.count()) > 0) {
      last = (await dirtyEl.innerText().catch(() => "")).trim();
      const match = last.match(/(\d+)\s*项变更/u);
      if (match && Number(match[1]) >= minDirty) {
        return { dirty: true, dirtyCountText: last, dirtyCount: Number(match[1]) };
      }
    }
    await page.waitForTimeout(50);
  }
  const viewText = await page
    .getByTestId(GIT_TEST_IDS.view)
    .innerText()
    .catch(() => "");
  const errText = await page
    .getByTestId(GIT_TEST_IDS.error)
    .innerText()
    .catch(() => "");
  throw new Error(
    `expected dirtyCount >= ${minDirty}; last=${JSON.stringify(last)}; `
    + `error=${JSON.stringify(errText)}; `
    + `view=${JSON.stringify(viewText.replace(/\s+/gu, " ").slice(0, 500))}`,
  );
}

/**
 * @param {import('playwright-core').Page} page
 * @param {string} kindLabel Chinese kind label
 * @param {string} portablePath
 * @param {{ originalPath?: string, timeoutMs?: number }} [options]
 */
export async function assertGitEntryVisible(
  page,
  kindLabel,
  portablePath,
  options = {},
) {
  assertPage(page);
  assertNonEmptyString(kindLabel, "kindLabel");
  assertNonEmptyString(portablePath, "portablePath");
  const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  await waitForGitSettled(page, timeoutMs);

  const list = page.getByTestId(GIT_TEST_IDS.fileList);
  await list.waitFor({ state: "visible", timeout: timeoutMs });
  const entries = list.locator("[data-testid^='git-changes-entry-']");
  const count = await entries.count();
  let matched = false;
  let matchedIndex = -1;
  for (let i = 0; i < count; i += 1) {
    const text = (await entries.nth(i).innerText().catch(() => "")).trim();
    if (!text.includes(kindLabel) || !text.includes(portablePath)) continue;
    if (options.originalPath && !text.includes(options.originalPath)) continue;
    matched = true;
    matchedIndex = i;
    break;
  }
  assert.ok(
    matched,
    `expected entry kind=${kindLabel} path=${portablePath}`
      + (options.originalPath ? ` original=${options.originalPath}` : "")
      + `; entryCount=${count}`,
  );
  return { matched: true, index: matchedIndex };
}

/**
 * @param {import('playwright-core').Page} page
 * @param {string} portablePath
 * @param {number} [timeoutMs]
 */
export async function selectGitPath(page, portablePath, timeoutMs = DEFAULT_TIMEOUT_MS) {
  assertPage(page);
  assertNonEmptyString(portablePath, "portablePath");
  await waitForGitSettled(page, timeoutMs);

  const list = page.getByTestId(GIT_TEST_IDS.fileList);
  await list.waitFor({ state: "visible", timeout: timeoutMs });
  const entries = list.locator("button[data-testid^='git-changes-entry-']");
  const count = await entries.count();
  for (let i = 0; i < count; i += 1) {
    const entry = entries.nth(i);
    const text = (await entry.innerText().catch(() => "")).trim();
    if (!text.includes(portablePath)) continue;
    await entry.click();
    await page.getByTestId(GIT_TEST_IDS.diff).waitFor({
      state: "visible",
      timeout: timeoutMs,
    });
    const pathText = (
      await page.getByTestId(GIT_TEST_IDS.diffPath).innerText()
    ).trim();
    assert.ok(
      pathText.includes(portablePath),
      `diff path must include ${portablePath}, got ${JSON.stringify(pathText)}`,
    );
    return { path: portablePath, opened: true };
  }
  throw new Error(`no selectable git entry for path ${JSON.stringify(portablePath)}`);
}

/**
 * Select a nested/root repository by portable id (`.` → root test id).
 *
 * @param {import('playwright-core').Page} page
 * @param {string} repositoryId
 * @param {number} [timeoutMs]
 */
export async function selectGitRepository(
  page,
  repositoryId,
  timeoutMs = DEFAULT_TIMEOUT_MS,
) {
  assertPage(page);
  assertNonEmptyString(repositoryId, "repositoryId");
  await waitForGitSettled(page, timeoutMs);

  const testId =
    repositoryId === "."
      ? "git-changes-repo-root"
      : `git-changes-repo-${repositoryId}`;
  const button = page.getByTestId(testId);
  await button.waitFor({ state: "visible", timeout: timeoutMs });
  assert.equal(
    await button.isDisabled(),
    false,
    `repository ${repositoryId} must be available`,
  );
  await button.click();
  await waitForGitSettled(page, timeoutMs);
  const pressed = await button.getAttribute("aria-pressed");
  assert.equal(pressed, "true", `repository ${repositoryId} must be selected`);
  return { repositoryId, selected: true };
}

/**
 * @param {import('playwright-core').Page} page
 * @param {number} [timeoutMs]
 */
export async function refreshGitChanges(page, timeoutMs = DEFAULT_TIMEOUT_MS) {
  assertPage(page);
  const refresh = page.getByTestId(GIT_TEST_IDS.refresh);
  await refresh.waitFor({ state: "visible", timeout: timeoutMs });
  await waitForEnabled(refresh, timeoutMs, GIT_TEST_IDS.refresh);
  await refresh.click();
  await waitForGitSettled(page, timeoutMs);
  return { refreshed: true };
}

/**
 * @param {import('playwright-core').Page} page
 * @param {string} expectedMessage
 * @param {number} [timeoutMs]
 */
export async function waitForGitError(
  page,
  expectedMessage,
  timeoutMs = DEFAULT_TIMEOUT_MS,
) {
  assertPage(page);
  assertNonEmptyString(expectedMessage, "expectedMessage");
  const error = page.getByTestId(GIT_TEST_IDS.error);
  await error.waitFor({ state: "visible", timeout: timeoutMs });
  const text = (await error.innerText()).trim();
  assert.equal(
    text,
    expectedMessage,
    `git error must be exact path-free public copy`,
  );
  return { error: text };
}

/**
 * @param {import('playwright-core').Page} page
 * @param {number} [timeoutMs]
 */
export async function waitForComposerIdle(page, timeoutMs = DEFAULT_TIMEOUT_MS) {
  assertPage(page);
  const deadline = performance.now() + timeoutMs;
  const composer = page.getByRole("textbox", { name: "工作请求" });
  const newChat = page.getByRole("button", { name: "新建对话" });
  while (performance.now() < deadline) {
    const cancelVisible =
      (await page.getByTestId("composer-cancel").count()) > 0
      && (await page.getByTestId("composer-cancel").isVisible().catch(() => false));
    const inputEnabled = await composer.isEnabled().catch(() => false);
    const newChatEnabled = await newChat.isEnabled().catch(() => false);
    if (!cancelVisible && inputEnabled && newChatEnabled) {
      return { idle: true };
    }
    await page.waitForTimeout(50);
  }
  throw new Error("composer did not become idle");
}

/**
 * Seed a short non-tool conversation so the stage is not the empty placeholder.
 *
 * @param {import('playwright-core').Page} page
 * @param {string} [prompt]
 */
export async function seedConversation(
  page,
  prompt = "git-acceptance seed conversation",
) {
  assertPage(page);
  await waitForComposerIdle(page);
  const newChat = page.getByRole("button", { name: "新建对话" });
  await newChat.click();
  await waitForComposerIdle(page);
  const composer = page.getByRole("textbox", { name: "工作请求" });
  await composer.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });
  await composer.fill(prompt);
  const send = page.getByTestId("composer-send");
  await waitForEnabled(send, DEFAULT_TIMEOUT_MS, "composer-send");
  await send.click();
  await page
    .getByTestId("timeline-user-message")
    .filter({ hasText: prompt })
    .waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });
  await waitForComposerIdle(page, DEFAULT_TIMEOUT_MS);
  return { seeded: true, prompt };
}

/**
 * Conversation surface dominates; Changes is contextual, not a second dashboard.
 *
 * @param {import('playwright-core').Page} page
 */
export async function assertConversationDominantWithChanges(page) {
  assertPage(page);
  const workbench = page.getByTestId("workbench");
  await workbench.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });
  const surface = page
    .locator(".conversation-surface, [aria-label='对话工作台']")
    .first();
  await surface.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });

  const stack = page.locator(
    ".approval-card-stack, [data-testid='approval-card-stack']",
  );
  assert.equal(await stack.count(), 0, "no permanent approval card stack");

  const emptyHeadingVisible = await page
    .getByRole("heading", { name: "开始新对话" })
    .isVisible()
    .catch(() => false);
  const userMessages = await page.getByTestId("timeline-user-message").count();
  assert.ok(
    !emptyHeadingVisible || userMessages > 0,
    "conversation stage must not be empty placeholder for visual proof",
  );

  const surfaceBox = await surface.boundingBox().catch(() => null);
  const pane = page.getByTestId("resource-pane");
  if (
    surfaceBox
    && (await pane.count()) > 0
    && (await pane.first().isVisible().catch(() => false))
  ) {
    const paneBox = await pane.first().boundingBox().catch(() => null);
    if (paneBox && paneBox.width > 0) {
      assert.ok(
        surfaceBox.width >= paneBox.width,
        `conversation surface must dominate contextual pane width `
          + `(surface=${surfaceBox.width}, pane=${paneBox.width})`,
      );
    }
  }

  const changes = page.getByTestId(GIT_TEST_IDS.view);
  if ((await changes.count()) > 0) {
    assert.equal(
      await changes.first().isVisible().catch(() => false),
      true,
      "Changes view should be visible for git visual proof",
    );
  }

  return { conversationDominant: true };
}

/**
 * @param {{
 *   page: import('playwright-core').Page,
 *   path: string,
 *   label: string,
 *   extraForbidden?: string[],
 * }} args
 */
export async function capturePrivacySafeScreenshot({
  page,
  path: filePath,
  label,
  extraForbidden = [],
}) {
  assertPage(page);
  assertNonEmptyString(filePath, "path");
  assertNonEmptyString(label, "label");
  await assertGitPrivacyClean(page, `screenshot:${label}`, extraForbidden);
  await page.screenshot({ path: filePath, fullPage: false });
  return { screenshot: filePath, label };
}

/**
 * Assert body/Changes surfaces do not contain foreign project markers.
 *
 * @param {import('playwright-core').Page} page
 * @param {string[]} foreignNeedles
 * @param {string} label
 */
export async function assertNoForeignGitEvidence(page, foreignNeedles, label) {
  assertPage(page);
  const text = await page.locator("body").innerText();
  const html = await page.locator("body").innerHTML();
  for (const needle of foreignNeedles) {
    if (!needle) continue;
    assert.ok(
      !text.includes(needle),
      `${label}: body text must not include foreign ${JSON.stringify(needle)}`,
    );
    assert.ok(
      !html.includes(needle),
      `${label}: body HTML must not include foreign ${JSON.stringify(needle)}`,
    );
  }
  return { foreignAbsent: true };
}
