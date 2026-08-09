/**
 * Wave C2-C2 — in-conversation agent activity disclosure (renderer slice).
 * Consumes conversation-owned activity only; no global task polling.
 */
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { AgentActivityDisclosure } from "../src/features/conversations/AgentActivityDisclosure";
import type { AgentActivityRecord } from "../src/features/conversations/agentActivityTypes";

const conversationA = "conv-aaa-1111-2222-3333";
const conversationB = "conv-bbb-4444-5555-6666";
const sessionId = "session-raw-id-should-never-render-xyz";

function baseRecord(
  overrides: Partial<AgentActivityRecord> = {},
): AgentActivityRecord {
  return {
    conversationId: conversationA,
    role: "实现者",
    status: "running",
    currentStep: "读取源文件",
    steps: [
      { title: "分析需求", status: "completed" },
      { title: "读取源文件", status: "active" },
      { title: "应用补丁", status: "pending" },
    ],
    tools: [
      {
        title: "fs.read",
        status: "completed",
        output: "fn main() {}",
      },
      {
        title: "fs.search",
        status: "running",
      },
    ],
    subagents: [
      {
        title: "审查者",
        role: "审查者",
        status: "running",
        progress: "检查边界条件",
      },
    ],
    artifacts: [{ path: "src/main.rs", kind: "file", label: "main.rs" }],
    sessionId,
    runId: "run-raw-id-do-not-show",
    ...overrides,
  };
}

function renderDisclosure(
  props: Partial<{
    conversationId: string;
    activity: AgentActivityRecord | readonly AgentActivityRecord[] | null;
    onOpenArtifact: (relativePath: string) => void;
  }> = {},
) {
  const onOpenArtifact = props.onOpenArtifact ?? vi.fn();
  const conversationId = props.conversationId ?? conversationA;
  const activity =
    props.activity === undefined ? baseRecord() : props.activity;
  const view = render(
    <AgentActivityDisclosure
      conversationId={conversationId}
      activity={activity}
      onOpenArtifact={onOpenArtifact}
    />,
  );
  return { ...view, onOpenArtifact, conversationId };
}

describe("AgentActivityDisclosure (C2-C2)", () => {
  it("renders only activity owned by the supplied conversationId", () => {
    const foreign = baseRecord({
      conversationId: conversationB,
      role: "外来代理",
      currentStep: "不应出现",
      tools: [{ title: "foreign.tool", status: "running" }],
    });
    const owned = baseRecord({
      role: "实现者",
      currentStep: "本会话步骤",
    });

    renderDisclosure({
      conversationId: conversationA,
      activity: [foreign, owned],
    });

    expect(screen.getByTestId("agent-activity-disclosure")).toBeInTheDocument();
    expect(screen.getByText(/本会话步骤/)).toBeInTheDocument();
    expect(screen.queryByText(/外来代理/)).toBeNull();
    expect(screen.queryByText(/不应出现/)).toBeNull();
    expect(screen.queryByText(/foreign\.tool/)).toBeNull();
  });

  it("renders nothing when inputs are missing or empty (no invented work)", () => {
    const { container: emptyContainer } = renderDisclosure({ activity: null });
    expect(emptyContainer).toBeEmptyDOMElement();

    const { container: undefinedish } = render(
      <AgentActivityDisclosure conversationId={conversationA} activity={[]} />,
    );
    expect(undefinedish).toBeEmptyDOMElement();

    const { container: foreignOnly } = renderDisclosure({
      activity: [
        baseRecord({
          conversationId: conversationB,
          tools: [{ title: "ghost", status: "running" }],
        }),
      ],
    });
    expect(foreignOnly).toBeEmptyDOMElement();
  });

  it("shows a compact human-readable summary without raw IDs", () => {
    renderDisclosure({
      activity: baseRecord({
        role: "实现者",
        status: "running",
        currentStep: "读取源文件",
        sessionId,
        runId: "run-deadbeef",
      }),
    });

    const summary = screen.getByTestId("agent-activity-summary");
    expect(summary).toHaveTextContent(/实现者/);
    expect(summary).toHaveTextContent(/运行中|进行中/);
    expect(summary).toHaveTextContent(/读取源文件/);
    expect(summary).not.toHaveTextContent(sessionId);
    expect(summary).not.toHaveTextContent("run-deadbeef");
    expect(summary).not.toHaveTextContent("session-");
    expect(document.body.textContent ?? "").not.toContain(sessionId);
    expect(document.body.textContent ?? "").not.toContain("run-deadbeef");
  });

  it("keeps details closed by default and expands to ordered steps", async () => {
    const user = userEvent.setup();
    renderDisclosure();

    const details = screen.getByTestId("agent-activity-details");
    expect(details).not.toHaveAttribute("open");
    // Nested progressive sections start closed (native details, not forced open).
    const nestedOpen = details.querySelectorAll("details[open]");
    expect(nestedOpen.length).toBe(0);

    await user.click(screen.getByTestId("agent-activity-summary"));

    expect(details).toHaveAttribute("open");
    const steps = screen.getByTestId("agent-activity-steps");
    const stepItems = within(steps).getAllByRole("listitem");
    expect(stepItems.map((el) => el.textContent ?? "").join("\n")).toMatch(
      /分析需求[\s\S]*读取源文件[\s\S]*应用补丁/,
    );

    const tools = screen.getByTestId("agent-activity-tools");
    const toolItems = within(tools).getAllByRole("listitem");
    expect(toolItems[0]).toHaveTextContent(/fs\.read/);
    expect(toolItems[1]).toHaveTextContent(/fs\.search/);
    expect(toolItems[0]).toHaveTextContent("工具已返回输出。");
    expect(toolItems[0]).not.toHaveTextContent("fn main() {}");

    const subagents = screen.getByTestId("agent-activity-subagents");
    expect(within(subagents).getByText(/审查者/)).toBeInTheDocument();
    expect(within(subagents).getByText(/检查边界条件/)).toBeInTheDocument();
  });

  it("derives tool and sub-agent entries only from supplied records", async () => {
    const user = userEvent.setup();
    renderDisclosure({
      activity: baseRecord({
        tools: [{ title: "only.real.tool", status: "completed" }],
        subagents: [{ title: "唯一子代理", status: "completed" }],
        steps: undefined,
      }),
    });

    await user.click(screen.getByTestId("agent-activity-summary"));

    expect(screen.getByText("only.real.tool")).toBeInTheDocument();
    expect(screen.getByText("唯一子代理")).toBeInTheDocument();
    expect(screen.queryByText("fs.read")).toBeNull();
    expect(screen.queryByText("fs.search")).toBeNull();
    expect(screen.queryByTestId("agent-activity-steps")).toBeNull();
  });

  it("keeps terminal failure and cancelled states visible and accessible", async () => {
    const user = userEvent.setup();
    renderDisclosure({
      activity: [
        baseRecord({
          role: "实现者",
          status: "failed",
          currentStep: "应用补丁",
          errorMessage: "工具执行失败",
          tools: [{ title: "fs.write", status: "failed" }],
          // Distinct stable identity: real timeline runs are unique by runId.
          runId: "run-terminal-failed",
        }),
        baseRecord({
          role: "审查者",
          status: "cancelled",
          currentStep: "已中止",
          tools: [{ title: "fs.list", status: "cancelled" }],
          runId: "run-terminal-cancelled",
        }),
      ],
    });

    const region = screen.getByTestId("agent-activity-disclosure");
    const text = region.textContent ?? "";
    expect(text).toMatch(/失败/);
    expect(text).toMatch(/已取消/);

    const summaries = screen.getAllByTestId("agent-activity-summary");
    expect(summaries.length).toBeGreaterThanOrEqual(2);

    for (const summary of summaries) {
      expect(summary.closest("details")).not.toHaveAttribute("open");
    }

    await user.click(summaries[0]!);
    expect(summaries[0]!.closest("details")).toHaveAttribute("open");
    // Failure detail remains reachable after expand.
    expect(screen.getByTestId("agent-activity-error")).toHaveTextContent(
      /工具执行失败/,
    );
    expect(within(region).getByText("fs.write")).toBeInTheDocument();
  });

  it("keeps successful history concise without stacking permanent cards", () => {
    renderDisclosure({
      activity: baseRecord({
        status: "completed",
        currentStep: "全部完成",
        tools: [
          { title: "t1", status: "completed" },
          { title: "t2", status: "completed" },
        ],
        subagents: [{ title: "s1", status: "completed" }],
      }),
    });

    const root = screen.getByTestId("agent-activity-disclosure");
    // One compact disclosure, not a permanent open card stack.
    expect(root.querySelectorAll("[data-testid='agent-activity-card']").length).toBe(
      0,
    );
    const details = within(root).getAllByTestId("agent-activity-details");
    expect(details).toHaveLength(1);
    expect(details[0]).not.toHaveAttribute("open");
    expect(within(root).getByTestId("agent-activity-summary")).toHaveTextContent(
      /完成|全部完成/,
    );
  });

  it("invokes onOpenArtifact with portable relative paths only", async () => {
    const user = userEvent.setup();
    const onOpenArtifact = vi.fn();
    renderDisclosure({
      activity: baseRecord({
        artifacts: [
          { path: "src/main.rs", kind: "file", label: "main.rs" },
          { path: "C:\\Users\\Alice\\Projects\\pillow\\secret.rs", kind: "file", label: "secret" },
          { path: "/abs/root/file.ts", kind: "file", label: "abs" },
          { path: "../escape.ts", kind: "file", label: "escape" },
        ],
      }),
      onOpenArtifact,
    });

    await user.click(screen.getByTestId("agent-activity-summary"));

    // Nested artifact disclosure — open progressive section if present.
    const artifactToggle = screen.queryByTestId("agent-activity-artifacts-toggle");
    if (artifactToggle) {
      await user.click(artifactToggle);
    }

    const openButtons = screen.getAllByRole("button", { name: /打开|main\.rs|src\/main\.rs/i });
    expect(openButtons.length).toBeGreaterThanOrEqual(1);
    await user.click(openButtons[0]!);

    expect(onOpenArtifact).toHaveBeenCalledTimes(1);
    expect(onOpenArtifact).toHaveBeenCalledWith("src/main.rs");
    for (const call of onOpenArtifact.mock.calls) {
      const path = call[0] as string;
      expect(path).not.toMatch(/^[A-Za-z]:/);
      expect(path).not.toMatch(/^[/\\]/);
      expect(path).not.toContain("..");
      expect(path).not.toContain("\\");
    }

    expect(screen.queryByText(/C:\\Users\\Alice/)).toBeNull();
    expect(screen.queryByText(/secret\.rs/)).toBeNull();
    expect(screen.queryByText(/escape\.ts/)).toBeNull();
  });

  it("renders opaque errors without leaking machine paths", async () => {
    const user = userEvent.setup();
    renderDisclosure({
      activity: baseRecord({
        status: "failed",
        errorMessage:
          "ENOENT: no such file C:\\Users\\Alice\\Projects\\pillow\\apps\\desktop\\src\\hidden.ts",
        tools: [
          {
            title: "fs.read",
            status: "failed",
            error: "failed to read C:\\Users\\me\\secret.env",
          },
        ],
      }),
    });

    await user.click(screen.getByTestId("agent-activity-summary"));

    const body = document.body.textContent ?? "";
    expect(body).not.toContain("C:\\Users\\Alice\\Projects\\pillow");
    expect(body).not.toContain("C:\\Users");
    expect(body).not.toContain("hidden.ts");
    expect(body).not.toContain("secret.env");
    // Opaque failure copy remains visible (path-bearing host messages replaced).
    expect(screen.getByTestId("agent-activity-error")).toHaveTextContent(
      /无法完成/,
    );
  });
});
