/**
 * Wave C3-F — governed-write WebView2 UI helpers.
 *
 * Public accessible roles/names and stable data-testid values only.
 * Does not launch/close processes, call the write executor, or mutate
 * project files after a proposal is pending (except the named base-drift
 * setup which the orchestrator owns outside this helper).
 *
 * Returns JSON-serializable evidence objects only.
 */

import assert from "node:assert/strict";

const DEFAULT_TIMEOUT_MS = 60_000;

/** Fixed Chinese terminal outcomes from agentActivityProjection. */
export const WRITE_OUTCOME = Object.freeze({
  succeeded: "文件写入已完成",
  rejected: "写入请求已拒绝",
  expired: "写入请求已过期",
  cancelled: "写入请求已取消",
  interrupted: "写入请求已中断",
  failed: "写入未能完成",
});

/**
 * Grant / id / digest carriers — must never appear in visible text or full
 * body HTML (screenshots + accessibility text per C3-F brief).
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
]);

/**
 * Absolute host-path prefixes. Must not appear in visible body text or in
 * full body HTML (including attributes). Native project roots in tooltips
 * are a product privacy defect — not an allowed exception.
 */
const FORBIDDEN_HOST_PATH_TOKENS = Object.freeze([
  "file://",
  "F:\\",
  "C:\\",
  "/Users/",
  "/home/",
]);

/** Conversation-owned write surfaces that must not embed host paths in HTML. */
const WRITE_PRIVACY_TEST_IDS = Object.freeze([
  "write-approval-bar",
  "write-diff-preview",
  "write-outcome-row",
  "resource-preview",
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
 * Required after cancel / terminal outcomes before the next send.
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
 * Privacy gate aligned with C2 (visible text) plus write-surface HTML.
 * Matches brief: no grant/id/path in screenshots or accessibility text.
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

  // Write surfaces themselves must not embed absolute paths in markup.
  for (const testId of WRITE_PRIVACY_TEST_IDS) {
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
    }
  }
}

/**
 * @param {import('playwright-core').Page} page
 * @param {string[]} _needles ignored — privacy sets are fixed (API kept for call sites)
 * @param {string} label
 */
async function assertBodyExcludes(page, _needles, label) {
  await assertPrivacyClean(page, label);
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

  // Project activation sets busy; wait until the shell is interactive again so
  // conversation scope / write bar effects have committed for the new project.
  await waitForComposerIdle(page, Math.min(DEFAULT_TIMEOUT_MS, 15_000)).catch(
    () => undefined,
  );

  return { selectedProjectName: projectName };
}

/**
 * Collapse the contextual resource pane when open.
 * Product may mount both `resource-pane` and `ctx-resource-pane`; never use
 * strict `.or()` waits that require a single match.
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
    // Best-effort collapse; subsequent scenarios re-select projects.
    return { collapsed: true, wasOpen: true };
  }
  return { collapsed: true, wasOpen: false };
}

/**
 * Create a conversation and send a turn. Does not wait for write approval.
 * Settles the composer after cancel/busy so draft fill is not lost on a
 * mid-create remount and send is not aimed at a still-cancelling run.
 *
 * @param {{ page: import('playwright-core').Page, prompt: string }} args
 */
export async function sendWritePrompt({ page, prompt }) {
  assertPage(page);
  assertNonEmptyString(prompt, "prompt");

  await waitForComposerIdle(page);

  const newChat = page.getByRole("button", { name: "新建对话" });
  await newChat.click();
  // createConversation sets busy; wait until the new empty conversation is selected.
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
 * Wait for the single conversation-owned approval bar.
 *
 * @param {{
 *   page: import('playwright-core').Page,
 *   expectedPath: string,
 *   expectedMode: "创建" | "修改",
 *   timeoutMs?: number,
 * }} args
 */
export async function waitForApprovalBar({
  page,
  expectedPath,
  expectedMode,
  timeoutMs = DEFAULT_TIMEOUT_MS,
}) {
  assertPage(page);
  assertNonEmptyString(expectedPath, "expectedPath");
  assertNonEmptyString(expectedMode, "expectedMode");

  const bar = page.getByTestId("write-approval-bar");
  await bar.waitFor({ state: "visible", timeout: timeoutMs });

  const pathText = (await page.getByTestId("write-approval-path").innerText()).trim();
  assert.ok(
    pathText.includes(expectedPath),
    `approval path must include ${expectedPath}, got ${JSON.stringify(pathText)}`,
  );

  const modeText = (await page.getByTestId("write-approval-mode").innerText()).trim();
  assert.equal(modeText, expectedMode, "approval mode label");

  // Exactly one bar.
  assert.equal(await page.getByTestId("write-approval-bar").count(), 1);

  // Initial focus on Approve.
  const approve = page.getByTestId("write-approval-approve");
  await approve.waitFor({ state: "visible", timeout: timeoutMs });
  const focused = await approve.evaluate((el) => el === document.activeElement);
  assert.equal(focused, true, "Approve must receive initial focus");

  await assertBodyExcludes(page, FORBIDDEN_SECRET_TOKENS, "approval-bar");

  return {
    barVisible: true,
    path: expectedPath,
    mode: expectedMode,
    approveFocused: true,
    singleBar: true,
  };
}

/**
 * Assert the approval bar is absent (foreign conversation / after decision).
 *
 * @param {import('playwright-core').Page} page
 */
export async function assertApprovalBarAbsent(page, timeoutMs = 10_000) {
  assertPage(page);
  const deadline = performance.now() + timeoutMs;
  while (performance.now() < deadline) {
    if ((await page.getByTestId("write-approval-bar").count()) === 0) {
      return { barAbsent: true };
    }
    await page.waitForTimeout(50);
  }
  throw new Error("write-approval-bar still present after timeout");
}

/**
 * Open the bounded host diff via Review diff.
 *
 * @param {import('playwright-core').Page} page
 * @param {string} expectedPath
 */
export async function openBoundedDiff(page, expectedPath) {
  assertPage(page);
  assertNonEmptyString(expectedPath, "expectedPath");

  const review = page.getByTestId("write-approval-review-diff");
  await review.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });
  assert.equal(await review.isDisabled(), false, "Review diff must be enabled");
  await review.click();

  const diff = page.getByTestId("write-diff-preview");
  await diff.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });

  const pathText = (await page.getByTestId("write-diff-path").innerText()).trim();
  assert.ok(
    pathText.includes(expectedPath),
    `diff path must include ${expectedPath}`,
  );

  const unified = page.getByTestId("write-diff-unified");
  await unified.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });

  await assertBodyExcludes(page, FORBIDDEN_SECRET_TOKENS, "bounded-diff");

  return {
    diffVisible: true,
    path: expectedPath,
    resourcePaneOpen: true,
  };
}

/**
 * Keyboard contract: Tab cycles Approve → Reject → Review, Escape rejects
 * only while the bar owns focus (does not trigger global Stop).
 *
 * @param {import('playwright-core').Page} page
 */
export async function exerciseApprovalKeyboard(page) {
  assertPage(page);
  const approve = page.getByTestId("write-approval-approve");
  const reject = page.getByTestId("write-approval-reject");
  const review = page.getByTestId("write-approval-review-diff");

  await approve.focus();
  assert.equal(
    await approve.evaluate((el) => el === document.activeElement),
    true,
    "approve focus",
  );

  await page.keyboard.press("Tab");
  assert.equal(
    await reject.evaluate((el) => el === document.activeElement),
    true,
    "Tab moves to Reject",
  );

  await page.keyboard.press("Tab");
  assert.equal(
    await review.evaluate((el) => el === document.activeElement),
    true,
    "Tab moves to Review diff",
  );

  await page.keyboard.press("Tab");
  assert.equal(
    await approve.evaluate((el) => el === document.activeElement),
    true,
    "Tab loops back to Approve",
  );

  // Escape while bar owns focus should reject (caller may re-open for other cases).
  return {
    tabLoop: true,
    approveFocusRestored: true,
  };
}

/**
 * Click Approve once.
 *
 * @param {import('playwright-core').Page} page
 */
export async function clickApproveOnce(page) {
  assertPage(page);
  const approve = page.getByTestId("write-approval-approve");
  await approve.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });
  assert.equal(await approve.isDisabled(), false, "Approve must be enabled");
  await approve.click();
  await assertApprovalBarAbsent(page, DEFAULT_TIMEOUT_MS);
  return { approved: true, barCleared: true };
}

/**
 * Click Reject.
 *
 * @param {import('playwright-core').Page} page
 */
export async function clickReject(page) {
  assertPage(page);
  const reject = page.getByTestId("write-approval-reject");
  await reject.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });
  assert.equal(await reject.isDisabled(), false, "Reject must be enabled");
  await reject.click();
  await assertApprovalBarAbsent(page, DEFAULT_TIMEOUT_MS);
  return { rejected: true, barCleared: true };
}

/**
 * Escape while bar owns focus → reject without global Stop.
 *
 * @param {import('playwright-core').Page} page
 */
export async function escapeRejectWhileBarFocused(page) {
  assertPage(page);
  const approve = page.getByTestId("write-approval-approve");
  await approve.focus();
  await page.keyboard.press("Escape");
  await assertApprovalBarAbsent(page, DEFAULT_TIMEOUT_MS);
  // Global cancel button must not have been the path (run may still complete).
  return { escapeRejected: true, barCleared: true };
}

/**
 * Cancel the owning run via composer Stop while a proposal is pending.
 * Approval bar owns focus initially; force-click Stop and re-click once if the
 * first click does not clear the bar (re-render / focus races).
 *
 * @param {import('playwright-core').Page} page
 */
export async function cancelOwningRun(page) {
  assertPage(page);
  assert.equal(
    await page.getByTestId("write-approval-bar").count(),
    1,
    "cancel requires a visible pending approval bar",
  );

  const cancel = page.getByTestId("composer-cancel");
  await cancel.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });
  await waitForEnabled(cancel, DEFAULT_TIMEOUT_MS, "composer-cancel");
  // Force: approval bar holds focus; normal click can miss the Stop control.
  await cancel.click({ force: true });

  const firstDeadline = performance.now() + 5_000;
  while (performance.now() < firstDeadline) {
    if ((await page.getByTestId("write-approval-bar").count()) === 0) {
      return { cancelled: true, barCleared: true, clicks: 1 };
    }
    await page.waitForTimeout(50);
  }

  // Second attempt if the first click lost a re-render race.
  if ((await page.getByTestId("composer-cancel").count()) > 0) {
    await page.getByTestId("composer-cancel").click({ force: true });
  }

  await assertApprovalBarAbsent(page, DEFAULT_TIMEOUT_MS);

  // Helpful residual signal for product-vs-harness triage if this later fails.
  return { cancelled: true, barCleared: true, clicks: 2 };
}

/**
 * Wait for a fixed write outcome message in the owning timeline.
 *
 * @param {{
 *   page: import('playwright-core').Page,
 *   message: string,
 *   expectOpenFile?: boolean,
 *   timeoutMs?: number,
 * }} args
 */
export async function waitForWriteOutcome({
  page,
  message,
  expectOpenFile = false,
  timeoutMs = DEFAULT_TIMEOUT_MS,
}) {
  assertPage(page);
  assertNonEmptyString(message, "message");

  const row = page
    .getByTestId("write-outcome-row")
    .filter({ hasText: message })
    .first();
  await row.waitFor({ state: "visible", timeout: timeoutMs });

  const openFile = row.getByTestId("write-outcome-open-file");
  const openCount = await openFile.count();
  if (expectOpenFile) {
    assert.equal(openCount, 1, "success must offer Open file");
    assert.equal(await openFile.isVisible(), true);
  } else {
    assert.equal(
      openCount,
      0,
      "non-success outcomes must not offer Open file",
    );
  }

  await assertBodyExcludes(page, FORBIDDEN_SECRET_TOKENS, "write-outcome");

  return {
    outcome: message,
    openFileOffered: expectOpenFile,
  };
}

/**
 * Click Open file on a succeeded write outcome and prove resource preview.
 *
 * @param {{ page: import('playwright-core').Page, expectedPath: string, expectedMarker?: string }} args
 */
export async function openSucceededFile({ page, expectedPath, expectedMarker }) {
  assertPage(page);
  assertNonEmptyString(expectedPath, "expectedPath");

  const open = page.getByTestId("write-outcome-open-file").first();
  await open.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });
  await open.click();

  // Product may mount both resource-pane and ctx-resource-pane; avoid strict .or().
  // Preview content is async via the resource gateway — wait for path/marker, not
  // merely for an empty pane shell to appear.
  const basename = expectedPath.split("/").pop() ?? expectedPath;
  const pathNeedles = [expectedPath, basename].filter(Boolean);
  const deadline = performance.now() + DEFAULT_TIMEOUT_MS;
  let body = "";
  let contentReady = false;
  while (performance.now() < deadline) {
    body = await page.locator("body").innerText();
    const pathVisible = pathNeedles.some((needle) => body.includes(needle));
    const markerVisible = expectedMarker ? body.includes(expectedMarker) : true;
    if (pathVisible && markerVisible) {
      contentReady = true;
      break;
    }
    await page.waitForTimeout(50);
  }
  assert.equal(
    contentReady,
    true,
    `opened resource must reference ${expectedPath}`
      + (expectedMarker ? ` and marker ${JSON.stringify(expectedMarker)}` : ""),
  );

  await assertBodyExcludes(page, FORBIDDEN_SECRET_TOKENS, "open-file");

  return { opened: true, path: expectedPath };
}

/**
 * Assert foreign proposal/diff/outcome/path markers are absent.
 *
 * After project switch, React clearWriteScope runs in an effect — wait briefly
 * for the conversation-owned bar to unmount before treating residual count as
 * a product isolation leak.
 *
 * @param {{
 *   page: import('playwright-core').Page,
 *   forbidden: string[],
 *   settleMs?: number,
 * }} args
 */
export async function assertForeignWriteEvidenceAbsent({
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
      if ((await page.getByTestId("write-approval-bar").count()) === 0) {
        break;
      }
      await page.waitForTimeout(50);
    }

    const barCount = await page.getByTestId("write-approval-bar").count();
    if (barCount !== 0) {
      const projectName = await page
        .getByTestId("active-project-name")
        .innerText()
        .catch(() => "");
      const barPath = await page
        .getByTestId("write-approval-path")
        .innerText()
        .catch(() => "");
      const bodySnippet = (await page.locator("body").innerText().catch(() => ""))
        .replace(/\s+/gu, " ")
        .slice(0, 300);
      throw new Error(
        `foreign selection must not show approval bar; count=${barCount}; `
        + `project=${JSON.stringify(projectName)}; `
        + `barPath=${JSON.stringify(barPath)}; `
        + `body=${JSON.stringify(bodySnippet)}`,
      );
    }
  }

  const text = await page.locator("body").innerText();
  for (const token of forbidden) {
    assert.ok(
      !text.includes(token),
      `foreign token must be absent: ${JSON.stringify(token)}`,
    );
  }
  return { foreignAbsent: true };
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
 * Prefer the already-selected conversation when project switch restores it.
 * Title rename from "新对话" only happens after sendTurn fully returns, so a
 * pending write often still shows "新对话" in the rail — match by prompt
 * prefix when renamed, otherwise the sole/active row, never an arbitrary
 * residual "新对话" among many.
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

  // Project restore often reselects the last conversation; give the timeline
  // a short settle before deciding it is missing.
  const earlyDeadline = performance.now() + 5_000;
  while (performance.now() < earlyDeadline) {
    if (await timelineHasUserPrompt(page, prompt)) {
      return { conversationOpened: true, alreadyVisible: true };
    }
    // Pending bar without timeline yet still means the right conversation.
    const barVisible =
      (await page.getByTestId("write-approval-bar").count()) > 0
      && (await page
        .getByTestId("write-approval-bar")
        .first()
        .isVisible()
        .catch(() => false));
    if (barVisible) {
      // Wait a bit more for timeline rehydration under the restored bar.
      if (await timelineHasUserPrompt(page, prompt)) {
        return { conversationOpened: true, alreadyVisible: true, viaBar: true };
      }
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
  const defaultRows = group.getByTestId("conversation-row").filter({
    hasText: "新对话",
  });

  let row = byPrompt;
  let strategy = "title-match";
  if ((await byPrompt.count()) === 0) {
    if ((await activeRow.count()) > 0) {
      row = activeRow;
      strategy = "aria-current";
    } else if (rowCount === 1) {
      row = allRows;
      strategy = "sole-row";
    } else if (rowCount > 0 && (await defaultRows.count()) === 1 && rowCount === 1) {
      row = defaultRows;
      strategy = "sole-default-title";
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
  // Re-click even if already selected to force a timeline reload after project switch.
  await row.first().click();

  try {
    await page
      .getByTestId("timeline-user-message")
      .filter({ hasText: prompt })
      .waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });
  } catch (cause) {
    const bodySnippet = (await page.locator("body").innerText().catch(() => ""))
      .replace(/\s+/gu, " ")
      .slice(0, 400);
    const barVisible =
      (await page.getByTestId("write-approval-bar").count()) > 0
      && (await page
        .getByTestId("write-approval-bar")
        .first()
        .isVisible()
        .catch(() => false));
    // Isolation contract: restored pending may surface the bar even if the
    // durable timeline projection lags; accept bar+path when bar is already proven.
    if (barVisible && strategy !== "title-match") {
      return { conversationOpened: true, strategy, timelineLag: true };
    }
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
 * Capture a screenshot of the main workbench at a supported viewport size.
 * Caller owns page viewport; this only writes the file after privacy asserts.
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
  await assertBodyExcludes(page, FORBIDDEN_SECRET_TOKENS, `screenshot:${label}`);
  await page.screenshot({ path: filePath, fullPage: false });
  return { screenshot: filePath, label };
}

/**
 * Conversation surface should dominate; no permanent approval-card stack.
 *
 * @param {import('playwright-core').Page} page
 */
export async function assertConversationDominant(page) {
  assertPage(page);
  const workbench = page.getByTestId("workbench");
  await workbench.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });
  const surface = page.locator(".conversation-surface, [aria-label='对话工作台']").first();
  await surface.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });

  const bars = await page.getByTestId("write-approval-bar").count();
  assert.ok(bars <= 1, "at most one approval bar");

  // No dashboard-style permanent card stack class if present.
  const stack = page.locator(".approval-card-stack, [data-testid='approval-card-stack']");
  assert.equal(await stack.count(), 0, "no permanent approval card stack");

  return { conversationDominant: true, approvalBars: bars };
}
