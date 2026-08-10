/**
 * Wave C4-D1 Task 4 — isolated execution UI primitives.
 * Controlled components only: sanitized DTOs + callbacks; no product mount.
 */
import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CommandActivityRow } from "../src/features/execution/CommandActivityRow";
import { CommandApprovalBar } from "../src/features/execution/CommandApprovalBar";
import { GitChangesView } from "../src/features/execution/GitChangesView";
import { TerminalOutputView } from "../src/features/execution/TerminalOutputView";
import {
  COMMAND_DISPLAY_EXPAND_THRESHOLD,
  formatArgvDisplay,
  formatCommandArgvDisplay,
  isShellProgram,
  SHELL_PROGRAM_LABEL,
  type CommandActivityView,
  type CommandProposalView,
  type TerminalOutputModel,
} from "../src/features/execution/executionTypes";
import type {
  GitDiffView,
  GitRepositoryView,
  GitStatusView,
} from "../src/lib/gitGateway";
import "../src/styles/execution.css";

const here = dirname(fileURLToPath(import.meta.url));
const featureDir = join(here, "../src/features/execution");

const RAW_PROPOSAL_ID = "prop-raw-exec-id-must-not-show";
const RAW_CONVERSATION_ID = "conv-raw-id-must-not-show";
const RAW_RUN_ID = "run-raw-id-must-not-show";
const RAW_GRANT = "abcdefghijklmnopqrstuvwxyz0123456789ABCDEFG";
const NATIVE_PATH = "F:\\projects\\pillow\\crates\\tool-executor";

function pendingProposal(
  overrides: Partial<CommandProposalView> = {},
): CommandProposalView {
  return {
    proposalId: RAW_PROPOSAL_ID,
    conversationId: RAW_CONVERSATION_ID,
    origin: "agent",
    state: "pending_approval",
    programLabel: "cargo",
    argv: ["test", "-p", "keeper-tool-executor"],
    portableCwd: "crates/tool-executor",
    riskClass: "standard",
    summary: "Run focused package tests",
    expiresAt: new Date(Date.now() + 60_000).toISOString(),
    ...overrides,
  };
}

function activityFixture(
  overrides: Partial<CommandActivityView> = {},
): CommandActivityView {
  return {
    origin: "agent",
    state: "succeeded",
    programLabel: "cargo",
    argv: ["test", "-p", "keeper-tool-executor"],
    portableCwd: "crates/tool-executor",
    exitCode: 0,
    durationMs: 1_250,
    outputTail: ["running 1 test", "test result: ok. 1 passed"],
    truncated: false,
    encodingLossy: false,
    ...overrides,
  };
}

function terminalFixture(
  overrides: Partial<TerminalOutputModel> = {},
): TerminalOutputModel {
  return {
    programLabel: "cargo",
    argv: ["test", "-p", "keeper-tool-executor"],
    state: "succeeded",
    chunks: [
      { stream: "stdout", sequence: 0, text: "running 1 test\n" },
      { stream: "stdout", sequence: 1, text: "test result: ok. 1 passed\n" },
    ],
    liveRetained: true,
    truncated: false,
    encodingLossy: false,
    capturedBytes: 48,
    exitCode: 0,
    durationMs: 1_250,
    ...overrides,
  };
}

function gitStatusFixture(
  overrides: Partial<GitStatusView> = {},
): GitStatusView {
  return {
    repositoryId: ".",
    generation: 3,
    branch: "main",
    detached: false,
    headShort: "7b7f70f",
    dirtyCount: 2,
    files: [
      {
        path: "src/features/execution/CommandActivityRow.tsx",
        kind: "modified",
      },
      {
        path: "src/lib/execGateway.ts",
        kind: "added",
      },
    ],
    truncated: false,
    ...overrides,
  };
}

function gitDiffFixture(overrides: Partial<GitDiffView> = {}): GitDiffView {
  return {
    repositoryId: ".",
    generation: 3,
    path: "src/features/execution/CommandActivityRow.tsx",
    kind: "text",
    unified: "@@ -1,2 +1,2 @@\n-const a = 1;\n+const a = 2;\n",
    truncated: false,
    lineCount: 4,
    byteCount: 52,
    ...overrides,
  };
}

function gitRepositoriesFixture(
  overrides: GitRepositoryView[] = [
    { repositoryId: ".", available: true },
    { repositoryId: "nested/lib", available: true },
  ],
): GitRepositoryView[] {
  return overrides;
}

function expectNoSecrets(container: HTMLElement) {
  const text = container.textContent ?? "";
  const html = container.innerHTML;
  expect(text).not.toContain(RAW_PROPOSAL_ID);
  expect(text).not.toContain(RAW_CONVERSATION_ID);
  expect(text).not.toContain(RAW_RUN_ID);
  expect(text).not.toContain(RAW_GRANT);
  expect(text).not.toContain(NATIVE_PATH);
  expect(html).not.toContain(RAW_PROPOSAL_ID);
  expect(html).not.toContain(RAW_CONVERSATION_ID);
  expect(html).not.toContain(RAW_RUN_ID);
  expect(html).not.toContain(RAW_GRANT);
  expect(html).not.toContain(NATIVE_PATH);
  expect(html).not.toMatch(/approvalGrant|data-grant|data-proposal|data-run-id/i);
  expect(text).not.toMatch(/F:\\|C:\\|\\\\server/);
}

afterEach(() => {
  vi.useRealTimers();
});

describe("C4 execution feature boundary", () => {
  it("does not import Tauri, fetch, or persist APIs from feature modules", () => {
    const files = [
      "executionTypes.ts",
      "CommandActivityRow.tsx",
      "CommandApprovalBar.tsx",
      "TerminalOutputView.tsx",
      "GitChangesView.tsx",
    ];
    for (const name of files) {
      const source = readFileSync(join(featureDir, name), "utf8");
      expect(source).not.toMatch(/@tauri-apps/);
      expect(source).not.toMatch(/\bfetch\s*\(/);
      expect(source).not.toMatch(/localStorage|sessionStorage|indexedDB/);
      expect(source).not.toMatch(/invoke\s*\(/);
    }
  });
});

describe("CommandActivityRow", () => {
  it("renders one compact row with origin, status, program and literal argv", () => {
    const { container } = render(
      <CommandActivityRow activity={activityFixture()} />,
    );
    const row = screen.getByTestId("command-activity-row");
    expect(row).toBeInTheDocument();
    expect(within(row).getByTestId("command-activity-origin")).toHaveTextContent(
      "代理",
    );
    expect(within(row).getByTestId("command-activity-status")).toHaveTextContent(
      "已成功",
    );
    expect(within(row).getByTestId("command-activity-program")).toHaveTextContent(
      "cargo",
    );
    expect(within(row).getByTestId("command-activity-argv")).toHaveTextContent(
      '"test" "-p" "keeper-tool-executor"',
    );
    expect(within(row).getByTestId("command-activity-exit")).toHaveTextContent(
      "0",
    );
    expect(within(row).getByTestId("command-activity-duration")).toHaveTextContent(
      "1250",
    );
    expectNoSecrets(container);
  });

  it("shows user origin chip and progressive disclosure for output tail", async () => {
    const user = userEvent.setup();
    render(
      <CommandActivityRow
        activity={activityFixture({
          origin: "user",
          state: "failed",
          exitCode: 1,
        })}
      />,
    );
    expect(screen.getByTestId("command-activity-origin")).toHaveTextContent(
      "用户",
    );
    expect(screen.getByTestId("command-activity-status")).toHaveTextContent(
      "已失败",
    );
    const details = screen.getByTestId("command-activity-details");
    expect(details).not.toHaveAttribute("open");
    await user.click(screen.getByTestId("command-activity-summary"));
    expect(details).toHaveAttribute("open");
    expect(screen.getByTestId("command-activity-tail")).toHaveTextContent(
      "test result: ok. 1 passed",
    );
  });

  it("invokes open-output callback without exposing raw identifiers", async () => {
    const user = userEvent.setup();
    const onOpenOutput = vi.fn();
    const { container } = render(
      <CommandActivityRow
        activity={activityFixture({ state: "running", exitCode: null })}
        onOpenOutput={onOpenOutput}
      />,
    );
    await user.click(screen.getByTestId("command-activity-open-output"));
    expect(onOpenOutput).toHaveBeenCalledTimes(1);
    expectNoSecrets(container);
  });
});

describe("CommandApprovalBar", () => {
  function renderBar(
    props: Partial<{
      proposal: CommandProposalView;
      decisionAvailable: boolean;
      disabled: boolean;
      onDecide: ReturnType<typeof vi.fn>;
      onReview: ReturnType<typeof vi.fn>;
      onExpiryDeadline: ReturnType<typeof vi.fn>;
    }> = {},
  ) {
    const onDecide = props.onDecide ?? vi.fn().mockResolvedValue(undefined);
    const onReview = props.onReview ?? vi.fn();
    const onExpiryDeadline = props.onExpiryDeadline ?? vi.fn();
    const proposal = props.proposal ?? pendingProposal();
    const view = render(
      <CommandApprovalBar
        decisionAvailable={
          props.decisionAvailable === undefined ? true : props.decisionAvailable
        }
        disabled={props.disabled}
        onDecide={onDecide}
        onExpiryDeadline={onExpiryDeadline}
        onReview={onReview}
        proposal={proposal}
      />,
    );
    return { ...view, onDecide, onReview, onExpiryDeadline, proposal };
  }

  it("shows program, literal argv, portable cwd and risk without grant or ids", () => {
    const { container } = renderBar({
      proposal: pendingProposal({
        riskClass: "high",
        argv: ["-e", "console.log(1)"],
        programLabel: "node",
        portableCwd: ".",
      }),
    });
    const bar = screen.getByTestId("command-approval-bar");
    expect(within(bar).getByTestId("command-approval-program")).toHaveTextContent(
      "node",
    );
    expect(within(bar).getByTestId("command-approval-argv")).toHaveTextContent(
      '"-e" "console.log(1)"',
    );
    expect(within(bar).getByTestId("command-approval-cwd")).toHaveTextContent(
      ".",
    );
    expect(within(bar).getByTestId("command-approval-risk")).toHaveTextContent(
      "高风险",
    );
    expect(screen.getByTestId("command-approval-approve")).toHaveTextContent(
      "批准一次",
    );
    expect(screen.getByTestId("command-approval-reject")).toHaveTextContent(
      "拒绝",
    );
    expect(screen.getByTestId("command-approval-review")).toHaveTextContent(
      "查看命令",
    );
    expectNoSecrets(container);
  });

  it("focuses Approve once initially and Tab cycles only inside actions", async () => {
    const user = userEvent.setup();
    renderBar();
    const approve = screen.getByTestId("command-approval-approve");
    const reject = screen.getByTestId("command-approval-reject");
    const review = screen.getByTestId("command-approval-review");
    expect(approve).toHaveFocus();

    await user.tab();
    expect(reject).toHaveFocus();
    await user.tab();
    expect(review).toHaveFocus();
    await user.tab();
    expect(approve).toHaveFocus();
    await user.tab({ shift: true });
    expect(review).toHaveFocus();
  });

  it("Escape rejects only while the bar owns focus", async () => {
    const user = userEvent.setup();
    const onDecide = vi.fn().mockResolvedValue(undefined);
    renderBar({ onDecide });
    expect(screen.getByTestId("command-approval-approve")).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(onDecide).toHaveBeenCalledWith("reject");
  });

  it("Escape does nothing when focus is outside the bar", async () => {
    const user = userEvent.setup();
    const onDecide = vi.fn().mockResolvedValue(undefined);
    render(
      <div>
        <button data-testid="outside" type="button">
          outside
        </button>
        <CommandApprovalBar
          decisionAvailable={true}
          onDecide={onDecide}
          onReview={vi.fn()}
          proposal={pendingProposal()}
        />
      </div>,
    );
    screen.getByTestId("outside").focus();
    await user.keyboard("{Escape}");
    expect(onDecide).not.toHaveBeenCalled();
  });

  it("fires approve-once, reject and review callbacks without double-fire", async () => {
    const user = userEvent.setup();
    let resolveDecide: (() => void) | undefined;
    const onDecide = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveDecide = resolve;
        }),
    );
    const onReview = vi.fn();
    renderBar({ onDecide, onReview });

    await user.click(screen.getByTestId("command-approval-approve"));
    expect(onDecide).toHaveBeenCalledWith("approve_once");
    expect(onDecide).toHaveBeenCalledTimes(1);
    expect(screen.getByTestId("command-approval-approve")).toBeDisabled();
    expect(screen.getByTestId("command-approval-reject")).toBeDisabled();
    await user.click(screen.getByTestId("command-approval-reject"));
    expect(onDecide).toHaveBeenCalledTimes(1);

    await act(async () => {
      resolveDecide?.();
    });
  });

  it("enables decisions only when decisionAvailable is true; Review stays independent of that flag", async () => {
    const user = userEvent.setup();
    const onReview = vi.fn();
    const { container, rerender, onDecide } = renderBar({
      decisionAvailable: false,
      onReview,
    });

    expect(screen.getByTestId("command-approval-approve")).toBeDisabled();
    expect(screen.getByTestId("command-approval-reject")).toBeDisabled();
    const review = screen.getByTestId("command-approval-review");
    expect(review).not.toBeDisabled();
    await user.click(review);
    expect(onReview).toHaveBeenCalledTimes(1);
    expect(onDecide).not.toHaveBeenCalled();
    expectNoSecrets(container);

    // Sanitized boolean true unlocks Approve/Reject without any raw grant prop.
    rerender(
      <CommandApprovalBar
        decisionAvailable={true}
        onDecide={onDecide}
        onReview={onReview}
        proposal={pendingProposal()}
      />,
    );
    expect(screen.getByTestId("command-approval-approve")).not.toBeDisabled();
    expect(screen.getByTestId("command-approval-reject")).not.toBeDisabled();
    expect(screen.getByTestId("command-approval-review")).not.toBeDisabled();
    expectNoSecrets(container);
  });

  it("Review remains governed by disabled/expiry rules independent of decisionAvailable", () => {
    const { container } = renderBar({
      decisionAvailable: true,
      disabled: true,
    });
    expect(screen.getByTestId("command-approval-approve")).toBeDisabled();
    expect(screen.getByTestId("command-approval-reject")).toBeDisabled();
    expect(screen.getByTestId("command-approval-review")).toBeDisabled();
    expectNoSecrets(container);
  });
});

describe("TerminalOutputView", () => {
  it("renders bounded stream text with program and literal argv", () => {
    const { container } = render(
      <TerminalOutputView output={terminalFixture()} />,
    );
    expect(screen.getByTestId("terminal-output-view")).toBeInTheDocument();
    expect(screen.getByTestId("terminal-output-program")).toHaveTextContent(
      "cargo",
    );
    expect(screen.getByTestId("terminal-output-argv")).toHaveTextContent(
      '"test" "-p" "keeper-tool-executor"',
    );
    expect(screen.getByTestId("terminal-output-body")).toHaveTextContent(
      "running 1 test",
    );
    expect(screen.getByTestId("terminal-output-body")).toHaveTextContent(
      "test result: ok. 1 passed",
    );
    expect(screen.queryByTestId("terminal-output-truncated")).toBeNull();
    expect(screen.queryByTestId("terminal-output-lossy")).toBeNull();
    expectNoSecrets(container);
  });

  it("shows truncation and lossy encoding labels from host flags", () => {
    const { container } = render(
      <TerminalOutputView
        output={terminalFixture({
          truncated: true,
          encodingLossy: true,
          capturedBytes: 262_144,
          chunks: [
            {
              stream: "stderr",
              sequence: 0,
              text: "…truncated tail…\n",
            },
          ],
        })}
      />,
    );
    expect(screen.getByTestId("terminal-output-truncated")).toHaveTextContent(
      "输出已截断",
    );
    expect(screen.getByTestId("terminal-output-lossy")).toHaveTextContent(
      "编码有损",
    );
    expect(screen.getByTestId("terminal-output-bytes")).toHaveTextContent(
      "262144",
    );
    expectNoSecrets(container);
  });

  it("does not expose native paths, grants or raw ids in capped output", () => {
    const { container } = render(
      <TerminalOutputView
        output={terminalFixture({
          chunks: [
            {
              stream: "stdout",
              sequence: 0,
              text: "safe portable line\n",
            },
          ],
        })}
      />,
    );
    expectNoSecrets(container);
  });

  it("shows durable-only notice when liveRetained is false and never invents transcript", () => {
    const { container } = render(
      <TerminalOutputView
        output={terminalFixture({
          liveRetained: false,
          chunks: [],
          capturedBytes: 48,
          exitCode: 0,
          durationMs: 1_250,
        })}
      />,
    );
    expect(screen.getByTestId("terminal-output-durable-only")).toHaveTextContent(
      "实时输出未保留",
    );
    expect(screen.getByTestId("terminal-output-view")).toHaveAttribute(
      "data-live-retained",
      "false",
    );
    // Metadata may remain; body must stay host-chunk-only (empty here).
    expect(screen.getByTestId("terminal-output-body")).toHaveTextContent("");
    expect(screen.getByTestId("terminal-output-bytes")).toHaveTextContent("48");
    expectNoSecrets(container);
  });

  it("omits durable-only notice when live output is retained", () => {
    render(<TerminalOutputView output={terminalFixture({ liveRetained: true })} />);
    expect(screen.queryByTestId("terminal-output-durable-only")).toBeNull();
    expect(screen.getByTestId("terminal-output-view")).toHaveAttribute(
      "data-live-retained",
      "true",
    );
  });
});

describe("GitChangesView", () => {
  it("renders branch identity, portable paths and dirty count", () => {
    const { container } = render(
      <GitChangesView status={gitStatusFixture()} />,
    );
    expect(screen.getByTestId("git-changes-view")).toBeInTheDocument();
    expect(screen.getByTestId("git-changes-branch")).toHaveTextContent("main");
    expect(screen.getByTestId("git-changes-head")).toHaveTextContent("7b7f70f");
    expect(screen.getByTestId("git-changes-dirty-count")).toHaveTextContent("2");
    expect(screen.getByTestId("git-changes-file-list")).toHaveTextContent(
      "src/features/execution/CommandActivityRow.tsx",
    );
    expectNoSecrets(container);
  });

  it("shows detached HEAD without inventing a branch name", () => {
    render(
      <GitChangesView
        status={gitStatusFixture({
          branch: null,
          detached: true,
          headShort: "abc1234",
          dirtyCount: 0,
          files: [],
        })}
      />,
    );
    expect(screen.getByTestId("git-changes-branch")).toHaveTextContent(
      "分离 HEAD",
    );
    expect(screen.getByTestId("git-changes-head")).toHaveTextContent("abc1234");
    expect(screen.getByTestId("git-changes-clean")).toHaveTextContent(
      "工作区干净",
    );
  });

  it("shows rename as one semantic entry with original and new portable paths", () => {
    render(
      <GitChangesView
        status={gitStatusFixture({
          dirtyCount: 1,
          files: [
            {
              path: "src/renamed.ts",
              kind: "renamed",
              originalPath: "src/old-name.ts",
            },
          ],
        })}
      />,
    );
    const entry = screen.getByTestId("git-changes-entry-0");
    expect(entry).toHaveTextContent("src/renamed.ts");
    expect(entry).toHaveTextContent("src/old-name.ts");
    expect(within(entry).getByTestId("git-changes-kind")).toHaveTextContent(
      "重命名",
    );
  });

  it("shows binary and unsupported diffs without inventing unified text", () => {
    const { rerender } = render(
      <GitChangesView
        status={gitStatusFixture({
          dirtyCount: 1,
          files: [{ path: "assets/icon.png", kind: "modified" }],
        })}
        diff={gitDiffFixture({
          path: "assets/icon.png",
          kind: "binary",
          unified: undefined,
          lineCount: undefined,
          byteCount: undefined,
        })}
      />,
    );
    expect(screen.getByTestId("git-changes-diff-binary")).toHaveTextContent(
      "二进制文件",
    );
    expect(screen.queryByTestId("git-changes-diff-unified")).toBeNull();
    // Status entries must not invent isBinary badges from wire fields.
    expect(screen.queryByText("二进制")).toBeNull();

    rerender(
      <GitChangesView
        status={gitStatusFixture({
          dirtyCount: 1,
          files: [{ path: "huge.bin", kind: "modified" }],
        })}
        diff={gitDiffFixture({
          path: "huge.bin",
          kind: "unsupported",
          unified: undefined,
          lineCount: undefined,
          byteCount: undefined,
        })}
      />,
    );
    expect(screen.getByTestId("git-changes-diff-unsupported")).toHaveTextContent(
      "不支持的差异类型",
    );
    expect(screen.queryByTestId("git-changes-diff-unified")).toBeNull();
  });

  it("surfaces UI-only stale/error/loading with refresh and path callbacks only", async () => {
    const user = userEvent.setup();
    const onRefresh = vi.fn();
    const onSelectPath = vi.fn();
    const { container } = render(
      <GitChangesView
        status={gitStatusFixture()}
        stale
        error="Git result is stale; refresh changes."
        onRefresh={onRefresh}
        onSelectPath={onSelectPath}
      />,
    );
    expect(screen.getByTestId("git-changes-stale")).toHaveTextContent(
      "状态已过期",
    );
    expect(screen.getByTestId("git-changes-error")).toHaveTextContent(
      "Git result is stale",
    );
    await user.click(screen.getByTestId("git-changes-refresh"));
    expect(onRefresh).toHaveBeenCalledTimes(1);
    await user.click(screen.getByTestId("git-changes-entry-0"));
    expect(onSelectPath).toHaveBeenCalledWith(
      "src/features/execution/CommandActivityRow.tsx",
    );
    expectNoSecrets(container);

    render(
      <GitChangesView status={null} loading onRefresh={onRefresh} />,
    );
    expect(screen.getByTestId("git-changes-loading")).toHaveTextContent(
      "加载变更",
    );
  });

  it("shows text diff truncation notice when host marks truncated", () => {
    const { container } = render(
      <GitChangesView
        status={gitStatusFixture()}
        diff={gitDiffFixture({ truncated: true, lineCount: 4000 })}
      />,
    );
    expect(screen.getByTestId("git-changes-diff-truncated")).toHaveTextContent(
      "差异已截断",
    );
    expect(screen.getByTestId("git-changes-diff-unified")).toHaveTextContent(
      "const a = 2",
    );
    expectNoSecrets(container);
  });

  it("shows repository selector only for multiple repos; unavailable not selectable", async () => {
    const user = userEvent.setup();
    const onSelectRepository = vi.fn();
    const { rerender, container } = render(
      <GitChangesView
        repositories={gitRepositoriesFixture([
          { repositoryId: ".", available: true },
          { repositoryId: "nested/lib", available: false },
        ])}
        selectedRepositoryId="."
        status={gitStatusFixture()}
        onSelectRepository={onSelectRepository}
      />,
    );
    expect(screen.getByTestId("git-changes-repo-selector")).toBeInTheDocument();
    expect(screen.getByTestId("git-changes-repo-root")).toBeEnabled();
    expect(screen.getByTestId("git-changes-repo-nested/lib")).toBeDisabled();
    expect(screen.getByTestId("git-changes-repo-nested/lib")).toHaveTextContent(
      "不可用",
    );
    await user.click(screen.getByTestId("git-changes-repo-nested/lib"));
    expect(onSelectRepository).not.toHaveBeenCalled();
    await user.click(screen.getByTestId("git-changes-repo-root"));
    expect(onSelectRepository).toHaveBeenCalledWith(".");
    expectNoSecrets(container);

    // Single repository: no selector chrome.
    rerender(
      <GitChangesView
        repositories={[{ repositoryId: ".", available: true }]}
        selectedRepositoryId="."
        status={gitStatusFixture()}
        onSelectRepository={onSelectRepository}
      />,
    );
    expect(screen.queryByTestId("git-changes-repo-selector")).toBeNull();
  });

  it("does not declare isBinary/stale/unavailable on gateway wire types in source", () => {
    const viewSource = readFileSync(
      join(featureDir, "GitChangesView.tsx"),
      "utf8",
    );
    const typesSource = readFileSync(
      join(featureDir, "executionTypes.ts"),
      "utf8",
    );
    expect(typesSource).not.toMatch(/export type GitStatusView/);
    expect(typesSource).not.toMatch(/export type GitDiffView/);
    expect(typesSource).not.toMatch(/isBinary/);
    expect(viewSource).toMatch(/from ["']\.\.\/\.\.\/lib\/gitGateway["']/);
    expect(viewSource).not.toMatch(/isBinary/);
    expect(viewSource).not.toMatch(/unavailableReason/);
    expect(viewSource).not.toMatch(/status\.stale|diff\.stale/);
  });
});

/**
 * Shell vs structured command display (approval + activity surfaces).
 * Hard rule: shell display text must be character-identical to the sealed
 * command string — no JSON escape, no whitespace normalize.
 */
describe("shell command approval display", () => {
  /**
   * Nested quotes — the sealed command as the host stores it (single argv
   * element). Under formatArgvDisplay this becomes a JSON-escaped blob;
   * shell display must keep this string character-identical.
   */
  const NESTED_QUOTE_CMD =
    `node -e "require('fs').writeFileSync('out.txt', 'hello \"world\"')"`;

  function shellProposal(
    command: string,
    overrides: Partial<CommandProposalView> = {},
  ): CommandProposalView {
    return pendingProposal({
      programLabel: SHELL_PROGRAM_LABEL,
      argv: [command],
      riskClass: "high",
      summary: "governed shell",
      ...overrides,
    });
  }

  it("formatCommandArgvDisplay: shell is character-identical; structured stays JSON-encoded", () => {
    expect(isShellProgram(SHELL_PROGRAM_LABEL)).toBe(true);
    expect(isShellProgram("cargo")).toBe(false);

    // Positive accept: rendered text === sealed command, char by char.
    expect(formatCommandArgvDisplay("shell", [NESTED_QUOTE_CMD])).toBe(
      NESTED_QUOTE_CMD,
    );
    expect(formatCommandArgvDisplay("shell", [NESTED_QUOTE_CMD])).not.toBe(
      formatArgvDisplay([NESTED_QUOTE_CMD]),
    );
    // Empty shell argv → empty string (not "shell", not '""').
    expect(formatCommandArgvDisplay("shell", [])).toBe("");
    // Multi-element shell is out of host contract; still use argv[0] only
    // (no rejoin that would invent a different command string).
    expect(formatCommandArgvDisplay("shell", ["echo a", "echo b"])).toBe(
      "echo a",
    );

    // Structured regression: same encoding as formatArgvDisplay.
    const structured = ["test", "-p", "keeper-tool-executor"];
    expect(formatCommandArgvDisplay("cargo", structured)).toBe(
      formatArgvDisplay(structured),
    );
    expect(formatCommandArgvDisplay("cargo", structured)).toBe(
      '"test" "-p" "keeper-tool-executor"',
    );
    expect(formatCommandArgvDisplay("node", ["-e", 'say "hi"'])).toBe(
      '"-e" "say \\"hi\\""',
    );
  });

  it("CommandApprovalBar: shell nested-quote command textContent equals input char-for-char", () => {
    render(
      <CommandApprovalBar
        decisionAvailable={true}
        onDecide={vi.fn()}
        onReview={vi.fn()}
        proposal={shellProposal(NESTED_QUOTE_CMD)}
      />,
    );

    const bar = screen.getByTestId("command-approval-bar");
    expect(bar).toHaveAttribute("data-command-kind", "shell");
    expect(within(bar).getByTestId("command-approval-kind")).toHaveTextContent(
      "Shell 命令",
    );
    // No separate program chip for shell — label is "Shell 命令".
    expect(within(bar).queryByTestId("command-approval-program")).toBeNull();

    const argvEl = within(bar).getByTestId("command-approval-argv");
    // Character-identical (not toHaveTextContent, which normalizes whitespace).
    expect(argvEl.textContent).toBe(NESTED_QUOTE_CMD);
    expect(argvEl.getAttribute("data-full-command")).toBe(NESTED_QUOTE_CMD);
    expect(argvEl.getAttribute("title")).toBe(NESTED_QUOTE_CMD);
    // Must not be the JSON-escaped form users previously saw.
    const escaped = formatArgvDisplay([NESTED_QUOTE_CMD]);
    expect(escaped).not.toBe(NESTED_QUOTE_CMD);
    expect(argvEl.textContent).not.toBe(escaped);
  });

  it("CommandApprovalBar: structured proposal presentation is unchanged", () => {
    render(
      <CommandApprovalBar
        decisionAvailable={true}
        onDecide={vi.fn()}
        onReview={vi.fn()}
        proposal={pendingProposal({
          programLabel: "node",
          argv: ["-e", "console.log(1)"],
          riskClass: "high",
        })}
      />,
    );

    const bar = screen.getByTestId("command-approval-bar");
    expect(bar).toHaveAttribute("data-command-kind", "structured");
    expect(within(bar).queryByTestId("command-approval-kind")).toBeNull();
    expect(within(bar).getByTestId("command-approval-program")).toHaveTextContent(
      "node",
    );
    const argvEl = within(bar).getByTestId("command-approval-argv");
    expect(argvEl.textContent).toBe('"-e" "console.log(1)"');
    expect(argvEl.getAttribute("data-full-command")).toBe(
      '"-e" "console.log(1)"',
    );
  });

  it("CommandApprovalBar: long shell command keeps full text available after visual truncate affordance", async () => {
    const user = userEvent.setup();
    const longCmd =
      "node -e " +
      JSON.stringify(
        "console.log(" + "x".repeat(COMMAND_DISPLAY_EXPAND_THRESHOLD) + ")",
      );
    expect(longCmd.length).toBeGreaterThan(COMMAND_DISPLAY_EXPAND_THRESHOLD);

    render(
      <CommandApprovalBar
        decisionAvailable={true}
        onDecide={vi.fn()}
        onReview={vi.fn()}
        proposal={shellProposal(longCmd)}
      />,
    );

    const argvEl = screen.getByTestId("command-approval-argv");
    // Full command is always in the DOM / title / data attr — never only a
    // truncated copy that the user could approve without access to the rest.
    expect(argvEl.textContent).toBe(longCmd);
    expect(argvEl.getAttribute("data-full-command")).toBe(longCmd);
    expect(argvEl.getAttribute("title")).toBe(longCmd);

    const expand = screen.getByTestId("command-approval-expand-command");
    expect(expand).toHaveTextContent("展开完整命令");
    await user.click(expand);
    expect(expand).toHaveTextContent("收起");
    expect(argvEl).toHaveClass("command-approval-bar__command-text--expanded");
    // Still character-identical after expand.
    expect(argvEl.textContent).toBe(longCmd);
    expect(argvEl.getAttribute("data-full-command")).toBe(longCmd);
  });

  it("same suite: shell and structured both render correctly side by side", () => {
    const shellCmd = `echo 'a "b" c' && printf "%s" 'nested'`;
    const structuredArgv = ["test", "-p", "pillow-desktop"] as const;

    const { rerender } = render(
      <CommandApprovalBar
        decisionAvailable={true}
        onDecide={vi.fn()}
        onReview={vi.fn()}
        proposal={shellProposal(shellCmd)}
      />,
    );
    expect(screen.getByTestId("command-approval-bar")).toHaveAttribute(
      "data-command-kind",
      "shell",
    );
    expect(screen.getByTestId("command-approval-argv").textContent).toBe(
      shellCmd,
    );

    rerender(
      <CommandApprovalBar
        decisionAvailable={true}
        onDecide={vi.fn()}
        onReview={vi.fn()}
        proposal={pendingProposal({
          programLabel: "cargo",
          argv: [...structuredArgv],
        })}
      />,
    );
    expect(screen.getByTestId("command-approval-bar")).toHaveAttribute(
      "data-command-kind",
      "structured",
    );
    expect(screen.getByTestId("command-approval-program")).toHaveTextContent(
      "cargo",
    );
    expect(screen.getByTestId("command-approval-argv").textContent).toBe(
      '"test" "-p" "pillow-desktop"',
    );
  });

  it("CommandActivityRow: shell command is not JSON-escaped", () => {
    const cmd = `python -c "print('hi \"x\"')"`;
    const { container } = render(
      <CommandActivityRow
        activity={activityFixture({
          programLabel: SHELL_PROGRAM_LABEL,
          argv: [cmd],
          state: "pending_approval",
        })}
      />,
    );
    const row = screen.getByTestId("command-activity-row");
    expect(row).toHaveAttribute("data-command-kind", "shell");
    expect(within(row).getByTestId("command-activity-kind")).toHaveTextContent(
      "Shell 命令",
    );
    expect(within(row).queryByTestId("command-activity-program")).toBeNull();
    const rowArgv = within(row).getByTestId("command-activity-argv");
    expect(rowArgv.textContent).toBe(cmd);
    expect(rowArgv.getAttribute("data-full-command")).toBe(cmd);
    expect(rowArgv.textContent).not.toBe(formatArgvDisplay([cmd]));
    // No native path / grant leakage from this fixture.
    expect(container.textContent).not.toContain(RAW_GRANT);
  });

  it("TerminalOutputView: shell command is not JSON-escaped", () => {
    const cmd = `python -c "print('hi \"x\"')"`;
    render(
      <TerminalOutputView
        output={terminalFixture({
          programLabel: SHELL_PROGRAM_LABEL,
          argv: [cmd],
        })}
      />,
    );
    const term = screen.getByTestId("terminal-output-view");
    expect(term).toHaveAttribute("data-command-kind", "shell");
    expect(within(term).getByTestId("terminal-output-kind")).toHaveTextContent(
      "Shell 命令",
    );
    expect(within(term).queryByTestId("terminal-output-program")).toBeNull();
    expect(within(term).getByTestId("terminal-output-argv").textContent).toBe(
      cmd,
    );
  });
});

/**
 * Display-only encoding for sealed argv. Must preserve argument boundaries
 * without shell parsing. Encoding: JSON-string per argument, space-separated.
 */
describe("formatArgvDisplay", () => {
  it("encodes each sealed argument as a JSON string joined by spaces", () => {
    expect(formatArgvDisplay(["test", "-p", "keeper-tool-executor"])).toBe(
      '"test" "-p" "keeper-tool-executor"',
    );
    expect(formatArgvDisplay([])).toBe("");
  });

  it("preserves empty strings, quotes, backslashes, whitespace and Unicode", () => {
    expect(formatArgvDisplay([""])).toBe('""');
    expect(formatArgvDisplay(["", "x"])).toBe('"" "x"');
    expect(formatArgvDisplay(['say "hi"'])).toBe('"say \\"hi\\""');
    expect(formatArgvDisplay(["a\\b", "c"])).toBe('"a\\\\b" "c"');
    expect(formatArgvDisplay(["  "])).toBe('"  "');
    expect(formatArgvDisplay(["a b"])).toBe('"a b"');
    expect(formatArgvDisplay(["你好", "世界"])).toBe('"你好" "世界"');
  });

  it("renders different argv arrays with different display text for boundary cases", () => {
    // Pairs that collide under argv.join(" ") must remain distinct for approval/review.
    const joinCollisions: Array<[readonly string[], readonly string[]]> = [
      [
        ["a b", "c"],
        ["a", "b", "c"],
      ],
      [
        ["-e", "console.log(1)"],
        ["-e console.log(1)"],
      ],
      [
        ["path with space", "file.txt"],
        ["path", "with", "space", "file.txt"],
      ],
    ];
    for (const [left, right] of joinCollisions) {
      expect(left.join(" ")).toBe(right.join(" "));
      expect(formatArgvDisplay(left)).not.toBe(formatArgvDisplay(right));
    }

    // Additional sealed-boundary pairs (may or may not collide under join).
    const more: Array<[readonly string[], readonly string[]]> = [
      [["a", ""], ["a"]],
      [["", ""], [""]],
      [['a"b'], ["a", "b"]],
      [["a\\b"], ["a", "b"]],
      [["a", "b c"], ["a b", "c"]],
    ];
    for (const [left, right] of more) {
      expect(formatArgvDisplay(left)).not.toBe(formatArgvDisplay(right));
    }
  });

  it("is deterministic and never introduces shell metacharacter interpretation", () => {
    const argv = ["$HOME", "$(id)", "`uname`", "a|b", "c&d", ";rm", ">", "<"];
    const first = formatArgvDisplay(argv);
    const second = formatArgvDisplay(argv);
    expect(first).toBe(second);
    expect(first).toBe(
      '"$HOME" "$(id)" "`uname`" "a|b" "c&d" ";rm" ">" "<"',
    );
    // Display only: structured helper body must stay JSON-stringify join —
    // no exec/spawn. Shell-aware path is formatCommandArgvDisplay (suite above).
    const source = readFileSync(join(featureDir, "executionTypes.ts"), "utf8");
    const start = source.indexOf("export function formatArgvDisplay");
    expect(start).toBeGreaterThanOrEqual(0);
    const end = source.indexOf("\n}", start);
    expect(end).toBeGreaterThan(start);
    const helper = source.slice(start, end + 2);
    expect(helper).toMatch(/^export function formatArgvDisplay[\s\S]*JSON\.stringify[\s\S]*\}$/);
    expect(helper).not.toMatch(/exec\b|spawn\b|cmd\.exe|powershell/i);
    expect(helper).not.toMatch(/isShellProgram|formatCommandArgvDisplay/);
  });
});
