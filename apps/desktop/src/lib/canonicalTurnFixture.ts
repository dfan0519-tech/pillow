/**
 * Canonical Task 2 fake-turn fixture (snake_case payloads).
 * Source of truth: wave-b1-runtime tests/fixtures/protocol/fake-turn.jsonl
 */

import type { RuntimeEventKind } from "./conversationGateway";
import type { RuntimeEventDto } from "./conversationGateway";

export type FixtureStep = {
  kind: RuntimeEventKind;
  delay_ms: number;
  payload: RuntimeEventDto["payload"];
};

/** Ordered steps matching tests/fixtures/protocol/fake-turn.jsonl. */
export const CANONICAL_TURN_FIXTURE: FixtureStep[] = [
  {
    kind: "run_started",
    delay_ms: 0,
    payload: { prompt_preview: "summarize the workspace" },
  },
  { kind: "heartbeat", delay_ms: 1, payload: { tick: 1 } },
  {
    kind: "reasoning_summary_delta",
    delay_ms: 1,
    payload: { text: "Inspecting project layout." },
  },
  {
    kind: "plan_updated",
    delay_ms: 1,
    payload: {
      steps: [
        { id: "s1", title: "Scan files", status: "active" },
        { id: "s2", title: "Write summary", status: "pending" },
      ],
    },
  },
  {
    kind: "tool_call_started",
    delay_ms: 1,
    payload: {
      tool_call_id: "tc-list",
      name: "list_dir",
      summary: "list workspace root",
    },
  },
  {
    kind: "tool_call_output",
    delay_ms: 1,
    payload: {
      tool_call_id: "tc-list",
      chunk: "crates/\napps/\nREADME.md\n",
    },
  },
  {
    kind: "tool_call_completed",
    delay_ms: 1,
    payload: { tool_call_id: "tc-list", status: "completed" },
  },
  {
    kind: "subagent_started",
    delay_ms: 1,
    payload: { subagent_id: "sa-explore", role: "explore" },
  },
  {
    kind: "subagent_progress",
    delay_ms: 1,
    payload: {
      subagent_id: "sa-explore",
      summary: "indexed 3 top-level entries",
    },
  },
  {
    kind: "subagent_completed",
    delay_ms: 1,
    payload: { subagent_id: "sa-explore", status: "completed" },
  },
  {
    kind: "plan_updated",
    delay_ms: 1,
    payload: {
      steps: [
        { id: "s1", title: "Scan files", status: "completed" },
        { id: "s2", title: "Write summary", status: "active" },
      ],
    },
  },
  {
    kind: "assistant_delta",
    delay_ms: 1,
    payload: { text: "Workspace contains " },
  },
  {
    kind: "assistant_delta",
    delay_ms: 1,
    payload: { text: "apps, crates, and docs. " },
  },
  {
    kind: "assistant_delta",
    delay_ms: 1,
    payload: { text: "Ready for deeper work." },
  },
  {
    kind: "artifact_produced",
    delay_ms: 1,
    payload: {
      artifact_id: "art-summary",
      path: "artifacts/workspace-summary.md",
      kind: "markdown",
    },
  },
  {
    kind: "plan_updated",
    delay_ms: 1,
    payload: {
      steps: [
        { id: "s1", title: "Scan files", status: "completed" },
        { id: "s2", title: "Write summary", status: "completed" },
      ],
    },
  },
  {
    kind: "turn_completed",
    delay_ms: 0,
    payload: { summary: "workspace summary ready" },
  },
];

/** Materialize fixture steps into typed RuntimeEventDto values for a run. */
export function materializeCanonicalEvents(
  conversationId: string,
  runId: string,
  createdAtBase = "2026-07-30T12:00:00.000Z",
): RuntimeEventDto[] {
  const baseMs = Date.parse(createdAtBase);
  return CANONICAL_TURN_FIXTURE.map((step, index) => {
    const createdAt = new Date(baseMs + index * 1000).toISOString();
    return {
      eventId: `evt-${runId}-${index + 1}`,
      runId,
      conversationId,
      sequence: index + 1,
      kind: step.kind,
      payload: step.payload,
      createdAt,
    } as RuntimeEventDto;
  });
}
