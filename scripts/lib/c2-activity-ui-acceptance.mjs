/**
 * C2-E conversation / agent-activity UI helpers for real WebView2 acceptance.
 *
 * Public accessible roles/names and stable data-testid values only.
 * Does not select projects, launch/close processes, touch SQLite/filesystem,
 * or expose raw run/session/tool/sub-agent IDs.
 *
 * Committed fake-runtime fixture (tests/fixtures/protocol/fake-turn.jsonl):
 * plan "Scan files" / "Write summary", tool list_dir ("list workspace root"),
 * sub-agent "explore", artifact workspace-summary.md, terminal assistant text.
 */

const DEFAULT_TIMEOUT_MS = 45_000;

/** Terminal assistant body from the committed fake-turn fixture. */
const TERMINAL_ASSISTANT_RE =
  /Workspace contains[\s\S]*Ready for deeper work\.?/;

/** Plan step titles from the committed fake-turn fixture. */
const FIXTURE_PLAN_STEPS = Object.freeze(["Scan files", "Write summary"]);

/** Tool title prefers summary, falls back to name. */
const FIXTURE_TOOL_RE = /list workspace root|list_dir/i;

/** Fail-closed local status for a tool that returned provider output. */
const FIXED_TOOL_OUTPUT_STATUS = "工具已返回输出。";

/** Provider-owned fixture content that must never reach activity DOM. */
const FORBIDDEN_RAW_TOOL_OUTPUT = Object.freeze([
  "crates/",
  "apps/",
  "README.md",
]);

/** Sub-agent role/title and progress from the fixture. */
const FIXTURE_SUBAGENT_RE = /explore/i;
const FIXTURE_SUBAGENT_PROGRESS_RE = /indexed 3 top-level entries/i;

/** Portable artifact path from the fixture. */
const FIXTURE_ARTIFACT_PATH = "artifacts/workspace-summary.md";

/** Human status label for completed runs. */
const COMPLETED_STATUS_RE = /已完成/;

/**
 * Raw fixture identity tokens that must never appear in UI evidence or DOM.
 * (tool_call_id / subagent_id / artifact_id — not human titles.)
 */
const FORBIDDEN_RAW_IDS = Object.freeze([
  "tc-list",
  "sa-explore",
  "art-summary",
]);

/** Debug and host-path carriers must not appear in activity text. */
const FORBIDDEN_DEBUG_OR_PATH_TOKENS = Object.freeze([
  "debug",
  "api_token",
  "file://",
  "F:\\",
  "\\\\",
  "/home/",
  "/Users/",
]);

/**
 * @param {import('playwright-core').Page} page
 * @param {string} prompt
 * @returns {Promise<Record<string, unknown>>}
 */
export async function createConversationActivity({ page, prompt }) {
  assertPage(page);
  assertNonEmptyString(prompt, "prompt");

  await page.getByRole("button", { name: "新建对话" }).click();

  const composer = page.getByRole("textbox", { name: "工作请求" });
  await composer.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });
  await composer.fill(prompt);

  const send = page.getByTestId("composer-send");
  await send.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });
  // Controlled composer enables send only after non-empty draft.
  await waitForEnabled(send, DEFAULT_TIMEOUT_MS, "composer-send");
  await send.click();

  await page
    .getByTestId("timeline-user-message")
    .filter({ hasText: prompt })
    .waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });

  await page
    .getByTestId("timeline-assistant-message")
    .filter({ hasText: TERMINAL_ASSISTANT_RE })
    .waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });

  const activity = await assertFakeRuntimeActivityExpanded(page);

  return {
    phase: "create",
    promptPresent: true,
    assistantTerminal: true,
    ...activity,
    sentTurn: true,
  };
}

/**
 * Prove prompt, assistant content, and activity restore without resending.
 *
 * @param {import('playwright-core').Page} page
 * @param {string} prompt
 * @param {string} foreignPrompt
 * @returns {Promise<Record<string, unknown>>}
 */
export async function assertConversationActivityRestored({
  page,
  prompt,
  foreignPrompt,
}) {
  assertPage(page);
  assertNonEmptyString(prompt, "prompt");
  assertNonEmptyString(foreignPrompt, "foreignPrompt");

  // Restore must not create a conversation or send another turn.
  await page
    .getByTestId("timeline-user-message")
    .filter({ hasText: prompt })
    .waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });

  await page
    .getByTestId("timeline-assistant-message")
    .filter({ hasText: TERMINAL_ASSISTANT_RE })
    .waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });

  const bodyText = await page.locator("body").innerText();
  assert(
    !bodyText.includes(foreignPrompt),
    `Restored conversation must not display foreign prompt ${JSON.stringify(foreignPrompt)}.`,
  );

  const activity = await assertFakeRuntimeActivityExpanded(page);

  return {
    phase: "restore",
    promptPresent: true,
    foreignPromptAbsent: true,
    assistantTerminal: true,
    sentTurn: false,
    ...activity,
  };
}

/**
 * Prove a foreign project's prompt and conversation-owned activity are absent.
 *
 * @param {import('playwright-core').Page} page
 * @param {string} forbiddenPrompt
 * @returns {Promise<Record<string, unknown>>}
 */
export async function assertConversationActivityAbsent({
  page,
  forbiddenPrompt,
}) {
  assertPage(page);
  assertNonEmptyString(forbiddenPrompt, "forbiddenPrompt");

  const bodyText = await page.locator("body").innerText();
  assert(
    !bodyText.includes(forbiddenPrompt),
    `Forbidden prompt must be absent: ${JSON.stringify(forbiddenPrompt)}.`,
  );

  const userMessages = page.getByTestId("timeline-user-message");
  const userCount = await userMessages.count();
  for (let index = 0; index < userCount; index += 1) {
    const text = await userMessages.nth(index).innerText();
    assert(
      !text.includes(forbiddenPrompt),
      `timeline-user-message[${index}] must not contain forbidden prompt.`,
    );
  }

  const conversationRows = page.getByTestId("conversation-row");
  const rowCount = await conversationRows.count();
  for (let index = 0; index < rowCount; index += 1) {
    const text = await conversationRows.nth(index).innerText();
    assert(
      !text.includes(forbiddenPrompt),
      `conversation-row[${index}] must not list a conversation owned by the forbidden prompt.`,
    );
  }

  // Foreign conversation-owned activity: no disclosure body may claim ownership
  // of the forbidden prompt. Fixture plan titles alone are not decisive because
  // the fake runtime is shared; ownership is conversation-scoped via the prompt.
  const disclosures = page.getByTestId("agent-activity-disclosure");
  const disclosureCount = await disclosures.count();
  for (let index = 0; index < disclosureCount; index += 1) {
    const text = await disclosures.nth(index).innerText();
    assert(
      !text.includes(forbiddenPrompt),
      `agent-activity-disclosure[${index}] must not surface the forbidden prompt.`,
    );
  }

  return {
    phase: "absent",
    forbiddenPromptAbsent: true,
    forbiddenUserMessageCount: 0,
    conversationRowsChecked: rowCount,
    activityDisclosuresChecked: disclosureCount,
    foreignConversationActivityAbsent: true,
  };
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/**
 * Expand the real agent-activity-disclosure and assert fake-runtime content.
 * @param {import('playwright-core').Page} page
 */
async function assertFakeRuntimeActivityExpanded(page) {
  const disclosure = page.getByTestId("agent-activity-disclosure");
  await disclosure.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });

  const details = disclosure.getByTestId("agent-activity-details").first();
  const summary = disclosure.getByTestId("agent-activity-summary").first();

  await summary
    .filter({ hasText: COMPLETED_STATUS_RE })
    .waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });

  const openAttr = await details.getAttribute("open");
  if (openAttr === null) {
    await summary.click();
  }

  // Progressive disclosure: outer details must be open before nested checks.
  await waitForAttribute(details, "open", DEFAULT_TIMEOUT_MS, "agent-activity-details");

  const steps = disclosure.getByTestId("agent-activity-steps");
  await steps.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });
  const stepsText = await steps.innerText();
  for (const title of FIXTURE_PLAN_STEPS) {
    assert(
      stepsText.includes(title),
      `Plan steps must include ${JSON.stringify(title)} from the fake runtime fixture.`,
    );
  }

  const tools = disclosure.getByTestId("agent-activity-tools");
  await tools.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });
  const toolsHeaderText = await tools.innerText();
  assert(
    FIXTURE_TOOL_RE.test(toolsHeaderText),
    "Tool row must show list_dir / list workspace root from the fake runtime fixture.",
  );

  // Nested tool output starts closed — expand to verify only fixed local copy.
  const outputSummary = tools.locator("summary").filter({ hasText: /^输出$/ });
  if ((await outputSummary.count()) > 0) {
    const nested = outputSummary.first().locator("xpath=ancestor::details[1]");
    const nestedOpen = await nested.getAttribute("open");
    if (nestedOpen === null) {
      await outputSummary.first().click();
    }
  }
  const toolsText = await tools.innerText();
  assert(
    toolsText.includes(FIXED_TOOL_OUTPUT_STATUS),
    "Expanded tool output must show the fixed local returned-output status.",
  );

  const subagents = disclosure.getByTestId("agent-activity-subagents");
  await subagents.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });
  const subagentsText = await subagents.innerText();
  assert(
    FIXTURE_SUBAGENT_RE.test(subagentsText),
    "Sub-agent row must include explore from the fake runtime fixture.",
  );
  assert(
    FIXTURE_SUBAGENT_PROGRESS_RE.test(subagentsText),
    "Sub-agent progress must include 'indexed 3 top-level entries'.",
  );

  const artifactsToggle = disclosure.getByTestId(
    "agent-activity-artifacts-toggle",
  );
  await artifactsToggle.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });
  const artifactsDetails = artifactsToggle.locator(
    "xpath=ancestor::details[1]",
  );
  if ((await artifactsDetails.getAttribute("open")) === null) {
    await artifactsToggle.click();
  }
  const artifacts = disclosure.getByTestId("agent-activity-artifacts");
  await artifacts.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });
  const artifactsText = await artifacts.innerText();
  assert(
    artifactsText.includes(FIXTURE_ARTIFACT_PATH),
    "Artifact disclosure must include the portable fixture artifact path.",
  );

  const summaryText = await summary.innerText();
  assert(
    COMPLETED_STATUS_RE.test(summaryText),
    "Activity summary must show completed status (已完成).",
  );

  const disclosureText = await disclosure.innerText();
  assertNoRawProviderMaterial(disclosureText, "agent-activity-disclosure");

  return {
    activityVisible: true,
    activityExpanded: true,
    planSteps: [...FIXTURE_PLAN_STEPS],
    toolMatched: true,
    toolOutputStateFixed: true,
    subagentMatched: true,
    subagentProgressMatched: true,
    artifactMatched: true,
    completed: true,
    rawProviderMaterialAbsent: true,
  };
}

/**
 * @param {unknown} page
 */
function assertPage(page) {
  assert(
    page != null && typeof page === "object",
    "page is required (Playwright Page over WebView2 CDP).",
  );
  assert(
    typeof page.getByRole === "function"
      && typeof page.getByTestId === "function",
    "page must expose Playwright getByRole/getByTestId.",
  );
}

/**
 * @param {unknown} value
 * @param {string} name
 */
function assertNonEmptyString(value, name) {
  assert(
    typeof value === "string" && value.trim().length > 0,
    `${name} must be a non-empty string.`,
  );
}

/**
 * @param {unknown} condition
 * @param {string} message
 */
function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

/**
 * @param {string} text
 * @param {string} region
 */
function assertNoRawProviderMaterial(text, region) {
  for (const id of FORBIDDEN_RAW_IDS) {
    assert(
      !text.includes(id),
      `${region} must not expose raw fixture id ${JSON.stringify(id)}.`,
    );
  }
  for (const output of FORBIDDEN_RAW_TOOL_OUTPUT) {
    assert(
      !text.includes(output),
      `${region} must not expose raw fixture output ${JSON.stringify(output)}.`,
    );
  }
  for (const token of FORBIDDEN_DEBUG_OR_PATH_TOKENS) {
    assert(
      !text.includes(token),
      `${region} must not expose debug or host-path token ${JSON.stringify(token)}.`,
    );
  }
}

/**
 * @param {import('playwright-core').Locator} locator
 * @param {number} timeoutMs
 * @param {string} label
 */
async function waitForEnabled(locator, timeoutMs, label) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (await locator.isEnabled()) {
      return;
    }
    await sleep(50);
  }
  throw new Error(`${label} did not become enabled within ${timeoutMs}ms.`);
}

/**
 * @param {import('playwright-core').Locator} locator
 * @param {string} name
 * @param {number} timeoutMs
 * @param {string} label
 */
async function waitForAttribute(locator, name, timeoutMs, label) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const value = await locator.getAttribute(name);
    if (value !== null) {
      return value;
    }
    await sleep(50);
  }
  throw new Error(`${label} attribute ${name} not set within ${timeoutMs}ms.`);
}

/**
 * @param {number} ms
 */
function sleep(ms) {
  return new Promise((resolve) => {
    setTimeout(resolve, ms);
  });
}
