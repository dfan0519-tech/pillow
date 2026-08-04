/**
 * Wave C2 Task 3 — real conversation timeline → AgentActivityDisclosure projection.
 * Consumes actual RunActivityView fixtures; asserts public DOM behavior only.
 * Written before production mapper/mount (TDD RED first).
 * Review-fix pass: stable keys, exact artifact identity, ownership/chronology,
 * defensive ID non-disclosure.
 */
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { AgentActivityDisclosure } from "../src/features/conversations/AgentActivityDisclosure";
import {
  agentActivityFromRun,
  resolveResourceFromActivity,
} from "../src/features/conversations/agentActivityFromTimeline";
import { ConversationTimeline } from "../src/features/conversations/ConversationTimeline";
import type {
  ConversationTimelineView,
  ResourceReferenceView,
  RunActivityView,
} from "../src/features/conversations/conversationTypes";
import "../src/styles/tokens.css";
import "../src/styles/workbench.css";
import "../src/styles/agent-activity.css";

const conversationId = "conv-timeline-owner-aaaa";
const foreignConversationId = "conv-foreign-bbbb";

const SENSITIVE_IDS = {
  runId: "run-raw-deadbeef-never-show",
  toolCallId: "tool-call-raw-id-xyz",
  subagentId: "subagent-raw-id-abc",
  artifactId: "artifact-raw-id-999",
  resourceId: "resource-raw-id-777",
  planStepId: "plan-step-raw-id-s1",
  receiptId: "receipt-raw-id-rrr",
  embeddedResourceId: "embedded-resource-raw-eee",
} as const;

function baseActivity(
  overrides: Partial<RunActivityView> = {},
): RunActivityView {
  return {
    runId: SENSITIVE_IDS.runId,
    status: "running",
    startedAt: "2026-07-31T08:00:00.000Z",
    updatedAt: "2026-07-31T08:00:05.000Z",
    assistantText: "",
    reasoningText: "",
    plan: {
      steps: [
        {
          id: SENSITIVE_IDS.planStepId,
          title: "分析需求",
          status: "completed",
        },
        {
          id: "plan-step-raw-id-s2",
          title: "读取源文件",
          status: "active",
        },
        {
          id: "plan-step-raw-id-s3",
          title: "应用补丁",
          status: "pending",
        },
      ],
    },
    tools: [
      {
        toolCallId: SENSITIVE_IDS.toolCallId,
        name: "fs.read",
        title: "读取 main.rs",
        status: "completed",
        startedAt: "2026-07-31T08:00:01.000Z",
        updatedAt: "2026-07-31T08:00:02.000Z",
        output: "fn main() {}",
        outputTruncated: true,
        omittedOutputChars: 42,
        resources: [
          {
            resourceId: SENSITIVE_IDS.resourceId,
            kind: "file",
            label: "main.rs",
            path: "src/main.rs",
            contentPreview: "fn main() {}",
            truncated: false,
          },
        ],
      },
      {
        toolCallId: "tool-call-raw-search",
        name: "fs.search",
        title: "搜索入口",
        status: "running",
        startedAt: "2026-07-31T08:00:03.000Z",
        updatedAt: "2026-07-31T08:00:04.000Z",
        resources: [],
      },
    ],
    subagents: [
      {
        subagentId: SENSITIVE_IDS.subagentId,
        title: "探索者",
        status: "running",
        startedAt: "2026-07-31T08:00:02.000Z",
        updatedAt: "2026-07-31T08:00:04.000Z",
        progress: "已索引 3 个顶层项",
      },
    ],
    artifacts: [
      {
        artifactId: SENSITIVE_IDS.artifactId,
        path: "reports/summary.md",
        kind: "markdown",
        createdAt: "2026-07-31T08:00:05.000Z",
        resource: {
          resourceId: "resource-artifact-raw-888",
          kind: "artifact",
          label: "摘要报告",
          path: "reports/summary.md",
          contentPreview: "# Summary",
          truncated: false,
        },
      },
    ],
    assistantTextTruncated: true,
    omittedAssistantChars: 100,
    reasoningTextTruncated: true,
    omittedReasoningChars: 50,
    omittedPlanStepCount: 2,
    omittedToolCount: 1,
    omittedSubagentCount: 1,
    omittedArtifactCount: 1,
    omittedResourceCount: 1,
    omittedResourcePreviewChars: 64,
    ...overrides,
  };
}

function emptyTimeline(
  overrides: Partial<ConversationTimelineView> = {},
): ConversationTimelineView {
  return {
    conversationId,
    messages: [],
    events: [],
    seenEventKeys: new Set(),
    assistantText: "",
    reasoningText: "",
    plan: null,
    tools: [],
    subagents: [],
    activitiesByRunId: {},
    activeRunId: null,
    ...overrides,
  };
}

function timelineWithActivity(
  activity: RunActivityView,
  convId = conversationId,
): ConversationTimelineView {
  return emptyTimeline({
    conversationId: convId,
    activitiesByRunId: { [activity.runId]: activity },
    activeRunId: activity.runId,
  });
}

function collectAttributeValues(root: ParentNode): string[] {
  const values: string[] = [];
  const walk = (node: Element) => {
    for (const attr of Array.from(node.attributes)) {
      values.push(attr.value);
    }
    for (const child of Array.from(node.children)) {
      walk(child);
    }
  };
  if (root instanceof Element) walk(root);
  else {
    for (const child of Array.from(root.children ?? [])) {
      walk(child as Element);
    }
  }
  return values;
}

function assertNoSensitiveIds(root: HTMLElement = document.body) {
  const text = root.textContent ?? "";
  const attrs = collectAttributeValues(root).join("\n");
  for (const id of Object.values(SENSITIVE_IDS)) {
    expect(text).not.toContain(id);
    expect(attrs).not.toContain(id);
  }
  expect(text).not.toContain("resource-artifact-raw-888");
  expect(attrs).not.toContain("resource-artifact-raw-888");
  expect(text).not.toContain("plan-step-raw-id-s2");
  expect(attrs).not.toContain("plan-step-raw-id-s2");
  expect(text).not.toContain("tool-call-raw-search");
  expect(attrs).not.toContain("tool-call-raw-search");
  expect(text).not.toContain(SENSITIVE_IDS.receiptId);
  expect(attrs).not.toContain(SENSITIVE_IDS.receiptId);
  expect(text).not.toContain(SENSITIVE_IDS.embeddedResourceId);
  expect(attrs).not.toContain(SENSITIVE_IDS.embeddedResourceId);
}

describe("agentActivityFromRun mapper (pure)", () => {
  it("maps RunActivityView into a conversation-owned AgentActivityRecord", () => {
    const activity = baseActivity();
    const record = agentActivityFromRun(conversationId, activity);

    expect(record.conversationId).toBe(conversationId);
    expect(record.runId).toBe(SENSITIVE_IDS.runId);
    // Stable non-DOM identity from the real run.
    expect(record.recordIdentity).toBe(SENSITIVE_IDS.runId);
    // Carrier includes discarded IDs (embedded resource) for sanitization.
    expect(record.forbiddenDisplayIds).toEqual(
      expect.arrayContaining([
        SENSITIVE_IDS.runId,
        SENSITIVE_IDS.planStepId,
        SENSITIVE_IDS.toolCallId,
        SENSITIVE_IDS.resourceId,
        SENSITIVE_IDS.subagentId,
        SENSITIVE_IDS.artifactId,
        "resource-artifact-raw-888",
      ]),
    );
    expect(record.status).toBe("running");
    expect(record.currentStep).toBe("读取源文件");
    expect(record.role).toBeUndefined();
    expect(record.testResults ?? []).toEqual([]);

    expect(record.steps?.map((s) => s.title)).toEqual([
      "分析需求",
      "读取源文件",
      "应用补丁",
    ]);
    expect(record.steps?.map((s) => s.status)).toEqual([
      "completed",
      "active",
      "pending",
    ]);

    expect(record.tools?.map((t) => t.title)).toEqual([
      "读取 main.rs",
      "搜索入口",
    ]);
    expect(record.tools?.[0]?.output).toBe("fn main() {}");
    expect(record.tools?.[0]?.outputTruncated).toBe(true);
    expect(record.tools?.[0]?.omittedOutputChars).toBe(42);

    expect(record.subagents?.map((s) => s.title)).toEqual(["探索者"]);
    expect(record.subagents?.[0]?.progress).toBe("已索引 3 个顶层项");
    expect(record.subagents?.[0]?.role).toBeUndefined();

    // Tool resources first, then run artifacts — portable paths only.
    expect(record.artifacts?.map((a) => a.path)).toEqual([
      "src/main.rs",
      "reports/summary.md",
    ]);
    // Each mapped artifact carries a non-DOM source token.
    expect(record.artifacts?.[0]?.sourceToken).toMatch(/^tool-resource:/);
    expect(record.artifacts?.[1]?.sourceToken).toMatch(/^run-artifact:/);

    const noticeText = (record.notices ?? []).map((n) => n.text).join("\n");
    expect(noticeText).toMatch(/回复已截断.*100/);
    expect(noticeText).toMatch(/推理已截断.*50/);
    expect(noticeText).toMatch(/计划步骤/);
    expect(noticeText).toMatch(/工具/);
    expect(noticeText).toMatch(/子代理/);
    expect(noticeText).toMatch(/产物/);
    expect(noticeText).toMatch(/资源/);
    expect(noticeText).toMatch(/64/);
  });

  it("omits currentStep when no active plan step exists", () => {
    const activity = baseActivity({
      plan: {
        steps: [
          { id: "s1", title: "已完成步骤", status: "completed" },
          { id: "s2", title: "待办步骤", status: "pending" },
        ],
      },
    });
    const record = agentActivityFromRun(conversationId, activity);
    expect(record.currentStep).toBeUndefined();
  });

  it("routes failed/cancelled tool output through the error path", () => {
    const activity = baseActivity({
      tools: [
        {
          toolCallId: "tool-fail",
          name: "fs.read",
          title: "失败工具",
          status: "failed",
          startedAt: "2026-07-31T08:00:01.000Z",
          updatedAt: "2026-07-31T08:00:02.000Z",
          output: "ENOENT F:\\secret\\hidden.env",
          resources: [],
        },
        {
          toolCallId: "tool-cancel",
          name: "fs.write",
          title: "取消工具",
          status: "cancelled",
          startedAt: "2026-07-31T08:00:01.000Z",
          updatedAt: "2026-07-31T08:00:02.000Z",
          output: "cancelled mid-flight",
          resources: [],
        },
      ],
    });
    const record = agentActivityFromRun(conversationId, activity);
    expect(record.tools?.[0]?.output).toBeUndefined();
    expect(record.tools?.[0]?.error).toBeTruthy();
    expect(record.tools?.[1]?.output).toBeUndefined();
    expect(record.tools?.[1]?.error).toBe("cancelled mid-flight");
  });

  it("drops non-portable artifact and resource paths", () => {
    const activity = baseActivity({
      tools: [
        {
          toolCallId: "t1",
          name: "fs.read",
          title: "读",
          status: "completed",
          startedAt: "2026-07-31T08:00:01.000Z",
          updatedAt: "2026-07-31T08:00:02.000Z",
          resources: [
            {
              resourceId: "r-abs",
              kind: "file",
              label: "abs",
              path: "<repo>\\secret.rs",
              truncated: false,
            },
            {
              resourceId: "r-url",
              kind: "file",
              label: "url",
              path: "https://example.com/x",
              truncated: false,
            },
            {
              resourceId: "r-ok",
              kind: "file",
              label: "ok",
              path: "src/lib.rs",
              truncated: false,
            },
          ],
        },
      ],
      artifacts: [
        {
          artifactId: "a1",
          path: "../escape.ts",
          kind: "file",
          createdAt: "2026-07-31T08:00:05.000Z",
        },
        {
          artifactId: "a2",
          path: "docs/note.md",
          kind: "file",
          createdAt: "2026-07-31T08:00:06.000Z",
        },
      ],
    });
    const record = agentActivityFromRun(conversationId, activity);
    expect(record.artifacts?.map((a) => a.path)).toEqual([
      "src/lib.rs",
      "docs/note.md",
    ]);
  });

  it("never invents test results or roles from tool names", () => {
    const activity = baseActivity({
      tools: [
        {
          toolCallId: "t-test",
          name: "vitest",
          title: "vitest run",
          status: "completed",
          startedAt: "2026-07-31T08:00:01.000Z",
          updatedAt: "2026-07-31T08:00:02.000Z",
          output: "Tests: 9 passed",
          resources: [],
        },
      ],
      subagents: [
        {
          subagentId: "sa1",
          title: "实现者",
          status: "completed",
          startedAt: "2026-07-31T08:00:01.000Z",
          updatedAt: "2026-07-31T08:00:02.000Z",
        },
      ],
      plan: null,
    });
    const record = agentActivityFromRun(conversationId, activity);
    expect(record.testResults ?? []).toEqual([]);
    expect(record.role).toBeUndefined();
    expect(record.subagents?.[0]?.role).toBeUndefined();
  });

  it("preserves colliding paths as distinct sources without silent dedupe", () => {
    const activity = baseActivity({
      tools: [
        {
          toolCallId: "t-shared",
          name: "fs.read",
          title: "读共享路径",
          status: "completed",
          startedAt: "2026-07-31T08:00:01.000Z",
          updatedAt: "2026-07-31T08:00:02.000Z",
          resources: [
            {
              resourceId: "res-tool-shared",
              kind: "file",
              label: "工具侧标签",
              path: "shared/report.md",
              contentPreview: "tool preview",
              truncated: false,
            },
          ],
        },
      ],
      artifacts: [
        {
          artifactId: "art-shared",
          path: "shared/report.md",
          kind: "markdown",
          createdAt: "2026-07-31T08:00:05.000Z",
          resource: {
            resourceId: "res-art-shared",
            kind: "artifact",
            label: "产物侧标签",
            path: "shared/report.md",
            contentPreview: "artifact preview",
            truncated: false,
          },
        },
      ],
    });
    const record = agentActivityFromRun(conversationId, activity);
    expect(record.artifacts).toHaveLength(2);
    expect(record.artifacts?.map((a) => a.path)).toEqual([
      "shared/report.md",
      "shared/report.md",
    ]);
    expect(record.artifacts?.[0]?.label).toBe("工具侧标签");
    expect(record.artifacts?.[1]?.label).toBe("产物侧标签");
    expect(record.artifacts?.[0]?.sourceToken).not.toBe(
      record.artifacts?.[1]?.sourceToken,
    );
  });
});

describe("ConversationTimeline mounts AgentActivityDisclosure", () => {
  it("renders AgentActivityDisclosure instead of the old RunActivity site", async () => {
    const user = userEvent.setup();
    const activity = baseActivity({ status: "completed" });
    render(
      <ConversationTimeline
        run={null}
        timeline={timelineWithActivity(activity)}
      />,
    );

    expect(screen.getByTestId("agent-activity-disclosure")).toBeInTheDocument();
    expect(screen.queryByTestId("run-activity")).toBeNull();

    await user.click(screen.getByTestId("agent-activity-summary"));
    expect(screen.getByTestId("agent-activity-steps")).toBeInTheDocument();
    assertNoSensitiveIds();
  });

  it("keeps disclosure open across status/currentStep rerenders via stable recordKey", async () => {
    const user = userEvent.setup();
    const running = baseActivity({
      status: "running",
      plan: {
        steps: [
          {
            id: SENSITIVE_IDS.planStepId,
            title: "分析需求",
            status: "completed",
          },
          {
            id: "plan-step-raw-id-s2",
            title: "读取源文件",
            status: "active",
          },
        ],
      },
    });

    const { rerender } = render(
      <ConversationTimeline
        run={null}
        timeline={timelineWithActivity(running)}
      />,
    );

    const details = screen.getByTestId(
      "agent-activity-details",
    ) as HTMLDetailsElement;
    expect(details.open).toBe(false);
    await user.click(screen.getByTestId("agent-activity-summary"));
    expect(details.open).toBe(true);
    expect(screen.getByTestId("agent-activity-summary")).toHaveTextContent(
      /读取源文件/,
    );

    // Same run identity, new status + current step — disclosure must stay open.
    const completed = baseActivity({
      runId: running.runId,
      status: "completed",
      plan: {
        steps: [
          {
            id: SENSITIVE_IDS.planStepId,
            title: "分析需求",
            status: "completed",
          },
          {
            id: "plan-step-raw-id-s2",
            title: "写入补丁",
            status: "completed",
          },
        ],
      },
    });

    rerender(
      <ConversationTimeline
        run={null}
        timeline={timelineWithActivity(completed)}
      />,
    );

    const detailsAfter = screen.getByTestId(
      "agent-activity-details",
    ) as HTMLDetailsElement;
    expect(detailsAfter.open).toBe(true);
    expect(screen.getByTestId("agent-activity-summary")).toHaveTextContent(
      /已完成/,
    );
    // Nested body still mounted (not remounted closed).
    expect(screen.getByTestId("agent-activity-steps")).toBeInTheDocument();
  });

  it("keeps the same run's disclosure open when the activity array is reordered", async () => {
    const user = userEvent.setup();
    const runAlpha = {
      ...agentActivityFromRun(
        conversationId,
        baseActivity({
          runId: "run-alpha-stable-id",
          status: "running",
          plan: {
            steps: [
              {
                id: "step-alpha",
                title: "Alpha 步骤",
                status: "active",
              },
            ],
          },
          tools: [],
          subagents: [],
          artifacts: [],
          assistantTextTruncated: false,
          reasoningTextTruncated: false,
          omittedPlanStepCount: 0,
          omittedToolCount: 0,
          omittedSubagentCount: 0,
          omittedArtifactCount: 0,
          omittedResourceCount: 0,
          omittedResourcePreviewChars: 0,
        }),
      ),
      // Distinct visible role so summaries stay identifiable after reorder.
      role: "Alpha 角色",
    };
    const runBeta = {
      ...agentActivityFromRun(
        conversationId,
        baseActivity({
          runId: "run-beta-stable-id",
          status: "completed",
          plan: {
            steps: [
              {
                id: "step-beta",
                title: "Beta 步骤",
                status: "completed",
              },
            ],
          },
          tools: [],
          subagents: [],
          artifacts: [],
          assistantTextTruncated: false,
          reasoningTextTruncated: false,
          omittedPlanStepCount: 0,
          omittedToolCount: 0,
          omittedSubagentCount: 0,
          omittedArtifactCount: 0,
          omittedResourceCount: 0,
          omittedResourcePreviewChars: 0,
        }),
      ),
      role: "Beta 角色",
    };

    expect(runAlpha.recordIdentity).toBe("run-alpha-stable-id");
    expect(runBeta.recordIdentity).toBe("run-beta-stable-id");
    expect(runAlpha.recordIdentity).not.toBe(runBeta.recordIdentity);

    const { rerender } = render(
      <AgentActivityDisclosure
        activity={[runAlpha, runBeta]}
        conversationId={conversationId}
      />,
    );

    const summaries = screen.getAllByTestId("agent-activity-summary");
    expect(summaries).toHaveLength(2);
    // Open Beta (second item) while Alpha is first.
    const betaSummary = summaries.find((el) =>
      (el.textContent ?? "").includes("Beta 角色"),
    );
    expect(betaSummary).toBeTruthy();
    await user.click(betaSummary!);

    const detailsBefore = screen.getAllByTestId(
      "agent-activity-details",
    ) as HTMLDetailsElement[];
    const betaDetailsBefore = detailsBefore.find((el) =>
      (el.textContent ?? "").includes("Beta 角色"),
    );
    expect(betaDetailsBefore?.open).toBe(true);

    // Reverse array order — Beta must remain open via stable recordIdentity.
    rerender(
      <AgentActivityDisclosure
        activity={[runBeta, runAlpha]}
        conversationId={conversationId}
      />,
    );

    const detailsAfter = screen.getAllByTestId(
      "agent-activity-details",
    ) as HTMLDetailsElement[];
    const betaDetailsAfter = detailsAfter.find((el) =>
      (el.textContent ?? "").includes("Beta 角色"),
    );
    const alphaDetailsAfter = detailsAfter.find((el) =>
      (el.textContent ?? "").includes("Alpha 角色"),
    );
    expect(betaDetailsAfter?.open).toBe(true);
    expect(alphaDetailsAfter?.open).toBe(false);
    // Identity stays React-internal — never text/attrs.
    const region = screen.getByTestId("agent-activity-disclosure");
    expect(region.textContent ?? "").not.toContain("run-alpha-stable-id");
    expect(region.textContent ?? "").not.toContain("run-beta-stable-id");
    const attrs = collectAttributeValues(region).join("\n");
    expect(attrs).not.toContain("run-alpha-stable-id");
    expect(attrs).not.toContain("run-beta-stable-id");
  });

  it("filters foreign conversation records at disclosure level (owned renders, foreign does not)", async () => {
    const user = userEvent.setup();
    const owned = agentActivityFromRun(
      conversationId,
      baseActivity({
        status: "running",
        plan: {
          steps: [
            {
              id: "owned-step",
              title: "本会话步骤标题",
              status: "active",
            },
          ],
        },
        tools: [
          {
            toolCallId: "owned-tool",
            name: "fs.read",
            title: "本会话工具",
            status: "running",
            startedAt: "2026-07-31T08:00:01.000Z",
            updatedAt: "2026-07-31T08:00:02.000Z",
            resources: [],
          },
        ],
        subagents: [],
        artifacts: [],
        assistantTextTruncated: false,
        reasoningTextTruncated: false,
        omittedPlanStepCount: 0,
        omittedToolCount: 0,
        omittedSubagentCount: 0,
        omittedArtifactCount: 0,
        omittedResourceCount: 0,
        omittedResourcePreviewChars: 0,
      }),
    );
    const foreign = agentActivityFromRun(
      foreignConversationId,
      baseActivity({
        runId: "run-foreign-should-hide",
        status: "running",
        plan: {
          steps: [
            {
              id: "foreign-step",
              title: "外会话步骤标题",
              status: "active",
            },
          ],
        },
        tools: [
          {
            toolCallId: "foreign-tool",
            name: "fs.read",
            title: "外会话工具",
            status: "running",
            startedAt: "2026-07-31T08:00:01.000Z",
            updatedAt: "2026-07-31T08:00:02.000Z",
            resources: [],
          },
        ],
        subagents: [],
        artifacts: [],
        assistantTextTruncated: false,
        reasoningTextTruncated: false,
        omittedPlanStepCount: 0,
        omittedToolCount: 0,
        omittedSubagentCount: 0,
        omittedArtifactCount: 0,
        omittedResourceCount: 0,
        omittedResourcePreviewChars: 0,
      }),
    );

    render(
      <AgentActivityDisclosure
        activity={[foreign, owned]}
        conversationId={conversationId}
      />,
    );

    expect(screen.getByTestId("agent-activity-disclosure")).toBeInTheDocument();
    // Summary + step row both include the owned title.
    expect(screen.getAllByText(/本会话步骤标题/).length).toBeGreaterThan(0);
    expect(screen.queryByText(/外会话步骤标题/)).toBeNull();
    expect(screen.queryByText(/外会话工具/)).toBeNull();
    expect(screen.queryByText("run-foreign-should-hide")).toBeNull();
    expect(document.body.textContent ?? "").not.toContain(
      "run-foreign-should-hide",
    );

    await user.click(screen.getByTestId("agent-activity-summary"));
    expect(screen.getAllByText(/本会话工具/).length).toBeGreaterThan(0);
    expect(screen.queryByText(/外会话工具/)).toBeNull();
    expect(screen.queryByText(/外会话步骤标题/)).toBeNull();
  });

  it("preserves plan/tool/sub-agent order and actual statuses/progress", async () => {
    const user = userEvent.setup();
    render(
      <ConversationTimeline
        run={null}
        timeline={timelineWithActivity(baseActivity())}
      />,
    );

    await user.click(screen.getByTestId("agent-activity-summary"));

    const steps = screen.getByTestId("agent-activity-steps");
    const stepItems = within(steps).getAllByRole("listitem");
    expect(stepItems.map((el) => el.textContent ?? "").join("\n")).toMatch(
      /分析需求[\s\S]*读取源文件[\s\S]*应用补丁/,
    );
    expect(stepItems[0]).toHaveAttribute("data-status", "completed");
    expect(stepItems[1]).toHaveAttribute("data-status", "active");
    expect(stepItems[2]).toHaveAttribute("data-status", "pending");

    const tools = screen.getByTestId("agent-activity-tools");
    const toolItems = within(tools).getAllByRole("listitem");
    expect(toolItems[0]).toHaveTextContent(/读取 main\.rs/);
    expect(toolItems[0]).toHaveAttribute("data-status", "completed");
    expect(toolItems[1]).toHaveTextContent(/搜索入口/);
    expect(toolItems[1]).toHaveAttribute("data-status", "running");

    const subagents = screen.getByTestId("agent-activity-subagents");
    expect(within(subagents).getByText(/探索者/)).toBeInTheDocument();
    expect(within(subagents).getByText(/已索引 3 个顶层项/)).toBeInTheDocument();
    assertNoSensitiveIds();
  });

  it("never exposes run/tool/sub-agent/artifact/resource IDs in text or attributes", async () => {
    const user = userEvent.setup();
    render(
      <ConversationTimeline
        run={null}
        timeline={timelineWithActivity(baseActivity())}
      />,
    );
    await user.click(screen.getByTestId("agent-activity-summary"));
    const artifactToggle = screen.queryByTestId("agent-activity-artifacts-toggle");
    if (artifactToggle) await user.click(artifactToggle);
    assertNoSensitiveIds(screen.getByTestId("agent-activity-disclosure"));
  });

  it("never renders successful raw tool output while preserving fixed disclosure state", async () => {
    const user = userEvent.setup();
    const leaked = {
      runId: "run-output-debug-111",
      sessionId: "session-output-debug-222",
      toolCallId: "tool-output-debug-333",
      receiptId: "receipt-output-debug-444",
      resourceId: "resource-output-debug-555",
      windowsPath: "F:\\agent\\workspace\\secret.env",
      uncPath: "\\\\build-host\\private\\trace.log",
      posixPath: "/home/agent/.config/credentials",
      fileUrl: "file:///C:/agent/workspace/secret.env",
      traceId: "trace-output-debug-666",
      reviewerPayload:
        '"debug": {"session id":"session-live-opaque-777"}; file://localhost/C$/agent/secret.env',
      nestedJsonPayload:
        'result={"debug":{"api_token":"token-opaque-777"}}',
      metadataArrayPayload:
        '[{"metadata":{"api_token":"metadata-token-opaque-888"}}]',
      escapedDebugPayload:
        '{"de\\u0062ug":{"api_token":"escaped-key-token-999"}}',
      escapedLeadingDebugPayload:
        '{"\\u0064ebug":{"api_token":"escaped-leading-token-998"}}',
    };
    const activity = baseActivity({
      status: "completed",
      plan: null,
      tools: [
        {
          toolCallId: leaked.toolCallId,
          name: "vitest",
          title: "运行测试",
          status: "completed",
          startedAt: "2026-07-31T08:00:01.000Z",
          updatedAt: "2026-07-31T08:00:02.000Z",
          output: [
            "9 passed",
            "const passed = 9;",
            `runId=${leaked.runId}; session_id=${leaked.sessionId}; toolCallId=${leaked.toolCallId}`,
            `receiptId=${leaked.receiptId}; resourceId=${leaked.resourceId}`,
            `paths ${leaked.windowsPath} ${leaked.uncPath} ${leaked.posixPath} ${leaked.fileUrl}`,
            `debug={\"traceId\":\"${leaked.traceId}\",\"requestId\":\"request-output-debug-777\"}`,
            leaked.reviewerPayload,
            leaked.nestedJsonPayload,
            leaked.metadataArrayPayload,
            leaked.escapedDebugPayload,
            leaked.escapedLeadingDebugPayload,
          ].join("\n"),
          outputTruncated: true,
          omittedOutputChars: 123,
          resources: [],
        },
      ],
      subagents: [],
      artifacts: [],
      assistantTextTruncated: false,
      reasoningTextTruncated: false,
      omittedPlanStepCount: 0,
      omittedToolCount: 0,
      omittedSubagentCount: 0,
      omittedArtifactCount: 0,
      omittedResourceCount: 0,
      omittedResourcePreviewChars: 0,
    });

    render(
      <ConversationTimeline
        run={null}
        timeline={timelineWithActivity(activity)}
      />,
    );

    await user.click(screen.getByTestId("agent-activity-summary"));

    const region = screen.getByTestId("agent-activity-disclosure");
    const text = region.textContent ?? "";
    const attrs = collectAttributeValues(region).join("\n");
    for (const secret of Object.values(leaked)) {
      expect(text).not.toContain(secret);
      expect(attrs).not.toContain(secret);
    }
    for (const reviewerFragment of [
      '"debug"',
      '"session id"',
      "session-live-opaque-777",
      "file://localhost/C$/agent/secret.env",
      '"api_token"',
      "token-opaque-777",
      '"metadata"',
      "metadata-token-opaque-888",
      "de\\u0062ug",
      "\\u0064ebug",
      "escaped-key-token-999",
      "escaped-leading-token-998",
    ]) {
      expect(text).not.toContain(reviewerFragment);
      expect(attrs).not.toContain(reviewerFragment);
    }
    expect(text).not.toContain("9 passed");
    expect(text).not.toContain("const passed = 9;");
    expect(text).toContain("工具已返回输出。");
    expect(text).toMatch(/输出已截断.*123/);
  });

  it("sanitizes adversarial labels that echo associated raw IDs", async () => {
    const user = userEvent.setup();
    const adversarial = baseActivity({
      status: "running",
      plan: {
        steps: [
          {
            id: SENSITIVE_IDS.planStepId,
            title: `步骤 ${SENSITIVE_IDS.planStepId}`,
            status: "active",
          },
        ],
      },
      tools: [
        {
          toolCallId: SENSITIVE_IDS.toolCallId,
          name: "fs.read",
          title: `工具 ${SENSITIVE_IDS.toolCallId}`,
          status: "completed",
          startedAt: "2026-07-31T08:00:01.000Z",
          updatedAt: "2026-07-31T08:00:02.000Z",
          output: "ok",
          resources: [
            {
              resourceId: SENSITIVE_IDS.resourceId,
              kind: "file",
              label: `资源 ${SENSITIVE_IDS.resourceId}`,
              path: "src/adv.rs",
              contentPreview: "code",
              truncated: false,
            },
          ],
        },
      ],
      subagents: [
        {
          subagentId: SENSITIVE_IDS.subagentId,
          title: `代理 ${SENSITIVE_IDS.subagentId}`,
          status: "running",
          startedAt: "2026-07-31T08:00:02.000Z",
          updatedAt: "2026-07-31T08:00:04.000Z",
          progress: `进度 ${SENSITIVE_IDS.subagentId}`,
        },
      ],
      artifacts: [
        {
          artifactId: SENSITIVE_IDS.artifactId,
          path: "reports/adv.md",
          kind: "markdown",
          createdAt: "2026-07-31T08:00:05.000Z",
          resource: {
            resourceId: "resource-artifact-raw-888",
            kind: "artifact",
            label: `产物 ${SENSITIVE_IDS.artifactId}`,
            path: "reports/adv.md",
            contentPreview: "adv",
            truncated: false,
          },
        },
      ],
    });

    render(
      <ConversationTimeline
        run={null}
        timeline={timelineWithActivity(adversarial)}
      />,
    );

    await user.click(screen.getByTestId("agent-activity-summary"));
    await user.click(screen.getByTestId("agent-activity-artifacts-toggle"));

    const region = screen.getByTestId("agent-activity-disclosure");
    assertNoSensitiveIds(region);
    // Ordinary chrome remains; only the echoed IDs are stripped.
    expect(region.textContent ?? "").toMatch(/步骤|工具|代理|产物|adv\.md|adv\.rs/);
    // Portable path passed to open must remain unmutated.
    const record = agentActivityFromRun(conversationId, adversarial);
    const toolArtifact = record.artifacts?.find((a) => a.path === "src/adv.rs");
    expect(toolArtifact?.path).toBe("src/adv.rs");
    expect(toolArtifact?.sourceToken).toBeTruthy();
    const resolved = resolveResourceFromActivity(
      adversarial,
      toolArtifact!.path,
      toolArtifact!.sourceToken,
    );
    expect(resolved?.path).toBe("src/adv.rs");
  });

  it("sanitizes a receipt/resource label that echoes its receipt ID", async () => {
    const user = userEvent.setup();
    const onOpenResource = vi.fn();
    const receiptId = SENSITIVE_IDS.receiptId;
    const activity = baseActivity({
      status: "completed",
      plan: null,
      tools: [
        {
          toolCallId: "tool-with-receipt",
          name: "fs.write",
          title: "写入结果",
          status: "completed",
          startedAt: "2026-07-31T08:00:01.000Z",
          updatedAt: "2026-07-31T08:00:02.000Z",
          output: "ok",
          receipt: {
            receiptId,
            label: `回执 ${receiptId}`,
            resource: {
              resourceId: "res-from-receipt",
              kind: "receipt",
              // Label echoes the discarded receipt ID — must be sanitized.
              label: `资源 ${receiptId}`,
              path: "receipts/out.md",
              contentPreview: "receipt body",
              truncated: false,
            },
          },
          resources: [
            {
              resourceId: "res-tool-beside-receipt",
              kind: "file",
              // Tool resource label also echoes the same receipt ID.
              label: `旁路 ${receiptId}`,
              path: "receipts/out.md",
              contentPreview: "tool body",
              truncated: false,
            },
          ],
        },
      ],
      subagents: [],
      artifacts: [],
      assistantTextTruncated: false,
      reasoningTextTruncated: false,
      omittedPlanStepCount: 0,
      omittedToolCount: 0,
      omittedSubagentCount: 0,
      omittedArtifactCount: 0,
      omittedResourceCount: 0,
      omittedResourcePreviewChars: 0,
    });

    const record = agentActivityFromRun(conversationId, activity);
    expect(record.forbiddenDisplayIds).toEqual(
      expect.arrayContaining([receiptId, "res-from-receipt", "res-tool-beside-receipt"]),
    );
    // Carrier is internal-only — never a projected display field.
    expect(JSON.stringify(record.artifacts ?? [])).not.toContain("forbiddenDisplayIds");

    render(
      <ConversationTimeline
        onOpenResource={onOpenResource}
        run={null}
        timeline={timelineWithActivity(activity)}
      />,
    );

    await user.click(screen.getByTestId("agent-activity-summary"));
    await user.click(screen.getByTestId("agent-activity-artifacts-toggle"));

    const region = screen.getByTestId("agent-activity-disclosure");
    expect(region.textContent ?? "").not.toContain(receiptId);
    expect(collectAttributeValues(region).join("\n")).not.toContain(receiptId);
    // Neutral / path-leaf chrome remains; portable callback path unchanged.
    expect(region.textContent ?? "").toMatch(/out\.md|产物|旁路|资源/);

    const toolArtifact = record.artifacts?.find(
      (a) => a.path === "receipts/out.md",
    );
    expect(toolArtifact?.path).toBe("receipts/out.md");
    expect(toolArtifact?.sourceToken).toBeTruthy();
    const resolved = resolveResourceFromActivity(
      activity,
      toolArtifact!.path,
      toolArtifact!.sourceToken,
    );
    expect(resolved?.path).toBe("receipts/out.md");
    expect(resolved?.resourceId).toBe("res-tool-beside-receipt");

    await user.click(screen.getByRole("button", { name: /打开/ }));
    expect(onOpenResource).toHaveBeenCalledTimes(1);
    const opened = onOpenResource.mock.calls[0]![0] as ResourceReferenceView;
    expect(opened.path).toBe("receipts/out.md");
    // Opened reference identity remains exact; DOM never showed receiptId.
    expect(opened.resourceId).toBe("res-tool-beside-receipt");
  });

  it("sanitizes embedded resource label that echoes embedded resource ID (artifact ID differs)", async () => {
    const user = userEvent.setup();
    const onOpenResource = vi.fn();
    const embeddedId = SENSITIVE_IDS.embeddedResourceId;
    const distinctArtifactId = "artifact-distinct-from-embedded";
    const activity = baseActivity({
      status: "completed",
      plan: null,
      tools: [],
      subagents: [],
      artifacts: [
        {
          artifactId: distinctArtifactId,
          path: "reports/embed-report.md",
          kind: "markdown",
          createdAt: "2026-07-31T08:00:05.000Z",
          resource: {
            resourceId: embeddedId,
            kind: "artifact",
            // Entire label is the discarded embedded resource ID (not artifactId).
            label: embeddedId,
            path: "reports/embed-report.md",
            contentPreview: "EMBEDDED_PREVIEW_OK",
            truncated: false,
          },
        },
      ],
      assistantTextTruncated: false,
      reasoningTextTruncated: false,
      omittedPlanStepCount: 0,
      omittedToolCount: 0,
      omittedSubagentCount: 0,
      omittedArtifactCount: 0,
      omittedResourceCount: 0,
      omittedResourcePreviewChars: 0,
    });

    const record = agentActivityFromRun(conversationId, activity);
    expect(record.forbiddenDisplayIds).toEqual(
      expect.arrayContaining([embeddedId, distinctArtifactId]),
    );
    // Artifact field still uses artifactId, not embedded resourceId.
    expect(record.artifacts?.[0]?.artifactId).toBe(distinctArtifactId);
    expect(record.artifacts?.[0]?.artifactId).not.toBe(embeddedId);

    render(
      <ConversationTimeline
        onOpenResource={onOpenResource}
        run={null}
        timeline={timelineWithActivity(activity)}
      />,
    );

    await user.click(screen.getByTestId("agent-activity-summary"));
    await user.click(screen.getByTestId("agent-activity-artifacts-toggle"));

    const region = screen.getByTestId("agent-activity-disclosure");
    expect(region.textContent ?? "").not.toContain(embeddedId);
    expect(region.textContent ?? "").not.toContain(distinctArtifactId);
    expect(collectAttributeValues(region).join("\n")).not.toContain(embeddedId);
    expect(collectAttributeValues(region).join("\n")).not.toContain(
      distinctArtifactId,
    );
    // Safe path leaf remains as display chrome.
    expect(region.textContent ?? "").toMatch(/embed-report\.md/);

    const mapped = record.artifacts?.[0];
    expect(mapped?.path).toBe("reports/embed-report.md");
    expect(mapped?.sourceToken).toMatch(/^run-artifact:/);
    const resolved = resolveResourceFromActivity(
      activity,
      mapped!.path,
      mapped!.sourceToken,
    );
    expect(resolved?.path).toBe("reports/embed-report.md");
    expect(resolved?.resourceId).toBe(embeddedId);
    expect(resolved?.contentPreview).toBe("EMBEDDED_PREVIEW_OK");

    await user.click(
      screen.getByRole("button", { name: /打开 embed-report\.md/ }),
    );
    expect(onOpenResource).toHaveBeenCalledTimes(1);
    const opened = onOpenResource.mock.calls[0]![0] as ResourceReferenceView;
    expect(opened.path).toBe("reports/embed-report.md");
    expect(opened.resourceId).toBe(embeddedId);
    expect(opened.contentPreview).toBe("EMBEDDED_PREVIEW_OK");
  });

  it("keeps live, terminal, failure, interrupted, and cancelling states accessible", () => {
    const states: Array<{
      status: RunActivityView["status"];
      expectLabel: RegExp;
    }> = [
      { status: "running", expectLabel: /运行中/ },
      { status: "cancelling", expectLabel: /取消中/ },
      { status: "completed", expectLabel: /完成/ },
      { status: "failed", expectLabel: /失败/ },
      { status: "interrupted", expectLabel: /中断/ },
      { status: "cancelled", expectLabel: /取消/ },
    ];

    for (const { status, expectLabel } of states) {
      const { unmount } = render(
        <ConversationTimeline
          run={null}
          timeline={timelineWithActivity(
            baseActivity({
              runId: `run-state-${status}`,
              status,
              plan: null,
              tools: [],
              subagents: [],
              artifacts: [],
              assistantTextTruncated: false,
              reasoningTextTruncated: false,
              omittedPlanStepCount: 0,
              omittedToolCount: 0,
              omittedSubagentCount: 0,
              omittedArtifactCount: 0,
              omittedResourceCount: 0,
              omittedResourcePreviewChars: 0,
            }),
          )}
        />,
      );
      const summary = screen.getByTestId("agent-activity-summary");
      expect(summary).toHaveTextContent(expectLabel);
      // Raw English tokens must not appear as the primary status copy.
      if (status === "cancelling" || status === "interrupted") {
        expect(summary.textContent ?? "").not.toMatch(/\bcancelling\b/i);
        expect(summary.textContent ?? "").not.toMatch(/\binterrupted\b/i);
      }
      unmount();
    }
  });

  it("opens only portable tool/run artifacts through onOpenResource", async () => {
    const user = userEvent.setup();
    const onOpenResource = vi.fn();
    const activity = baseActivity({
      tools: [
        {
          toolCallId: "t-portable",
          name: "fs.read",
          title: "读 README",
          status: "completed",
          startedAt: "2026-07-31T08:00:01.000Z",
          updatedAt: "2026-07-31T08:00:02.000Z",
          resources: [
            {
              resourceId: "res-readme",
              kind: "file",
              label: "README.md",
              path: "README.md",
              contentPreview: "# Keeper",
              truncated: false,
            },
          ],
        },
      ],
      artifacts: [
        {
          artifactId: "art-report",
          path: "reports/result.md",
          kind: "report",
          createdAt: "2026-07-31T08:00:05.000Z",
          resource: {
            resourceId: "res-report",
            kind: "artifact",
            label: "Result report",
            path: "reports/result.md",
            contentPreview: "Report preview",
            truncated: true,
          },
        },
        {
          artifactId: "art-path-only",
          path: "docs/path-only.md",
          kind: "file",
          createdAt: "2026-07-31T08:00:06.000Z",
        },
      ],
    });

    render(
      <ConversationTimeline
        onOpenResource={onOpenResource}
        run={null}
        timeline={timelineWithActivity(activity)}
      />,
    );

    await user.click(screen.getByTestId("agent-activity-summary"));
    const artifactToggle = screen.getByTestId("agent-activity-artifacts-toggle");
    await user.click(artifactToggle);

    await user.click(screen.getByRole("button", { name: /打开 README\.md/ }));
    expect(onOpenResource).toHaveBeenCalledTimes(1);
    const first = onOpenResource.mock.calls[0]![0] as ResourceReferenceView;
    expect(first.path).toBe("README.md");
    expect(first.label).toBe("README.md");
    expect(first.contentPreview).toBe("# Keeper");

    await user.click(screen.getByRole("button", { name: /打开 Result report/ }));
    expect(onOpenResource).toHaveBeenCalledTimes(2);
    const second = onOpenResource.mock.calls[1]![0] as ResourceReferenceView;
    expect(second.path).toBe("reports/result.md");
    expect(second.label).toBe("Result report");

    await user.click(screen.getByRole("button", { name: /打开 path-only\.md|docs\/path-only/ }));
    expect(onOpenResource).toHaveBeenCalledTimes(3);
    const third = onOpenResource.mock.calls[2]![0] as ResourceReferenceView;
    expect(third.path).toBe("docs/path-only.md");
    // Minimal derived reference — no fabricated content.
    expect(third.contentPreview).toBeUndefined();
  });

  it("opens artifact A not mismatched embedded resource B", async () => {
    const user = userEvent.setup();
    const onOpenResource = vi.fn();
    const activity = baseActivity({
      tools: [],
      artifacts: [
        {
          artifactId: "art-path-a",
          path: "reports/artifact-a.md",
          kind: "markdown",
          createdAt: "2026-07-31T08:00:05.000Z",
          resource: {
            resourceId: "res-path-b",
            kind: "file",
            label: "Wrong Resource B",
            path: "other/resource-b.md",
            contentPreview: "CONTENT FROM B — must not open",
            truncated: false,
          },
        },
      ],
      plan: null,
      subagents: [],
      assistantTextTruncated: false,
      reasoningTextTruncated: false,
      omittedPlanStepCount: 0,
      omittedToolCount: 0,
      omittedSubagentCount: 0,
      omittedArtifactCount: 0,
      omittedResourceCount: 0,
      omittedResourcePreviewChars: 0,
    });

    render(
      <ConversationTimeline
        onOpenResource={onOpenResource}
        run={null}
        timeline={timelineWithActivity(activity)}
      />,
    );

    await user.click(screen.getByTestId("agent-activity-summary"));
    await user.click(screen.getByTestId("agent-activity-artifacts-toggle"));

    // Display uses artifact path leaf, never resource B label.
    expect(screen.queryByText(/Wrong Resource B/)).toBeNull();
    expect(screen.getAllByText(/artifact-a\.md/).length).toBeGreaterThan(0);

    await user.click(
      screen.getByRole("button", { name: /打开 artifact-a\.md/ }),
    );
    expect(onOpenResource).toHaveBeenCalledTimes(1);
    const opened = onOpenResource.mock.calls[0]![0] as ResourceReferenceView;
    expect(opened.path).toBe("reports/artifact-a.md");
    expect(opened.label).toBe("artifact-a.md");
    expect(opened.contentPreview).toBeUndefined();
    expect(opened.resourceId).not.toBe("res-path-b");
    expect(JSON.stringify(opened)).not.toContain("CONTENT FROM B");
    expect(JSON.stringify(opened)).not.toContain("resource-b.md");
  });

  it("opens exact references when tool resource and run artifact share a path", async () => {
    const user = userEvent.setup();
    const onOpenResource = vi.fn();
    const activity = baseActivity({
      tools: [
        {
          toolCallId: "t-collide",
          name: "fs.read",
          title: "读共享",
          status: "completed",
          startedAt: "2026-07-31T08:00:01.000Z",
          updatedAt: "2026-07-31T08:00:02.000Z",
          resources: [
            {
              resourceId: "res-tool-collide",
              kind: "file",
              label: "工具预览标签",
              path: "shared/same.md",
              contentPreview: "FROM_TOOL_RESOURCE",
              truncated: false,
            },
          ],
        },
      ],
      artifacts: [
        {
          artifactId: "art-collide",
          path: "shared/same.md",
          kind: "markdown",
          createdAt: "2026-07-31T08:00:05.000Z",
          resource: {
            resourceId: "res-art-collide",
            kind: "artifact",
            label: "产物预览标签",
            path: "shared/same.md",
            contentPreview: "FROM_RUN_ARTIFACT",
            truncated: false,
          },
        },
      ],
      plan: null,
      subagents: [],
      assistantTextTruncated: false,
      reasoningTextTruncated: false,
      omittedPlanStepCount: 0,
      omittedToolCount: 0,
      omittedSubagentCount: 0,
      omittedArtifactCount: 0,
      omittedResourceCount: 0,
      omittedResourcePreviewChars: 0,
    });

    render(
      <ConversationTimeline
        onOpenResource={onOpenResource}
        run={null}
        timeline={timelineWithActivity(activity)}
      />,
    );

    await user.click(screen.getByTestId("agent-activity-summary"));
    await user.click(screen.getByTestId("agent-activity-artifacts-toggle"));

    // Both labels visible — no silent path dedupe.
    expect(screen.getByText("工具预览标签")).toBeInTheDocument();
    expect(screen.getByText("产物预览标签")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /打开 工具预览标签/ }));
    expect(onOpenResource).toHaveBeenCalledTimes(1);
    const toolOpened = onOpenResource.mock.calls[0]![0] as ResourceReferenceView;
    expect(toolOpened.path).toBe("shared/same.md");
    expect(toolOpened.label).toBe("工具预览标签");
    expect(toolOpened.contentPreview).toBe("FROM_TOOL_RESOURCE");
    expect(toolOpened.resourceId).toBe("res-tool-collide");

    await user.click(screen.getByRole("button", { name: /打开 产物预览标签/ }));
    expect(onOpenResource).toHaveBeenCalledTimes(2);
    const artOpened = onOpenResource.mock.calls[1]![0] as ResourceReferenceView;
    expect(artOpened.path).toBe("shared/same.md");
    expect(artOpened.label).toBe("产物预览标签");
    expect(artOpened.contentPreview).toBe("FROM_RUN_ARTIFACT");
    expect(artOpened.resourceId).toBe("res-art-collide");
  });

  it("never invokes onOpenResource for absolute, traversal, or URL paths", async () => {
    const user = userEvent.setup();
    const onOpenResource = vi.fn();
    const activity = baseActivity({
      tools: [
        {
          toolCallId: "t-bad",
          name: "fs.read",
          title: "坏路径",
          status: "completed",
          startedAt: "2026-07-31T08:00:01.000Z",
          updatedAt: "2026-07-31T08:00:02.000Z",
          resources: [
            {
              resourceId: "r1",
              kind: "file",
              label: "abs",
              path: "C:\\Users\\me\\secret.env",
              truncated: false,
            },
            {
              resourceId: "r2",
              kind: "file",
              label: "escape",
              path: "../escape.ts",
              truncated: false,
            },
            {
              resourceId: "r3",
              kind: "file",
              label: "url",
              path: "https://evil.example/x",
              truncated: false,
            },
          ],
        },
      ],
      artifacts: [
        {
          artifactId: "a-abs",
          path: "/abs/root/file.ts",
          kind: "file",
          createdAt: "2026-07-31T08:00:05.000Z",
        },
      ],
    });

    render(
      <ConversationTimeline
        onOpenResource={onOpenResource}
        run={null}
        timeline={timelineWithActivity(activity)}
      />,
    );

    await user.click(screen.getByTestId("agent-activity-summary"));
    // Non-portable paths dropped — no artifact open buttons for them.
    expect(screen.queryByTestId("agent-activity-artifacts-toggle")).toBeNull();
    expect(screen.queryByRole("button", { name: /打开/ })).toBeNull();
    expect(onOpenResource).not.toHaveBeenCalled();
    expect(document.body.textContent ?? "").not.toContain("C:\\Users");
    expect(document.body.textContent ?? "").not.toContain("escape.ts");
    expect(document.body.textContent ?? "").not.toContain("evil.example");
  });

  it("shows a fixed output state, output-truncation, and run omission summaries after expansion", async () => {
    const user = userEvent.setup();
    render(
      <ConversationTimeline
        run={null}
        timeline={timelineWithActivity(baseActivity({ status: "completed" }))}
      />,
    );

    await user.click(screen.getByTestId("agent-activity-summary"));

    const body = screen.getByTestId("agent-activity-disclosure").textContent ?? "";
    expect(body).toMatch(/工具已返回输出。/);
    expect(body).not.toMatch(/fn main\(\) \{\}/);
    expect(body).toMatch(/输出已截断.*42/);
    expect(body).toMatch(/回复已截断.*100/);
    expect(body).toMatch(/推理已截断.*50/);
    expect(body).toMatch(/计划步骤/);
    expect(body).toMatch(/工具/);
    expect(body).toMatch(/子代理/);
    expect(body).toMatch(/产物/);
    expect(body).toMatch(/资源/);
    // Reasoning text body must not appear — only truncation summary.
    expect(body).not.toMatch(/Inspecting project|chain.of.thought|推理摘要/i);
  });

  it("renders no invented test or role content when structured data is absent", async () => {
    const user = userEvent.setup();
    const activity = baseActivity({
      status: "completed",
      plan: null,
      tools: [
        {
          toolCallId: "t-vitest",
          name: "vitest",
          title: "vitest run",
          status: "completed",
          startedAt: "2026-07-31T08:00:01.000Z",
          updatedAt: "2026-07-31T08:00:02.000Z",
          output: "9 passed",
          resources: [],
        },
      ],
      subagents: [
        {
          subagentId: "sa-impl",
          title: "实现者",
          status: "completed",
          startedAt: "2026-07-31T08:00:01.000Z",
          updatedAt: "2026-07-31T08:00:02.000Z",
        },
      ],
      artifacts: [],
      assistantTextTruncated: false,
      reasoningTextTruncated: false,
      omittedPlanStepCount: 0,
      omittedToolCount: 0,
      omittedSubagentCount: 0,
      omittedArtifactCount: 0,
      omittedResourceCount: 0,
      omittedResourcePreviewChars: 0,
    });

    render(
      <ConversationTimeline
        run={null}
        timeline={timelineWithActivity(activity)}
      />,
    );

    await user.click(screen.getByTestId("agent-activity-summary"));

    expect(screen.queryByTestId("agent-activity-tests")).toBeNull();
    expect(screen.queryByText(/测试结果/)).toBeNull();
    // Sub-agent title is real; role chrome must not be invented beside it.
    const region = screen.getByTestId("agent-activity-disclosure");
    expect(within(region).getByText("实现者")).toBeInTheDocument();
    // No synthetic role labels like "角色: 实现者" beyond the real title.
    expect(region.textContent ?? "").not.toMatch(/角色\s*[:：]/);
    expect(region.textContent ?? "").not.toMatch(/testResults|passed.*skipped/i);
  });

  it("keeps chronological message/activity ordering and conversation isolation", async () => {
    const user = userEvent.setup();
    const activityEarly = baseActivity({
      runId: "run-early",
      status: "completed",
      startedAt: "2026-07-31T07:00:00.000Z",
      updatedAt: "2026-07-31T07:00:01.000Z",
      plan: null,
      tools: [
        {
          toolCallId: "t-early",
          name: "fs.read",
          title: "早期工具标记",
          status: "completed",
          startedAt: "2026-07-31T07:00:00.000Z",
          updatedAt: "2026-07-31T07:00:01.000Z",
          resources: [],
        },
      ],
      subagents: [],
      artifacts: [],
      assistantTextTruncated: false,
      reasoningTextTruncated: false,
      omittedPlanStepCount: 0,
      omittedToolCount: 0,
      omittedSubagentCount: 0,
      omittedArtifactCount: 0,
      omittedResourceCount: 0,
      omittedResourcePreviewChars: 0,
    });
    const activityLate = baseActivity({
      runId: "run-late",
      status: "completed",
      startedAt: "2026-07-31T09:00:00.000Z",
      updatedAt: "2026-07-31T09:00:01.000Z",
      plan: null,
      tools: [
        {
          toolCallId: "t-late",
          name: "fs.read",
          title: "晚期工具标记",
          status: "completed",
          startedAt: "2026-07-31T09:00:00.000Z",
          updatedAt: "2026-07-31T09:00:01.000Z",
          resources: [],
        },
      ],
      subagents: [],
      artifacts: [],
      assistantTextTruncated: false,
      reasoningTextTruncated: false,
      omittedPlanStepCount: 0,
      omittedToolCount: 0,
      omittedSubagentCount: 0,
      omittedArtifactCount: 0,
      omittedResourceCount: 0,
      omittedResourcePreviewChars: 0,
    });

    const timeline = emptyTimeline({
      conversationId,
      messages: [
        {
          id: "msg-mid",
          role: "user",
          content: "中间用户消息",
          createdAt: "2026-07-31T08:00:00.000Z",
        },
      ],
      activitiesByRunId: {
        [activityEarly.runId]: activityEarly,
        [activityLate.runId]: activityLate,
      },
      activeRunId: activityLate.runId,
    });

    const { rerender } = render(
      <ConversationTimeline run={null} timeline={timeline} />,
    );

    expect(screen.getAllByTestId("agent-activity-disclosure")).toHaveLength(2);

    // Expand both activity disclosures so tool titles enter the DOM order.
    const summaries = screen.getAllByTestId("agent-activity-summary");
    expect(summaries).toHaveLength(2);
    await user.click(summaries[0]!);
    await user.click(summaries[1]!);

    const page = screen.getByTestId("timeline-page");
    const text = page.textContent ?? "";
    const earlyIdx = text.indexOf("早期工具标记");
    const midIdx = text.indexOf("中间用户消息");
    const lateIdx = text.indexOf("晚期工具标记");
    expect(earlyIdx).toBeGreaterThanOrEqual(0);
    expect(midIdx).toBeGreaterThanOrEqual(0);
    expect(lateIdx).toBeGreaterThanOrEqual(0);
    // DOM chronology: early activity < middle message < late activity.
    expect(earlyIdx).toBeLessThan(midIdx);
    expect(midIdx).toBeLessThan(lateIdx);

    // Project/conversation isolation: remount under a different conversation clears prior.
    rerender(
      <ConversationTimeline
        run={null}
        timeline={emptyTimeline({
          conversationId: foreignConversationId,
          messages: [
            {
              id: "msg-foreign",
              role: "user",
              content: "外会话消息",
              createdAt: "2026-07-31T10:00:00.000Z",
            },
          ],
        })}
      />,
    );
    expect(screen.queryByText("中间用户消息")).toBeNull();
    expect(screen.queryByText("早期工具标记")).toBeNull();
    expect(screen.queryByText("晚期工具标记")).toBeNull();
    expect(screen.getByText("外会话消息")).toBeInTheDocument();
    expect(screen.queryByTestId("agent-activity-disclosure")).toBeNull();
  });
});
