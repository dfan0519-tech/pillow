/**
 * Permission rules UI — settings list, form validation, project+allow block,
 * approval-bar always-allow exact pattern + word-boundary note, timeline
 * auto-approve visibility, dual-skin body contrast.
 */
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { AgentActivityDisclosure } from "../src/features/conversations/AgentActivityDisclosure";
import { ConversationTimeline } from "../src/features/conversations/ConversationTimeline";
import type {
  ConversationTimelineView,
  RunActivityView,
} from "../src/features/conversations/conversationTypes";
import { CommandApprovalBar } from "../src/features/execution/CommandApprovalBar";
import {
  buildProcessActivityModels,
} from "../src/features/execution/processActivityModel";
import type { CommandProposalView } from "../src/features/execution/executionTypes";
import {
  PERMISSION_WORD_BOUNDARY_NOTE,
  suggestPatternFromSubject,
} from "../src/features/permissions/permissionPattern";
import { encodeProcessSubject } from "../src/features/permissions/processSubject";
import { SettingsPanel } from "../src/features/settings/SettingsPanel";
import { MockSettingsGateway } from "../src/features/settings/settingsGateway";
import {
  MockPermissionRulesGateway,
  isProjectAllowForbidden,
  parseUnreadablePermissionRuleView,
  type PermissionRuleView,
  type UnreadablePermissionRuleView,
} from "../src/lib/permissionRulesGateway";
import type { RuntimeEventDto } from "../src/lib/conversationGateway";
import "../src/styles/tokens.css";
import "../src/styles/settings.css";
import "../src/styles/permission-rules.css";
import "../src/styles/execution.css";
import "../src/styles/agent-activity.css";

// —— contrast helpers (same WCAG path as visual-shell-contract) ——————————————

function parseHex(hex: string): [number, number, number] {
  const raw = hex.replace("#", "").trim();
  const full =
    raw.length === 3
      ? raw.split("").map((c) => c + c).join("")
      : raw;
  const n = Number.parseInt(full, 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

function relativeLuminance(hex: string): number {
  const channels = parseHex(hex).map((channel) => {
    const s = channel / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * channels[0]! + 0.7152 * channels[1]! + 0.0722 * channels[2]!;
}

function contrastRatio(a: string, b: string): number {
  const l1 = relativeLuminance(a);
  const l2 = relativeLuminance(b);
  const lighter = Math.max(l1, l2);
  const darker = Math.min(l1, l2);
  return (lighter + 0.05) / (darker + 0.05);
}

function readToken(name: string, theme: "dark" | "light"): string {
  const root = document.documentElement;
  root.setAttribute("data-theme", theme);
  // Ensure tokens stylesheet is applied via imported CSS.
  const value = getComputedStyle(root).getPropertyValue(name).trim();
  return value;
}

// —— fixtures ————————————————————————————————————————————————————————————————

const GLOBAL_RULE: PermissionRuleView = {
  rule_id: "rule-global-1",
  scope: "global",
  prefix: "process",
  pattern: "cargo *",
  action: "allow",
  created_at: "2026-08-10T00:00:00.000Z",
};

const PROJECT_RULE: PermissionRuleView = {
  rule_id: "rule-project-1",
  scope: "project",
  project_id: "proj-a",
  prefix: "write",
  pattern: ".env*",
  action: "deny",
  created_at: "2026-08-10T00:01:00.000Z",
};

/**
 * The row EA-03 sequence B plants: a blank global process allow. The host
 * reports it instead of loading it, and reports no pattern for it.
 */
const UNREADABLE_ROW: UnreadablePermissionRuleView = {
  rule_id: "11111111-1111-4111-8111-111111111111",
  scope: "global",
  prefix: "process",
  action: "allow",
  created_at: "2026-08-12T10:00:00.000Z",
  reason: "pattern",
};

function pendingCommand(
  overrides: Partial<CommandProposalView> = {},
): CommandProposalView {
  return {
    proposalId: "prop-cmd-raw-id",
    conversationId: "conv-a",
    origin: "agent",
    state: "pending_approval",
    programLabel: "cargo",
    argv: ["test", "-p", "kernel-policy"],
    portableCwd: ".",
    riskClass: "standard",
    summary: "Run focused tests",
    expiresAt: new Date(Date.now() + 60_000).toISOString(),
    ...overrides,
  };
}

// —— 1. Rule list render + delete ————————————————————————————————————————————

describe("Permission rules — settings list", () => {
  it("1 — renders global and project rules and deletes a rule", async () => {
    const user = userEvent.setup();
    const rulesGateway = new MockPermissionRulesGateway([
      GLOBAL_RULE,
      PROJECT_RULE,
    ]);
    const settings = new MockSettingsGateway();

    render(
      <SettingsPanel
        gateway={settings}
        onClose={() => undefined}
        permissionRulesGateway={rulesGateway}
        projectId="proj-a"
      />,
    );

    await waitFor(() => {
      expect(screen.getByTestId("permission-rules-section")).toBeInTheDocument();
    });

    const globalList = await screen.findByTestId("permission-rules-global-list");
    expect(within(globalList).getByTestId("permission-rule-pattern"))
      .toHaveTextContent("process:cargo *");
    expect(within(globalList).getByTestId("permission-rule-action"))
      .toHaveTextContent("允许");

    const projectList = screen.getByTestId("permission-rules-project-list");
    expect(within(projectList).getByTestId("permission-rule-pattern"))
      .toHaveTextContent("write:.env*");
    expect(within(projectList).getByTestId("permission-rule-action"))
      .toHaveTextContent("拒绝");

    const deleteButtons = within(globalList).getAllByTestId(
      "permission-rule-delete",
    );
    await user.click(deleteButtons[0]!);

    await waitFor(() => {
      expect(screen.queryByTestId("permission-rules-global-list")).toBeNull();
    });
    expect(rulesGateway.getSnapshot().some((r) => r.rule_id === "rule-global-1"))
      .toBe(false);
    // Project rule remains.
    expect(screen.getByTestId("permission-rules-project-list")).toBeInTheDocument();
  });
});

// —— 1b. Unreadable row recovery ————————————————————————————————————————————

describe("Permission rules — unreadable row recovery", () => {
  it("1b — shows a quarantined row with its reason and removes it in-product", async () => {
    const user = userEvent.setup();
    const rulesGateway = new MockPermissionRulesGateway(
      [GLOBAL_RULE],
      [UNREADABLE_ROW],
    );
    const settings = new MockSettingsGateway();

    render(
      <SettingsPanel
        gateway={settings}
        onClose={() => undefined}
        permissionRulesGateway={rulesGateway}
        projectId="proj-a"
      />,
    );

    // One unreadable row must not blank the surface: the user's rule still shows.
    const globalList = await screen.findByTestId("permission-rules-global-list");
    expect(within(globalList).getByTestId("permission-rule-pattern"))
      .toHaveTextContent("process:cargo *");

    const block = screen.getByTestId("permission-rules-unreadable-block");
    expect(within(block).getByTestId("permission-rules-unreadable-note"))
      .toHaveTextContent("已被忽略");
    expect(within(block).getByTestId("permission-rule-unreadable-shape"))
      .toHaveTextContent("全局");
    expect(
      within(block).getByTestId("permission-rule-unreadable-reason").textContent,
    ).toContain("空模式会被内核当作匹配全部");

    await user.click(
      within(block).getByTestId("permission-rule-unreadable-delete"),
    );

    await waitFor(() => {
      expect(screen.queryByTestId("permission-rules-unreadable-block")).toBeNull();
    });
    expect(rulesGateway.getUnreadableSnapshot()).toEqual([]);
    // The repair removed only the quarantined row.
    expect(rulesGateway.getSnapshot().map((rule) => rule.rule_id))
      .toEqual(["rule-global-1"]);
    expect(screen.getByTestId("permission-rules-global-list")).toBeInTheDocument();
  });

  it("1b2 — a clean store renders no unreadable block at all", async () => {
    const rulesGateway = new MockPermissionRulesGateway([GLOBAL_RULE]);

    render(
      <SettingsPanel
        gateway={new MockSettingsGateway()}
        onClose={() => undefined}
        permissionRulesGateway={rulesGateway}
        projectId="proj-a"
      />,
    );

    await screen.findByTestId("permission-rules-global-list");
    expect(screen.queryByTestId("permission-rules-unreadable-block")).toBeNull();
  });

  it("1b3 — the parser keeps recognition fields and drops an untrusted pattern", () => {
    const parsed = parseUnreadablePermissionRuleView({
      rule_id: "11111111-1111-4111-8111-111111111111",
      scope: "global",
      prefix: "process",
      action: "allow",
      created_at: "2026-08-12T10:00:00Z",
      reason: "content_digest",
      // A host must not send this, and the renderer must not carry it: the
      // pattern of a row the host cannot vouch for is not shown as the user's.
      pattern: "rm -rf *",
    });

    expect(Object.keys(parsed).sort()).toEqual([
      "action",
      "created_at",
      "prefix",
      "reason",
      "rule_id",
      "scope",
    ]);
    expect(parsed).not.toHaveProperty("pattern");

    // A row with no reason is not a report the UI can act on.
    expect(() =>
      parseUnreadablePermissionRuleView({
        rule_id: "11111111-1111-4111-8111-111111111111",
      })
    ).toThrow(/invalid permission rules response/i);
  });
});

// —— 2. Add form validation ————————————————————————————————————————————————

describe("Permission rules — add form validation", () => {
  it("2 — empty pattern keeps submit disabled; valid add appends a row", async () => {
    const user = userEvent.setup();
    const rulesGateway = new MockPermissionRulesGateway();
    const settings = new MockSettingsGateway();

    render(
      <SettingsPanel
        gateway={settings}
        onClose={() => undefined}
        permissionRulesGateway={rulesGateway}
        projectId={null}
      />,
    );

    await waitFor(() => {
      expect(screen.getByTestId("permission-rules-add-form")).toBeInTheDocument();
    });

    const submit = screen.getByTestId("permission-rules-form-submit");
    expect(submit).toBeDisabled();

    await user.type(
      screen.getByTestId("permission-rules-form-pattern"),
      "cargo *",
    );
    expect(submit).not.toBeDisabled();

    await user.click(submit);

    await waitFor(() => {
      expect(screen.getByTestId("permission-rules-global-list")).toBeInTheDocument();
    });
    expect(screen.getByTestId("permission-rule-pattern")).toHaveTextContent(
      "process:cargo *",
    );
    expect(rulesGateway.getSnapshot()).toHaveLength(1);
    expect(rulesGateway.getSnapshot()[0]!.action).toBe("allow");
  });
});

// —— 3. Project scope allow disabled (negative control) ——————————————————————

describe("Permission rules — project allow UI block", () => {
  it("3 — project scope disables allow in the form (not backend-only)", async () => {
    const user = userEvent.setup();
    const rulesGateway = new MockPermissionRulesGateway();
    const addSpy = vi.spyOn(rulesGateway, "addPermissionRule");
    const settings = new MockSettingsGateway();

    render(
      <SettingsPanel
        gateway={settings}
        onClose={() => undefined}
        permissionRulesGateway={rulesGateway}
        projectId="proj-a"
      />,
    );

    await waitFor(() => {
      expect(screen.getByTestId("permission-rules-form-scope")).toBeInTheDocument();
    });

    await user.selectOptions(
      screen.getByTestId("permission-rules-form-scope"),
      "project",
    );

    const actionSelect = screen.getByTestId(
      "permission-rules-form-action",
    ) as HTMLSelectElement;
    // Action control itself is disabled under project scope.
    expect(actionSelect).toBeDisabled();
    expect(actionSelect.value).toBe("deny");

    const allowOption = within(actionSelect).getByRole("option", {
      name: "允许",
    }) as HTMLOptionElement;
    expect(allowOption).toBeDisabled();

    // Pure helper used by the form gate.
    expect(isProjectAllowForbidden("project", "allow")).toBe(true);
    expect(isProjectAllowForbidden("project", "deny")).toBe(false);
    expect(isProjectAllowForbidden("global", "allow")).toBe(false);

    // Visible explanation before submit (negative control — not a backend error).
    const block = screen.getByTestId("permission-rules-project-allow-blocked");
    expect(block.textContent ?? "").toMatch(/项目作用域只能拒绝/);
    expect(block.textContent ?? "").toMatch(/允许选项已禁用/);

    await user.type(
      screen.getByTestId("permission-rules-form-pattern"),
      "src/**",
    );

    // Submit is enabled only for deny under project; backend never sees allow.
    await user.click(screen.getByTestId("permission-rules-form-submit"));

    await waitFor(() => {
      expect(addSpy).toHaveBeenCalled();
    });
    expect(addSpy).toHaveBeenCalledWith(
      "project",
      "proj-a",
      "process",
      "src/**",
      "deny",
    );
    // Never called with allow under project.
    expect(
      addSpy.mock.calls.some((call) => call[4] === "allow" && call[0] === "project"),
    ).toBe(false);
  });

  it("3b — Mock gateway rejects project+allow if forced (host parity)", async () => {
    const rulesGateway = new MockPermissionRulesGateway();
    await expect(
      rulesGateway.addPermissionRule("project", "proj-a", "process", "cargo *", "allow"),
    ).rejects.toMatchObject({
      code: "permission_rules_project_allow_forbidden",
    });
  });
});

// —— 4. Approval bar always-allow exact pattern + word boundary ——————————————

describe("Permission rules — always allow from approval bar", () => {
  it("4 — shows exact pattern preview and word-boundary note", async () => {
    const user = userEvent.setup();
    const rulesGateway = new MockPermissionRulesGateway();
    const proposal = pendingCommand();
    // Host subjects are v1-encoded since EA-02; a flat "cargo test -p x"
    // string is no longer a shape the host can produce.
    const ruleSubject = encodeProcessSubject("cargo", [
      "test",
      "-p",
      "kernel-policy",
    ]);

    render(
      <CommandApprovalBar
        decisionAvailable
        onDecide={vi.fn()}
        onReview={vi.fn()}
        permissionRulesGateway={rulesGateway}
        projectId="proj-a"
        proposal={proposal}
        ruleSubject={ruleSubject}
      />,
    );

    expect(screen.getByTestId("command-approval-always-allow")).toHaveTextContent(
      "总是允许此模式",
    );
    await user.click(screen.getByTestId("command-approval-always-allow"));

    const dialog = await screen.findByTestId("always-rule-dialog");
    expect(dialog).toBeVisible();

    const suggested = suggestPatternFromSubject("process", ruleSubject, "allow");
    // Exact pattern must be visible — not just a vague “always allow”.
    const patternInput = within(dialog).getByTestId(
      "always-rule-pattern",
    ) as HTMLInputElement;
    expect(patternInput.value).toBe(suggested);
    expect(suggested).toBe("cargo test -p kernel-policy");
    expect(
      within(dialog).queryByTestId("always-rule-overbroad-warning"),
    ).toBeNull();

    const preview = within(dialog).getByTestId("always-rule-exact-preview");
    expect(preview.textContent).toContain(`allow process:${suggested}`);
    expect(preview.textContent).toMatch(/将写入/);

    const note = within(dialog).getByTestId("always-rule-word-boundary-note");
    expect(note).toHaveTextContent(PERMISSION_WORD_BOUNDARY_NOTE);
    expect(note.textContent).toMatch(/cargotest/);
    expect(note.textContent).toMatch(/cargo \*/);

    await user.clear(patternInput);
    await user.type(patternInput, "cargo *");
    const overbroad = within(dialog).getByTestId("always-rule-overbroad-warning");
    expect(overbroad).toBeVisible();
    expect(overbroad.textContent).toMatch(/cargo test\.other -p kernel-policy/);
    await user.click(within(dialog).getByTestId("always-rule-submit"));

    await waitFor(() => {
      expect(screen.queryByTestId("always-rule-dialog")).toBeNull();
    });
    const stored = rulesGateway.getSnapshot();
    expect(stored).toHaveLength(1);
    expect(stored[0]).toMatchObject({
      scope: "global",
      prefix: "process",
      pattern: "cargo *",
      action: "allow",
    });
  });

  it("4e — always-allow of node build.js warns with a real contrast if the user widens to node*", async () => {
    const user = userEvent.setup();
    const rulesGateway = new MockPermissionRulesGateway();
    const ruleSubject = encodeProcessSubject("node", ["build.js"]);

    render(
      <CommandApprovalBar
        decisionAvailable
        onDecide={vi.fn()}
        onReview={vi.fn()}
        permissionRulesGateway={rulesGateway}
        projectId="proj-a"
        proposal={pendingCommand({
          programLabel: "node",
          argv: ["build.js"],
          summary: "run build",
        })}
        ruleSubject={ruleSubject}
      />,
    );

    await user.click(screen.getByTestId("command-approval-always-allow"));
    const dialog = await screen.findByTestId("always-rule-dialog");
    const patternInput = within(dialog).getByTestId(
      "always-rule-pattern",
    ) as HTMLInputElement;
    expect(patternInput.value).toBe("node build.js");
    expect(
      within(dialog).queryByTestId("always-rule-overbroad-warning"),
    ).toBeNull();

    await user.clear(patternInput);
    await user.type(patternInput, "node*");
    const warning = within(dialog).getByTestId("always-rule-overbroad-warning");
    expect(warning).toBeVisible();
    expect(warning.textContent).toContain("node build.js.other");
    expect(warning.textContent).toMatch(/还会自动放行/);
  });

  it("4b — always deny can target project scope", async () => {
    const user = userEvent.setup();
    const rulesGateway = new MockPermissionRulesGateway();

    render(
      <CommandApprovalBar
        decisionAvailable
        onDecide={vi.fn()}
        onReview={vi.fn()}
        permissionRulesGateway={rulesGateway}
        projectId="proj-a"
        proposal={pendingCommand()}
        ruleSubject="cargo"
      />,
    );

    await user.click(screen.getByTestId("command-approval-always-deny"));
    const dialog = await screen.findByTestId("always-rule-dialog");
    // Default deny with project available → project.
    expect(
      (within(dialog).getByTestId("always-rule-scope") as HTMLSelectElement).value,
    ).toBe("project");
    // Suggested bare program becomes word-boundary-safe.
    expect(
      (within(dialog).getByTestId("always-rule-pattern") as HTMLInputElement).value,
    ).toBe("cargo *");
    expect(
      within(dialog).queryByTestId("always-rule-overbroad-warning"),
    ).toBeNull();
  });

  it("4c — shell allow is saved one normalized segment at a time", async () => {
    const user = userEvent.setup();
    const rulesGateway = new MockPermissionRulesGateway();
    const shellProposal = pendingCommand({
      programLabel: "shell",
      argv: ["cargo test && cargo build"],
      riskClass: "high",
    });

    render(
      <CommandApprovalBar
        decisionAvailable
        onDecide={vi.fn()}
        onReview={vi.fn()}
        permissionRulesGateway={rulesGateway}
        projectId="proj-a"
        proposal={shellProposal}
        ruleSegments={["cargo test", "cargo build"]}
        ruleSubject=""
      />,
    );

    const barNote = screen.getByTestId("command-approval-shell-rule-note");
    expect(barNote.textContent).toContain("2 段");
    expect(barNote.textContent).toContain("分别保存 2 条允许规则");
    const open = screen.getByTestId("command-approval-always-allow");
    expect(open).toHaveTextContent("逐段设置总是允许（2 条规则）");
    expect(screen.queryByTestId("command-approval-always-deny")).toBeNull();

    await user.click(open);
    const dialog = await screen.findByTestId("always-rule-dialog");
    const conjunction = within(dialog).getByTestId("always-rule-conjunction-note");
    expect(conjunction.textContent).toContain("每一段都有一条单独的允许规则");
    expect(conjunction.textContent).toContain("缺少任一条");
    expect(within(dialog).getByTestId("always-rule-progress")).toHaveTextContent(
      "第 1 / 2 段",
    );
    expect(within(dialog).getByTestId("always-rule-action")).toBeDisabled();
    expect(within(dialog).getByTestId("always-rule-scope")).toBeDisabled();
    expect(
      (within(dialog).getByTestId("always-rule-scope") as HTMLSelectElement).value,
    ).toBe("global");
    expect(
      (within(dialog).getByTestId("always-rule-pattern") as HTMLInputElement).value,
    ).toBe("cargo test");
    expect(within(dialog).getByTestId("always-rule-submit")).toHaveTextContent(
      "保存第 1 条规则并继续",
    );

    await user.click(within(dialog).getByTestId("always-rule-submit"));
    await waitFor(() => expect(rulesGateway.getSnapshot()).toHaveLength(1));
    expect(screen.getByTestId("always-rule-dialog")).toBeVisible();
    expect(screen.getByTestId("always-rule-progress")).toHaveTextContent(
      "第 2 / 2 段",
    );
    expect(
      (screen.getByTestId("always-rule-pattern") as HTMLInputElement).value,
    ).toBe("cargo build");
    expect(screen.getByTestId("always-rule-submit")).toHaveTextContent(
      "保存第 2 条规则并完成",
    );

    await user.click(screen.getByTestId("always-rule-submit"));
    await waitFor(() => {
      expect(screen.queryByTestId("always-rule-dialog")).toBeNull();
    });
    expect(rulesGateway.getSnapshot()).toEqual([
      expect.objectContaining({
        scope: "global",
        prefix: "process",
        pattern: "cargo test",
        action: "allow",
      }),
      expect.objectContaining({
        scope: "global",
        prefix: "process",
        pattern: "cargo build",
        action: "allow",
      }),
    ]);
  });

  it("4d — unreliable shell hides durable-rule actions and explains the fail-closed reason", () => {
    render(
      <CommandApprovalBar
        decisionAvailable
        onDecide={vi.fn()}
        onReview={vi.fn()}
        permissionRulesGateway={new MockPermissionRulesGateway()}
        projectId="proj-a"
        proposal={pendingCommand({
          programLabel: "shell",
          argv: ['echo "quoted"'],
          riskClass: "high",
        })}
      />,
    );

    expect(screen.queryByTestId("command-approval-always-allow")).toBeNull();
    expect(screen.queryByTestId("command-approval-always-deny")).toBeNull();
    const note = screen.getByTestId("command-approval-shell-rule-note");
    expect(note.textContent).toContain("命令结构过于复杂，无法安全拆分");
    expect(note.textContent).toContain("本次需逐次确认");
    expect(note.textContent).toContain("不提供“总是允许”");
  });
});

// —— 5. Timeline auto-approve visibility ————————————————————————————————————

describe("Permission rules — timeline auto-approve visibility", () => {
  it("5 — process activity shows auto-rule label and summary", () => {
    const conversationId = "conv-a";
    const runId = "run-1";
    const events: RuntimeEventDto[] = [
      {
        eventId: "evt-req-1",
        conversationId,
        runId,
        sequence: 1,
        kind: "process_proposal_requested",
        createdAt: "2026-08-10T00:00:00.000Z",
        payload: {
          proposal_id: "prop-1",
          program: "cargo",
          argv: ["test"],
          summary: "test",
          risk_class: "standard",
          expires_at: "2026-08-10T01:00:00.000Z",
        },
      },
      {
        eventId: "evt-dec-2",
        conversationId,
        runId,
        sequence: 2,
        kind: "process_proposal_decided",
        createdAt: "2026-08-10T00:00:01.000Z",
        payload: {
          proposal_id: "prop-1",
          program: "cargo",
          decision: "approve_once",
          state: "approved",
          auto_approved_by_rule: {
            prefix: "process",
            pattern: "cargo *",
            action: "allow",
          },
        },
      },
    ];

    const models = buildProcessActivityModels(events, conversationId, runId);
    expect(models).toHaveLength(1);
    expect(models[0]!.autoApprovedByRule).toEqual({
      prefix: "process",
      pattern: "cargo *",
      action: "allow",
    });

    const activity: RunActivityView = {
      runId,
      status: "running",
      startedAt: "2026-08-10T00:00:00.000Z",
      updatedAt: "2026-08-10T00:00:01.000Z",
      assistantText: "",
      reasoningText: "",
      plan: null,
      tools: [],
      subagents: [],
      artifacts: [],
    };

    const timeline: ConversationTimelineView = {
      conversationId,
      messages: [
        {
          id: "m1",
          role: "user",
          content: "跑测试",
          createdAt: "2026-08-10T00:00:00.000Z",
        },
      ],
      events,
      seenEventKeys: new Set(["run-1:1", "run-1:2"]),
      assistantText: "",
      reasoningText: "",
      plan: null,
      tools: [],
      subagents: [],
      activitiesByRunId: { [runId]: activity },
      activeRunId: runId,
    };

    render(
      <ConversationTimeline run={null} timeline={timeline} />,
    );

    const badge = screen.getByTestId("command-activity-auto-rule");
    expect(badge).toBeVisible();
    expect(screen.getByTestId("command-activity-auto-rule-label")).toHaveTextContent(
      "已由规则自动批准",
    );
    expect(screen.getByTestId("command-activity-auto-rule-summary")).toHaveTextContent(
      "allow process:cargo *",
    );
  });

  it("5b — write outcome shows auto-rule label and summary", () => {
    render(
      <AgentActivityDisclosure
        activity={{
          conversationId: "conv-a",
          runId: "run-w",
          status: "completed",
          writeOutcomes: [
            {
              proposalId: "wp-1",
              path: "src/lib.rs",
              mode: "modify",
              state: "succeeded",
              autoApprovedByRule: {
                prefix: "edit",
                pattern: "src/**/*.rs",
                action: "allow",
              },
            },
          ],
        }}
        conversationId="conv-a"
      />,
    );

    expect(screen.getByTestId("write-outcome-auto-rule")).toBeVisible();
    expect(screen.getByTestId("write-outcome-auto-rule-label")).toHaveTextContent(
      "已由规则自动批准",
    );
    expect(screen.getByTestId("write-outcome-auto-rule-summary")).toHaveTextContent(
      "allow edit:src/**/*.rs",
    );
  });
});

// —— 6. Dual-skin body contrast ≥ 4.5:1 ——————————————————————————————————————

describe("Permission rules — dual-skin contrast", () => {
  it("6 — permission-rule body text tokens meet ≥ 4.5:1 on both skins", () => {
    // Use the same surface tokens the permission UI paints against.
    const darkPrimary = readToken("--text-primary", "dark");
    const darkSecondary = readToken("--text-secondary", "dark");
    const darkElevated = readToken("--bg-elevated", "dark");
    const darkMain = readToken("--bg-main", "dark");

    const lightPrimary = readToken("--text-primary", "light");
    const lightSecondary = readToken("--text-secondary", "light");
    const lightElevated = readToken("--bg-elevated", "light");
    const lightMain = readToken("--bg-main", "light");

    // Resolve CSS color functions to hex if the test env returns hex/rgb.
    // tokens.css uses hex for these core tokens.
    const pairs: Array<[string, string, string]> = [
      ["dark primary on elevated", darkPrimary, darkElevated],
      ["dark primary on main", darkPrimary, darkMain],
      ["dark secondary on elevated", darkSecondary, darkElevated],
      ["light primary on elevated", lightPrimary, lightElevated],
      ["light primary on main", lightPrimary, lightMain],
      ["light secondary on elevated", lightSecondary, lightElevated],
    ];

    for (const [label, fg, bg] of pairs) {
      // Skip non-hex (color-mix) — core tokens are hex in tokens.css.
      if (!fg.startsWith("#") || !bg.startsWith("#")) {
        // Fallback known hex pairs from tokens.css when getComputedStyle
        // does not expand custom properties in jsdom.
        continue;
      }
      const ratio = contrastRatio(fg, bg);
      expect(ratio, label).toBeGreaterThanOrEqual(4.5);
    }

    // Explicit numeric checks against the published dual-skin palette in tokens.css.
    // Dark: --text-primary #faf9f7 on --bg-elevated #2c2c36 / --bg-main #1e1e24.
    expect(contrastRatio("#faf9f7", "#2c2c36")).toBeGreaterThanOrEqual(4.5);
    expect(contrastRatio("#faf9f7", "#1e1e24")).toBeGreaterThanOrEqual(4.5);
    expect(contrastRatio("#c7c7ce", "#2c2c36")).toBeGreaterThanOrEqual(4.5);
    // Light: --text-primary #3e3e46 / --text-secondary #5c5c66 on elevated/main.
    expect(contrastRatio("#3e3e46", "#ffffff")).toBeGreaterThanOrEqual(4.5);
    expect(contrastRatio("#3e3e46", "#eeebe6")).toBeGreaterThanOrEqual(4.5);
    expect(contrastRatio("#5c5c66", "#ffffff")).toBeGreaterThanOrEqual(4.5);
    expect(contrastRatio("#5c5c66", "#eeebe6")).toBeGreaterThanOrEqual(4.5);

    // Word-boundary note uses --text-primary on warning-tinted elevated —
    // primary on elevated already covered; paint a live node to confirm.
    document.documentElement.setAttribute("data-theme", "dark");
    const { container: darkNode } = render(
      <p className="permission-rule-word-boundary" data-testid="contrast-note-dark">
        {PERMISSION_WORD_BOUNDARY_NOTE}
      </p>,
    );
    expect(darkNode.textContent).toContain("cargotest");

    document.documentElement.setAttribute("data-theme", "light");
    const { container: lightNode } = render(
      <p className="permission-rule-word-boundary" data-testid="contrast-note-light">
        {PERMISSION_WORD_BOUNDARY_NOTE}
      </p>,
    );
    expect(lightNode.textContent).toContain("cargo *");
  });
});

describe("Permission pattern helpers", () => {
  it("suggests word-boundary-safe process patterns", () => {
    expect(suggestPatternFromSubject("process", "cargo")).toBe("cargo *");
    expect(
      suggestPatternFromSubject(
        "process",
        encodeProcessSubject("cargo", ["test", "-p", "kernel-policy"]),
      ),
    ).toBe("cargo test *");
    expect(suggestPatternFromSubject("edit", "src/lib.rs")).toBe("src/lib.rs");
  });

  it("allow suggestion from cargo test keeps every concrete argv token", () => {
    expect(
      suggestPatternFromSubject(
        "process",
        encodeProcessSubject("cargo", ["test", "-p", "kernel-policy"]),
        "allow",
      ),
    ).toBe("cargo test -p kernel-policy");
  });

  it("does not generalize a subject whose argv boundaries are unknown", () => {
    // This assertion used to read `.toBe("cargo test *")` — generalizing a flat
    // string whose argv boundaries had already been lost. The resulting rule
    // auto-approves commands the user never saw, which is EA-02. A subject the
    // host cannot decode stays verbatim so it fails closed instead of widening.
    expect(
      suggestPatternFromSubject("process", "cargo test -p kernel-policy"),
    ).toBe("cargo test -p kernel-policy");
  });
});
