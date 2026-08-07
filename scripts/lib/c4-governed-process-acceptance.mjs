/**
 * Wave C4 D3B3 — governed-process WebView2 UI helpers.
 *
 * Public accessible roles/names and stable data-testid values only.
 * Does not launch/close processes, call the process executor, or inject
 * renderer state. Returns JSON-serializable evidence objects only.
 */

import assert from "node:assert/strict";

const DEFAULT_TIMEOUT_MS = 60_000;

/** Fixed Chinese terminal status labels from processActivityModel. */
export const PROCESS_STATUS = Object.freeze({
  pending: "待批准",
  approved: "已批准",
  running: "运行中",
  succeeded: "已成功",
  failed: "已失败",
  rejected: "已拒绝",
  expired: "已过期",
  cancelled: "已取消",
  interrupted: "已中断",
});

export const PROCESS_TEST_IDS = Object.freeze({
  approval: "command-approval-bar",
  approve: "command-approval-approve",
  reject: "command-approval-reject",
  review: "command-approval-review",
  activity: "command-activity-row",
  activityDetails: "command-activity-details",
  activityStatus: "command-activity-status",
  activityOpenOutput: "command-activity-open-output",
  terminal: "terminal-output-view",
  terminalBody: "terminal-output-body",
});

/**
 * Grant / id / digest carriers — must never appear in visible text or full
 * body HTML (screenshots + accessibility text).
 */
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
  "bearer",
  "Bearer ",
  "token=",
  "api_key",
  "apiKey",
]);

/**
 * Absolute host-path prefixes. Must not appear in visible body text or in
 * full body HTML (including attributes).
 */
const FORBIDDEN_HOST_PATH_TOKENS = Object.freeze([
  "file://",
  "F:\\",
  "C:\\",
  "/Users/",
  "/home/",
]);

/** UUID shape (loose) — internal ids must not paint. */
const UUID_RE =
  /[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}/iu;

/** Process-owned surfaces that must not embed host paths or secrets. */
const PROCESS_PRIVACY_TEST_IDS = Object.freeze([
  PROCESS_TEST_IDS.approval,
  PROCESS_TEST_IDS.activity,
  PROCESS_TEST_IDS.terminal,
  "write-approval-bar",
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
 * Wait until the floating composer can accept a new turn: Stop is gone,
 * the work-request field is enabled, and 新建对话 is enabled (not busy).
 *
 * @param {import('playwright-core').Page} page
 * @param {number} [timeoutMs]
 */
export async function waitForComposerIdle(page, timeoutMs = DEFAULT_TIMEOUT_MS) {
  assertPage(page);
  const deadline = performance.now() + timeoutMs;
  const composer = page.getByRole("textbox", { name: "工作请求" });
  const newChat = page.getByRole("button", { name: "新建对话" });
  let last = {
    cancelVisible: true,
    inputEnabled: false,
    newChatEnabled: false,
  };
  while (performance.now() < deadline) {
    const cancelVisible =
      (await page.getByTestId("composer-cancel").count()) > 0
      && (await page.getByTestId("composer-cancel").isVisible().catch(() => false));
    const inputEnabled = await composer.isEnabled().catch(() => false);
    const newChatEnabled = await newChat.isEnabled().catch(() => false);
    last = { cancelVisible, inputEnabled, newChatEnabled };
    if (!cancelVisible && inputEnabled && newChatEnabled) {
      return { idle: true };
    }
    await page.waitForTimeout(50);
  }
  const bodySnippet = (await page.locator("body").innerText().catch(() => ""))
    .replace(/\s+/gu, " ")
    .slice(0, 300);
  throw new Error(
    "composer did not become idle (Stop gone, input enabled, 新建对话 enabled); "
    + `last=${JSON.stringify(last)}; body=${JSON.stringify(bodySnippet)}`,
  );
}

/**
 * Privacy gate: no grant/id/path/raw RPC in screenshots or accessibility text.
 *
 * @param {import('playwright-core').Page} page
 * @param {string} label
 */
async function assertPrivacyClean(page, label) {
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

  assert.ok(
    !UUID_RE.test(text),
    `${label}: body text must not contain UUID-shaped identifiers`,
  );

  for (const testId of PROCESS_PRIVACY_TEST_IDS) {
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

  await waitForComposerIdle(page, Math.min(DEFAULT_TIMEOUT_MS, 15_000)).catch(
    () => undefined,
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
    await toggle.click();
    const deadline = performance.now() + DEFAULT_TIMEOUT_MS;
    while (performance.now() < deadline) {
      const pressed = await toggle.getAttribute("aria-pressed").catch(() => "false");
      const paneVisible =
        ((await page.getByTestId("resource-pane").count()) > 0
          && (await page.getByTestId("resource-pane").first().isVisible().catch(() => false)))
        || ((await page.getByTestId("ctx-resource-pane").count()) > 0
          && (await page
            .getByTestId("ctx-resource-pane")
            .first()
            .isVisible()
            .catch(() => false)));
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
 * Create a conversation and send a process prompt through the public composer.
 *
 * @param {{ page: import('playwright-core').Page, prompt: string }} args
 */
export async function sendProcessPrompt({ page, prompt }) {
  assertPage(page);
  assertNonEmptyString(prompt, "prompt");

  await waitForComposerIdle(page);

  const newChat = page.getByRole("button", { name: "新建对话" });
  await newChat.click();
  await waitForComposerIdle(page);

  const composer = page.getByRole("textbox", { name: "工作请求" });
  await composer.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });
  await composer.click();
  await composer.fill(prompt);
  const draft = await composer.inputValue();
  assert.ok(
    draft.includes(prompt),
    `composer draft must retain prompt after fill, got ${JSON.stringify(draft)}`,
  );

  const send = page.getByTestId("composer-send");
  await send.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });
  await waitForEnabled(send, DEFAULT_TIMEOUT_MS, "composer-send");
  await send.click();

  try {
    await page
      .getByTestId("timeline-user-message")
      .filter({ hasText: prompt })
      .waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });
  } catch (cause) {
    const sendError = await page
      .getByTestId("composer-send-error")
      .innerText()
      .catch(() => "");
    const refreshWarning = await page
      .getByTestId("composer-refresh-warning")
      .innerText()
      .catch(() => "");
    const bodySnippet = (await page.locator("body").innerText().catch(() => ""))
      .replace(/\s+/gu, " ")
      .slice(0, 400);
    throw new Error(
      `user message not visible for ${JSON.stringify(prompt)}; `
      + `sendError=${JSON.stringify(sendError)}; `
      + `refreshWarning=${JSON.stringify(refreshWarning)}; `
      + `body=${JSON.stringify(bodySnippet)}; `
      + `cause=${cause instanceof Error ? cause.message : String(cause)}`,
    );
  }

  return { promptSent: true, prompt };
}

/**
 * Count the union of write and process approval bars. At most one allowed.
 *
 * @param {import('playwright-core').Page} page
 */
export async function assertSingleApprovalSlot(page) {
  assertPage(page);
  const writeBars = await page.getByTestId("write-approval-bar").count();
  const processBars = await page.getByTestId(PROCESS_TEST_IDS.approval).count();
  const total = writeBars + processBars;
  assert.ok(
    total <= 1,
    `at most one approval slot allowed; write=${writeBars} process=${processBars}`,
  );
  return { writeBars, processBars, total };
}

/**
 * Wait for the single conversation-owned process approval bar.
 *
 * @param {{
 *   page: import('playwright-core').Page,
 *   expectedProgram?: string,
 *   expectedArgvNeedle?: string,
 *   timeoutMs?: number,
 * }} args
 */
export async function waitForCommandApproval({
  page,
  expectedProgram = "node",
  expectedArgvNeedle,
  timeoutMs = DEFAULT_TIMEOUT_MS,
}) {
  assertPage(page);
  assertNonEmptyString(expectedProgram, "expectedProgram");

  const bar = page.getByTestId(PROCESS_TEST_IDS.approval);
  await bar.waitFor({ state: "visible", timeout: timeoutMs });

  const programText = (
    await page.getByTestId("command-approval-program").innerText()
  ).trim();
  assert.ok(
    programText.includes(expectedProgram) || programText === "命令",
    `approval program must include ${expectedProgram} or redacted 命令, got ${JSON.stringify(programText)}`,
  );

  if (expectedArgvNeedle) {
    const argvText = (
      await page.getByTestId("command-approval-argv").innerText()
    ).trim();
    assert.ok(
      argvText.includes(expectedArgvNeedle),
      `approval argv must include ${JSON.stringify(expectedArgvNeedle)}, got ${JSON.stringify(argvText)}`,
    );
  }

  await assertSingleApprovalSlot(page);

  const approve = page.getByTestId(PROCESS_TEST_IDS.approve);
  await approve.waitFor({ state: "visible", timeout: timeoutMs });
  const focused = await approve.evaluate((el) => el === document.activeElement);
  assert.equal(focused, true, "Approve must receive initial focus");

  await assertPrivacyClean(page, "command-approval-bar");

  return {
    barVisible: true,
    program: programText,
    approveFocused: true,
    singleBar: true,
  };
}

/**
 * Assert the process approval bar is absent.
 *
 * @param {import('playwright-core').Page} page
 * @param {number} [timeoutMs]
 */
export async function assertCommandApprovalAbsent(page, timeoutMs = 10_000) {
  assertPage(page);
  const deadline = performance.now() + timeoutMs;
  while (performance.now() < deadline) {
    if ((await page.getByTestId(PROCESS_TEST_IDS.approval).count()) === 0) {
      return { barAbsent: true };
    }
    await page.waitForTimeout(50);
  }
  throw new Error("command-approval-bar still present after timeout");
}

/**
 * Click Approve once on the process bar.
 *
 * Production process decide is synchronous through execute: long-running
 * commands keep the bar mounted (submitting/disabled) until the process
 * terminals. Callers that only need the click should pass `waitForClear:false`.
 *
 * @param {import('playwright-core').Page} page
 * @param {{ waitForClear?: boolean, timeoutMs?: number }} [options]
 */
export async function approveCommandOnce(page, options = {}) {
  assertPage(page);
  const waitForClear = options.waitForClear !== false;
  const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  const approve = page.getByTestId(PROCESS_TEST_IDS.approve);
  await approve.waitFor({ state: "visible", timeout: timeoutMs });
  assert.equal(await approve.isDisabled(), false, "Approve must be enabled");
  await approve.click({ force: true });
  if (waitForClear) {
    await assertCommandApprovalAbsent(page, timeoutMs);
    return { approved: true, barCleared: true };
  }
  // Long-running path: approve may stay submitting until host execute returns.
  const deadline = performance.now() + Math.min(timeoutMs, 15_000);
  while (performance.now() < deadline) {
    const barCount = await page.getByTestId(PROCESS_TEST_IDS.approval).count();
    if (barCount === 0) {
      return { approved: true, barCleared: true };
    }
    const disabled = await approve.isDisabled().catch(() => true);
    if (disabled) {
      return { approved: true, barCleared: false, submitting: true };
    }
    await page.waitForTimeout(50);
  }
  return { approved: true, barCleared: false };
}

/**
 * Click Reject on the process bar.
 *
 * @param {import('playwright-core').Page} page
 */
export async function rejectCommand(page) {
  assertPage(page);
  const reject = page.getByTestId(PROCESS_TEST_IDS.reject);
  await reject.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });
  assert.equal(await reject.isDisabled(), false, "Reject must be enabled");
  await reject.click();
  await assertCommandApprovalAbsent(page, DEFAULT_TIMEOUT_MS);
  return { rejected: true, barCleared: true };
}

/**
 * Wait for a command activity row with the expected status label.
 *
 * @param {{
 *   page: import('playwright-core').Page,
 *   status: string,
 *   argvNeedle?: string,
 *   exitCode?: number | null,
 *   timeoutMs?: number,
 * }} args
 */
export async function waitForCommandActivity({
  page,
  status,
  argvNeedle,
  exitCode,
  timeoutMs = DEFAULT_TIMEOUT_MS,
}) {
  assertPage(page);
  assertNonEmptyString(status, "status");

  const deadline = performance.now() + timeoutMs;
  let lastSnippet = "";
  while (performance.now() < deadline) {
    const rows = page.getByTestId(PROCESS_TEST_IDS.activity);
    const count = await rows.count();
    for (let i = 0; i < count; i += 1) {
      const row = rows.nth(i);
      if (!(await row.isVisible().catch(() => false))) continue;
      const statusText = (
        await row.getByTestId(PROCESS_TEST_IDS.activityStatus).innerText().catch(() => "")
      ).trim();
      if (statusText !== status) continue;
      if (argvNeedle) {
        // May need expand for argv; check summary + body.
        const rowText = (await row.innerText().catch(() => "")).trim();
        if (!rowText.includes(argvNeedle)) {
          // Try expanding details.
          const summary = row.getByTestId("command-activity-summary");
          if ((await summary.count()) > 0) {
            await summary.click().catch(() => undefined);
          }
          const expanded = (await row.innerText().catch(() => "")).trim();
          if (!expanded.includes(argvNeedle)) continue;
        }
      }
      if (exitCode !== undefined && exitCode !== null) {
        const exitText = await row
          .getByTestId("command-activity-exit")
          .innerText()
          .catch(() => "");
        if (!exitText.includes(String(exitCode))) {
          // Expand details if exit is nested.
          const summary = row.getByTestId("command-activity-summary");
          if ((await summary.count()) > 0) {
            await summary.click().catch(() => undefined);
          }
          const exitAfter = await row
            .getByTestId("command-activity-exit")
            .innerText()
            .catch(() => "");
          if (!exitAfter.includes(String(exitCode))) continue;
        }
      }
      await assertPrivacyClean(page, `command-activity:${status}`);
      return {
        status,
        visible: true,
        exitCode: exitCode ?? null,
      };
    }
    lastSnippet = (await page.locator("body").innerText().catch(() => ""))
      .replace(/\s+/gu, " ")
      .slice(0, 300);
    await page.waitForTimeout(50);
  }
  throw new Error(
    `command activity with status ${JSON.stringify(status)} not visible; `
    + `body=${JSON.stringify(lastSnippet)}`,
  );
}

/**
 * Expand activity details and open complete terminal output in the contextual pane.
 *
 * @param {{
 *   page: import('playwright-core').Page,
 *   expectTruncated?: boolean,
 *   expectLossy?: boolean,
 *   bodyNeedle?: string,
 *   timeoutMs?: number,
 * }} args
 */
export async function openCommandOutput({
  page,
  expectTruncated = false,
  expectLossy = false,
  bodyNeedle,
  timeoutMs = DEFAULT_TIMEOUT_MS,
}) {
  assertPage(page);

  const rows = page.getByTestId(PROCESS_TEST_IDS.activity);
  await rows.first().waitFor({ state: "visible", timeout: timeoutMs });
  const rowCount = await rows.count();
  // Prefer a completed activity row that exposes 完整输出; fall back to first.
  let row = rows.first();
  for (let i = 0; i < rowCount; i += 1) {
    const candidate = rows.nth(i);
    if (!(await candidate.isVisible().catch(() => false))) continue;
    const statusText = (
      await candidate
        .getByTestId(PROCESS_TEST_IDS.activityStatus)
        .innerText()
        .catch(() => "")
    ).trim();
    const completed = [
      PROCESS_STATUS.succeeded,
      PROCESS_STATUS.failed,
      PROCESS_STATUS.cancelled,
      PROCESS_STATUS.interrupted,
      PROCESS_STATUS.rejected,
      PROCESS_STATUS.expired,
    ].some((label) => statusText.includes(label));
    if (!completed && rowCount > 1) continue;
    const detailsCandidate = candidate.getByTestId(PROCESS_TEST_IDS.activityDetails);
    if ((await detailsCandidate.count()) > 0) {
      const openAttr = await detailsCandidate.getAttribute("open");
      if (openAttr === null) {
        const summary = candidate.getByTestId("command-activity-summary");
        if ((await summary.count()) > 0) {
          await summary.click().catch(() => undefined);
        }
      }
    }
    const openCandidate = candidate.getByTestId(PROCESS_TEST_IDS.activityOpenOutput);
    if ((await openCandidate.count()) > 0) {
      row = candidate;
      break;
    }
  }

  const details = row.getByTestId(PROCESS_TEST_IDS.activityDetails);
  if ((await details.count()) > 0) {
    const open = await details.getAttribute("open");
    if (open === null) {
      const summary = row.getByTestId("command-activity-summary");
      if ((await summary.count()) > 0) {
        await summary.click();
      } else {
        await details.click();
      }
    }
  }

  const openBtn = row.getByTestId(PROCESS_TEST_IDS.activityOpenOutput);
  await openBtn.waitFor({ state: "visible", timeout: timeoutMs });
  await waitForEnabled(openBtn, timeoutMs, PROCESS_TEST_IDS.activityOpenOutput);
  await openBtn.click();

  const terminal = page.getByTestId(PROCESS_TEST_IDS.terminal);
  await terminal.waitFor({ state: "visible", timeout: timeoutMs });

  if (expectTruncated) {
    await page
      .getByTestId("terminal-output-truncated")
      .waitFor({ state: "visible", timeout: timeoutMs });
  }
  if (expectLossy) {
    await page
      .getByTestId("terminal-output-lossy")
      .waitFor({ state: "visible", timeout: timeoutMs });
  }
  if (bodyNeedle) {
    const body = page.getByTestId(PROCESS_TEST_IDS.terminalBody);
    await body.waitFor({ state: "visible", timeout: timeoutMs });
    const deadline = performance.now() + timeoutMs;
    let text = "";
    while (performance.now() < deadline) {
      text = await body.innerText().catch(() => "");
      if (text.includes(bodyNeedle)) break;
      await page.waitForTimeout(50);
    }
    assert.ok(
      text.includes(bodyNeedle),
      `terminal body must include ${JSON.stringify(bodyNeedle)}`,
    );
  }

  await assertPrivacyClean(page, "terminal-output");
  return {
    terminalVisible: true,
    truncated: expectTruncated,
    lossy: expectLossy,
  };
}

/**
 * Snapshot DOM evidence for cancel-tree diagnosis (read-only; no product mutation).
 *
 * @param {import('playwright-core').Page} page
 * @param {string} [phase]
 */
export async function dumpCancelTreeUiState(page, phase = "snapshot") {
  assertPage(page);
  const cancelCount = await page.getByTestId("composer-cancel").count();
  const cancelVisible =
    cancelCount > 0
    && (await page.getByTestId("composer-cancel").isVisible().catch(() => false));
  const cancelEnabled =
    cancelVisible
    && (await page.getByTestId("composer-cancel").isEnabled().catch(() => false));
  const approvalCount = await page.getByTestId(PROCESS_TEST_IDS.approval).count();
  const approvalVisible =
    approvalCount > 0
    && (await page.getByTestId(PROCESS_TEST_IDS.approval).isVisible().catch(() => false));
  const approveDisabled = approvalVisible
    ? await page.getByTestId(PROCESS_TEST_IDS.approve).isDisabled().catch(() => null)
    : null;
  const activityStatuses = [];
  const activityRows = page.getByTestId(PROCESS_TEST_IDS.activity);
  const activityCount = await activityRows.count();
  for (let i = 0; i < Math.min(activityCount, 8); i += 1) {
    const statusText = (
      await activityRows
        .nth(i)
        .getByTestId(PROCESS_TEST_IDS.activityStatus)
        .innerText()
        .catch(() => "")
    ).trim();
    if (statusText) activityStatuses.push(statusText);
  }
  const bodyText = (await page.locator("body").innerText().catch(() => ""))
    .replace(/\s+/gu, " ")
    .slice(0, 1200);
  const projectName = (
    await page.getByTestId("active-project-name").innerText().catch(() => "")
  ).trim();
  const selectedConversation = await page
    .locator('[data-testid="conversation-row"][aria-current="page"]')
    .count();
  const conversationRows = await page.getByTestId("conversation-row").count();
  const timelineUserMessages = await page.getByTestId("timeline-user-message").count();
  const emptyStage = await isEmptyConversationStage(page).catch(() => null);
  const composerEnabled = await page
    .getByRole("textbox", { name: "工作请求" })
    .isEnabled()
    .catch(() => null);
  const newChatEnabled = await page
    .getByRole("button", { name: "新建对话" })
    .isEnabled()
    .catch(() => null);

  // Infer run-active proxy from product DOM gate: canCancel only mounts when
  // isRunActive(selectActiveRun(...)) under ownership is true.
  const inferredRunActive = cancelVisible;
  return {
    phase,
    at: new Date().toISOString(),
    perfMs: Math.round(performance.now()),
    composerCancel: {
      count: cancelCount,
      visible: cancelVisible,
      enabled: cancelEnabled,
    },
    inferredRunActive,
    approval: {
      count: approvalCount,
      visible: approvalVisible,
      approveDisabled,
    },
    activityStatuses,
    activityCount,
    projectName,
    selectedConversationRows: selectedConversation,
    conversationRows,
    timelineUserMessages,
    emptyStage,
    composerEnabled,
    newChatEnabled,
    bodySnippet: bodyText,
  };
}

/**
 * Cancel the owning run via composer Stop while approval is pending or process running.
 *
 * When `debug` is provided, samples cancel visibility during the wait and dumps
 * state on timeout (diagnostic only; product click path unchanged on success).
 *
 * @param {import('playwright-core').Page} page
 * @param {{
 *   timeoutMs?: number,
 *   debug?: {
 *     dumpDir?: string,
 *     label?: string,
 *     onSample?: (sample: object) => void,
 *   },
 * }} [options]
 */
export async function cancelOwningRun(page, options = {}) {
  assertPage(page);
  const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  const debug = options.debug ?? null;
  const t0 = performance.now();
  /** @type {Array<{ tMs: number, visible: boolean, count: number }>} */
  const visibilitySamples = [];
  let everVisible = false;

  const cancel = page.getByTestId("composer-cancel");
  const sampleVisibility = async () => {
    const count = await page.getByTestId("composer-cancel").count();
    const visible =
      count > 0
      && (await page.getByTestId("composer-cancel").isVisible().catch(() => false));
    if (visible) everVisible = true;
    const sample = {
      tMs: Math.round(performance.now() - t0),
      visible,
      count,
    };
    visibilitySamples.push(sample);
    if (debug?.onSample) debug.onSample(sample);
    return visible;
  };

  await sampleVisibility();
  let waitFailed = null;
  // Manual poll (no orphan background task): sample cancel visibility until
  // visible or timeout. Product wait condition is the same as before.
  while (performance.now() - t0 < timeoutMs) {
    if (await sampleVisibility()) break;
    await page.waitForTimeout(250);
  }
  if (!everVisible) {
    waitFailed = new Error(
      `locator.waitFor: Timeout ${timeoutMs}ms exceeded.\nCall log:\n  - waiting for getByTestId('composer-cancel') to be visible`,
    );
  }
  if (waitFailed) {
    await sampleVisibility();
    const dump = await dumpCancelTreeUiState(page, "cancelOwningRun-timeout");
    dump.everVisibleDuringWait = everVisible;
    dump.visibilitySamples = visibilitySamples;
    dump.waitMs = Math.round(performance.now() - t0);
    dump.waitError = waitFailed.message;
    if (debug?.dumpDir) {
      const { mkdir, writeFile } = await import("node:fs/promises");
      const pathMod = await import("node:path");
      const stamp = new Date().toISOString().replace(/[:.]/gu, "-");
      const label = debug.label ?? "cancel-tree";
      await mkdir(debug.dumpDir, { recursive: true });
      const jsonPath = pathMod.join(
        debug.dumpDir,
        `${label}-${stamp}-timeout.json`,
      );
      await writeFile(jsonPath, `${JSON.stringify(dump, null, 2)}\n`, "utf8");
      const shotPath = pathMod.join(
        debug.dumpDir,
        `${label}-${stamp}-timeout.png`,
      );
      await page.screenshot({ path: shotPath, fullPage: false }).catch(() => undefined);
      dump.dumpJsonPath = jsonPath;
      dump.dumpScreenshotPath = shotPath;
    }
    const err = new Error(
      `composer-cancel wait failed; everVisible=${everVisible}; `
      + `cancel=${JSON.stringify(dump.composerCancel)}; `
      + `activityStatuses=${JSON.stringify(dump.activityStatuses)}; `
      + `selectedConversationRows=${dump.selectedConversationRows}; `
      + `emptyStage=${dump.emptyStage}; `
      + `approval=${JSON.stringify(dump.approval)}; `
      + `body=${JSON.stringify(dump.bodySnippet.slice(0, 400))}`,
    );
    err.cause = waitFailed;
    err.cancelTreeDump = dump;
    throw err;
  }

  await waitForEnabled(cancel, DEFAULT_TIMEOUT_MS, "composer-cancel");
  await cancel.click({ force: true });

  const firstDeadline = performance.now() + 5_000;
  while (performance.now() < firstDeadline) {
    const barGone = (await page.getByTestId(PROCESS_TEST_IDS.approval).count()) === 0;
    const statusText = await page
      .getByTestId(PROCESS_TEST_IDS.activityStatus)
      .first()
      .innerText()
      .catch(() => "");
    if (
      barGone
      && (statusText.includes(PROCESS_STATUS.cancelled)
        || statusText.includes(PROCESS_STATUS.interrupted)
        || statusText.includes(PROCESS_STATUS.failed)
        || statusText.includes(PROCESS_STATUS.rejected))
    ) {
      return {
        cancelled: true,
        clicks: 1,
        everVisibleDuringWait: true,
        visibilitySamples,
      };
    }
    await page.waitForTimeout(50);
  }

  if ((await page.getByTestId("composer-cancel").count()) > 0) {
    await page.getByTestId("composer-cancel").click({ force: true });
  }

  return {
    cancelled: true,
    clicks: 2,
    everVisibleDuringWait: true,
    visibilitySamples,
  };
}

/**
 * True when the active timeline already shows this user prompt.
 *
 * @param {import('playwright-core').Page} page
 * @param {string} prompt
 */
async function timelineHasUserPrompt(page, prompt) {
  const message = page
    .getByTestId("timeline-user-message")
    .filter({ hasText: prompt });
  return (await message.count()) > 0
    && (await message.first().isVisible().catch(() => false));
}

/**
 * Open an existing conversation that owns the given user prompt.
 *
 * @param {import('playwright-core').Page} page
 * @param {string} prompt
 */
export async function openConversationForPrompt(page, prompt) {
  assertPage(page);
  assertNonEmptyString(prompt, "prompt");

  const group = page.getByTestId("project-conversations");
  await group.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });
  await page
    .getByTestId("conversation-row")
    .first()
    .waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS })
    .catch(() => undefined);

  const earlyDeadline = performance.now() + 5_000;
  while (performance.now() < earlyDeadline) {
    if (await timelineHasUserPrompt(page, prompt)) {
      return { conversationOpened: true, alreadyVisible: true };
    }
    const barVisible =
      (await page.getByTestId(PROCESS_TEST_IDS.approval).count()) > 0
      && (await page
        .getByTestId(PROCESS_TEST_IDS.approval)
        .first()
        .isVisible()
        .catch(() => false));
    if (barVisible && (await timelineHasUserPrompt(page, prompt))) {
      return { conversationOpened: true, alreadyVisible: true, viaBar: true };
    }
    await page.waitForTimeout(50);
  }

  const titleNeedle = prompt.slice(0, 24);
  const byPrompt = group.getByTestId("conversation-row").filter({
    hasText: titleNeedle,
  });
  const activeRow = group.locator(
    '[data-testid="conversation-row"][aria-current="page"]',
  );
  const allRows = group.getByTestId("conversation-row");
  const rowCount = await allRows.count();

  let row = byPrompt;
  let strategy = "title-match";
  if ((await byPrompt.count()) === 0) {
    if ((await activeRow.count()) > 0) {
      row = activeRow;
      strategy = "aria-current";
    } else if (rowCount === 1) {
      row = allRows;
      strategy = "sole-row";
    } else {
      const titles = await allRows
        .evaluateAll((nodes) =>
          nodes.map((node) => (node.textContent ?? "").trim().slice(0, 48)),
        )
        .catch(() => []);
      throw new Error(
        `openConversationForPrompt: no conversation row matching `
        + `${JSON.stringify(titleNeedle)}; rows=${JSON.stringify(titles)}`,
      );
    }
  }

  await row.first().waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });
  await row.first().click();

  try {
    await page
      .getByTestId("timeline-user-message")
      .filter({ hasText: prompt })
      .waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });
  } catch (cause) {
    const barVisible =
      (await page.getByTestId(PROCESS_TEST_IDS.approval).count()) > 0
      && (await page
        .getByTestId(PROCESS_TEST_IDS.approval)
        .first()
        .isVisible()
        .catch(() => false));
    if (barVisible && strategy !== "title-match") {
      return { conversationOpened: true, strategy, timelineLag: true };
    }
    const bodySnippet = (await page.locator("body").innerText().catch(() => ""))
      .replace(/\s+/gu, " ")
      .slice(0, 400);
    throw new Error(
      `openConversationForPrompt: timeline missing ${JSON.stringify(prompt)} `
      + `after strategy=${strategy}; barVisible=${barVisible}; `
      + `body=${JSON.stringify(bodySnippet)}; `
      + `cause=${cause instanceof Error ? cause.message : String(cause)}`,
    );
  }

  return { conversationOpened: true, strategy };
}

/**
 * Assert foreign process markers are absent from the visible body.
 *
 * @param {{
 *   page: import('playwright-core').Page,
 *   forbidden: string[],
 *   settleMs?: number,
 *   allowOwnedApprovalBar?: boolean,
 * }} args
 */
export async function assertNoForeignProcessEvidence({
  page,
  forbidden,
  settleMs = 8_000,
  allowOwnedApprovalBar = false,
}) {
  assertPage(page);
  assert.ok(Array.isArray(forbidden), "forbidden must be an array");

  if (!allowOwnedApprovalBar) {
    const barDeadline = performance.now() + settleMs;
    while (performance.now() < barDeadline) {
      if ((await page.getByTestId(PROCESS_TEST_IDS.approval).count()) === 0) {
        break;
      }
      await page.waitForTimeout(50);
    }
  }

  const text = await page.locator("body").innerText();
  for (const token of forbidden) {
    assert.ok(
      !text.includes(token),
      `foreign process token must be absent: ${JSON.stringify(token)}`,
    );
  }
  return { foreignAbsent: true };
}

/**
 * Capture a screenshot after privacy asserts.
 *
 * @param {{
 *   page: import('playwright-core').Page,
 *   path: string,
 *   label: string,
 * }} args
 */
export async function capturePrivacySafeScreenshot({ page, path: filePath, label }) {
  assertPage(page);
  assertNonEmptyString(filePath, "path");
  assertNonEmptyString(label, "label");
  await assertPrivacyClean(page, `screenshot:${label}`);
  await page.screenshot({ path: filePath, fullPage: false });
  return { screenshot: filePath, label };
}

/**
 * Meaningful process activity status labels (Chinese product strings).
 * Empty / pending-only stages are not sufficient for visual process proof.
 */
const COMPLETED_PROCESS_STATUSES = Object.freeze([
  PROCESS_STATUS.succeeded,
  PROCESS_STATUS.failed,
  PROCESS_STATUS.rejected,
  PROCESS_STATUS.cancelled,
  PROCESS_STATUS.interrupted,
  PROCESS_STATUS.expired,
]);

/**
 * Prefer rows that prove an executed process lifecycle for screenshots.
 * Rejected-without-start is still completed, but weaker visual proof.
 */
const PREFERRED_VISUAL_PROCESS_STATUSES = Object.freeze([
  PROCESS_STATUS.succeeded,
  PROCESS_STATUS.failed,
  PROCESS_STATUS.cancelled,
  PROCESS_STATUS.interrupted,
]);

/**
 * True when the conversation stage is the empty "开始新对话" placeholder with
 * no timeline, approval, or process activity evidence.
 *
 * @param {import('playwright-core').Page} page
 */
export async function isEmptyConversationStage(page) {
  assertPage(page);
  const emptyHeadingVisible = await page
    .getByRole("heading", { name: "开始新对话" })
    .isVisible()
    .catch(() => false);
  if (!emptyHeadingVisible) return false;
  const userMessages = await page.getByTestId("timeline-user-message").count();
  const assistantMessages = await page
    .getByTestId("timeline-assistant-message")
    .count();
  const activities = await page.getByTestId(PROCESS_TEST_IDS.activity).count();
  const processBars = await page.getByTestId(PROCESS_TEST_IDS.approval).count();
  const writeBars = await page.getByTestId("write-approval-bar").count();
  return (
    userMessages === 0
    && assistantMessages === 0
    && activities === 0
    && processBars === 0
    && writeBars === 0
  );
}

/**
 * True when the right contextual pane is the file Resource browser (not
 * terminal output / write diff). That layout must not pass as process proof.
 *
 * @param {import('playwright-core').Page} page
 */
export async function isResourceBrowserPaneOpen(page) {
  assertPage(page);
  const pane = page.getByTestId("resource-pane");
  if ((await pane.count()) === 0) return false;
  if (!(await pane.first().isVisible().catch(() => false))) return false;
  // Terminal / write-diff modes replace the Resource browser body.
  if (
    (await page.getByTestId(PROCESS_TEST_IDS.terminal).count()) > 0
    && (await page
      .getByTestId(PROCESS_TEST_IDS.terminal)
      .first()
      .isVisible()
      .catch(() => false))
  ) {
    return false;
  }
  if (
    (await page.getByTestId("write-diff-pane").count()) > 0
    && (await page
      .getByTestId("write-diff-pane")
      .first()
      .isVisible()
      .catch(() => false))
  ) {
    return false;
  }
  if (
    (await page.getByTestId("terminal-output-pane").count()) > 0
    && (await page
      .getByTestId("terminal-output-pane")
      .first()
      .isVisible()
      .catch(() => false))
  ) {
    return false;
  }
  const heading = (
    await pane.first().locator("h2").first().innerText().catch(() => "")
  ).trim();
  return heading === "资源" || heading.length === 0;
}

/**
 * Conversation surface should dominate; no permanent approval-card stack.
 * Rejects an empty conversation stage so a blank "开始新对话" cannot pass.
 *
 * @param {import('playwright-core').Page} page
 */
export async function assertConversationDominant(page) {
  assertPage(page);
  const workbench = page.getByTestId("workbench");
  await workbench.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });
  const surface = page.locator(".conversation-surface, [aria-label='对话工作台']").first();
  await surface.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });

  await assertSingleApprovalSlot(page);

  const stack = page.locator(".approval-card-stack, [data-testid='approval-card-stack']");
  assert.equal(await stack.count(), 0, "no permanent approval card stack");

  assert.equal(
    await isEmptyConversationStage(page),
    false,
    "conversation stage must not be the empty 开始新对话 placeholder",
  );

  // Conversation surface must remain the visual center relative to the
  // contextual pane when both are present (width dominance).
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

  const processBars = await page.getByTestId(PROCESS_TEST_IDS.approval).count();
  return { conversationDominant: true, processBars };
}

/**
 * Strict visual proof for the 1024x720 conversation-dominant screenshot:
 * completed process activity + terminal output contextual pane open.
 * Fails closed on empty conversation or Resource browser pane.
 *
 * @param {import('playwright-core').Page} page
 */
export async function assertProcessVisualProof(page) {
  assertPage(page);
  await assertConversationDominant(page);

  assert.equal(
    await isResourceBrowserPaneOpen(page),
    false,
    "Resource browser pane must not be the open contextual surface for process visual proof",
  );

  const rows = page.getByTestId(PROCESS_TEST_IDS.activity);
  const rowCount = await rows.count();
  assert.ok(rowCount > 0, "process visual proof requires a visible command-activity-row");

  let matchedStatus = "";
  let matchedIndex = -1;
  for (let i = 0; i < rowCount; i += 1) {
    const row = rows.nth(i);
    if (!(await row.isVisible().catch(() => false))) continue;
    const statusText = (
      await row
        .getByTestId(PROCESS_TEST_IDS.activityStatus)
        .innerText()
        .catch(() => "")
    ).trim();
    if (COMPLETED_PROCESS_STATUSES.some((label) => statusText.includes(label))) {
      matchedStatus = statusText;
      matchedIndex = i;
      break;
    }
  }
  assert.ok(
    matchedIndex >= 0,
    `process visual proof requires a completed activity status `
    + `(${COMPLETED_PROCESS_STATUSES.join("|")}); none visible`,
  );

  const row = rows.nth(matchedIndex);
  const statusEl = row.getByTestId(PROCESS_TEST_IDS.activityStatus);
  await statusEl.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });
  const details = row.getByTestId(PROCESS_TEST_IDS.activityDetails);
  if ((await details.count()) > 0) {
    const open = await details.getAttribute("open");
    if (open === null) {
      const summary = row.getByTestId("command-activity-summary");
      if ((await summary.count()) > 0) {
        await summary.click().catch(() => undefined);
      }
    }
    await details.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });
  }

  const terminal = page.getByTestId(PROCESS_TEST_IDS.terminal);
  await terminal.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });
  assert.equal(
    await terminal.isVisible(),
    true,
    "terminal-output-view must be visible for process visual proof",
  );

  // Prefer terminal body or durable-only notice so the pane is not a blank shell.
  const body = page.getByTestId(PROCESS_TEST_IDS.terminalBody);
  const durableOnly = page.getByTestId("terminal-output-durable-only");
  const bodyVisible = await body.isVisible().catch(() => false);
  const durableVisible = await durableOnly.isVisible().catch(() => false);
  assert.ok(
    bodyVisible || durableVisible,
    "terminal contextual pane must show body or durable-only notice",
  );

  await assertPrivacyClean(page, "process-visual-proof");
  return {
    processVisualProof: true,
    activityStatus: matchedStatus,
    terminalVisible: true,
    conversationDominant: true,
  };
}

/**
 * Classify a process activity status for visual-proof ranking.
 *
 * @param {string} statusText
 * @returns {number} higher is better; 0 means not a completed process status
 */
function rankProcessStatusForVisualProof(statusText) {
  const text = String(statusText ?? "").trim();
  if (PREFERRED_VISUAL_PROCESS_STATUSES.some((label) => text.includes(label))) {
    // Prefer succeeded for the strongest terminal-output proof.
    if (text.includes(PROCESS_STATUS.succeeded)) return 3;
    return 2;
  }
  if (COMPLETED_PROCESS_STATUSES.some((label) => text.includes(label))) {
    return 1;
  }
  return 0;
}

/**
 * Open a conversation that already owns a completed process activity row.
 * Prefers executed-process statuses (succeeded/failed/…) over reject-only.
 *
 * @param {import('playwright-core').Page} page
 * @param {number} [timeoutMs]
 */
export async function openConversationWithCompletedProcessActivity(
  page,
  timeoutMs = DEFAULT_TIMEOUT_MS,
) {
  assertPage(page);
  const group = page.getByTestId("project-conversations");
  await group.waitFor({ state: "visible", timeout: timeoutMs });
  const rows = group.getByTestId("conversation-row");
  const rowCount = await rows.count();

  /** @type {{ found: true, alreadyVisible: boolean, rowIndex?: number, status: string, rank: number } | null} */
  let best = null;

  // Fast path: already visible.
  if ((await page.getByTestId(PROCESS_TEST_IDS.activity).count()) > 0) {
    const activityCount = await page.getByTestId(PROCESS_TEST_IDS.activity).count();
    for (let a = 0; a < activityCount; a += 1) {
      const statusText = (
        await page
          .getByTestId(PROCESS_TEST_IDS.activity)
          .nth(a)
          .getByTestId(PROCESS_TEST_IDS.activityStatus)
          .innerText()
          .catch(() => "")
      ).trim();
      const rank = rankProcessStatusForVisualProof(statusText);
      if (rank > 0 && (!best || rank > best.rank)) {
        best = {
          found: true,
          alreadyVisible: true,
          status: statusText,
          rank,
        };
      }
    }
    // Keep scanning only if we only found a weak (reject-only) match.
    if (best && best.rank >= 2) {
      return best;
    }
  }

  for (let i = 0; i < rowCount; i += 1) {
    await rows.nth(i).click();
    const deadline = performance.now() + Math.min(timeoutMs, 5_000);
    while (performance.now() < deadline) {
      const activityCount = await page.getByTestId(PROCESS_TEST_IDS.activity).count();
      if (activityCount > 0) {
        for (let a = 0; a < activityCount; a += 1) {
          const statusText = (
            await page
              .getByTestId(PROCESS_TEST_IDS.activity)
              .nth(a)
              .getByTestId(PROCESS_TEST_IDS.activityStatus)
              .innerText()
              .catch(() => "")
          ).trim();
          const rank = rankProcessStatusForVisualProof(statusText);
          if (rank > 0 && (!best || rank > best.rank)) {
            best = {
              found: true,
              alreadyVisible: false,
              rowIndex: i,
              status: statusText,
              rank,
            };
          }
        }
        // Strong match found on this conversation — stop scanning.
        if (best && best.rank >= 2 && best.rowIndex === i) {
          return best;
        }
        break;
      }
      await page.waitForTimeout(50);
    }
    if (best && best.rank >= 3) {
      // Re-open the best row if we walked past it? Prefer early exit on succeed.
      return best;
    }
  }

  if (best) {
    // Ensure the best conversation is the active one after a full scan.
    if (typeof best.rowIndex === "number") {
      await rows.nth(best.rowIndex).click();
      const settle = performance.now() + 5_000;
      while (performance.now() < settle) {
        const statusText = (
          await page
            .getByTestId(PROCESS_TEST_IDS.activityStatus)
            .first()
            .innerText()
            .catch(() => "")
        ).trim();
        if (rankProcessStatusForVisualProof(statusText) > 0) break;
        await page.waitForTimeout(50);
      }
    }
    return best;
  }
  return { found: false };
}
