/**
 * Pure mapper: real RunActivityView → AgentActivityRecord for timeline mount.
 * Creates no state; never fabricates roles, tests, or progress.
 */

import { isPortableRelativePath } from "./agentActivityProjection";
import type {
  RuntimeEventDto,
  WriteProposalResolvedPayload,
} from "../../lib/conversationGateway";
import type {
  AgentActivityNoticeInput,
  AgentActivityRecord,
  AgentArtifactActivityInput,
  AgentSubagentActivityInput,
  AgentToolActivityInput,
  AgentWriteOutcomeInput,
} from "./agentActivityTypes";
import type {
  ResourceReferenceView,
  RunActivityView,
} from "./conversationTypes";

/** Internal source-token prefixes — never written to DOM. */
const TOOL_RESOURCE_TOKEN = "tool-resource";
const RUN_ARTIFACT_TOKEN = "run-artifact";

/**
 * Project one durable run activity into a conversation-owned disclosure record.
 * IDs stay as internal keys only and must never become visible UI copy.
 * Optional runtime events supply terminal write outcomes for this run.
 */
export function agentActivityFromRun(
  conversationId: string,
  activity: RunActivityView,
  events?: readonly RuntimeEventDto[],
): AgentActivityRecord {
  const currentStep = deriveCurrentStep(activity);
  const runId = activity.runId.trim();
  const writeOutcomes = events
    ? writeOutcomesFromEvents(events, activity.runId, conversationId)
    : [];
  // Process lifecycle is mounted as conversation-owned command activity rows
  // (processActivityModel), not as generic agent-activity notice paragraphs.
  const forbiddenDisplayIds = collectRunForbiddenDisplayIds(
    activity,
    writeOutcomes,
  );

  return {
    conversationId,
    // Internal key only — projection/renderer must never surface this.
    runId,
    // Stable non-DOM identity from the real run — survives array reorder.
    recordIdentity: runId,
    // Full internal-ID carrier (receipt / embedded resource / etc.).
    ...(forbiddenDisplayIds.length > 0
      ? { forbiddenDisplayIds }
      : {}),
    status: activity.status,
    ...(currentStep ? { currentStep } : {}),
    steps: (activity.plan?.steps ?? []).map((step) => ({
      id: step.id,
      title: step.title,
      status: step.status,
    })),
    tools: activity.tools.map((tool) => mapTool(tool)),
    subagents: activity.subagents.map((agent) => mapSubagent(agent)),
    artifacts: collectPortableArtifacts(activity),
    // RunActivityView has no structured test-result DTO — never invent one.
    testResults: [],
    ...(writeOutcomes.length > 0 ? { writeOutcomes } : {}),
    notices: buildRunNotices(activity),
  };
}

/**
 * Collect durable write_proposal_resolved outcomes for one run.
 * Terminal copy is fixed and path-free at projection time.
 */
export function writeOutcomesFromEvents(
  events: readonly RuntimeEventDto[],
  runId: string,
  conversationId?: string,
): AgentWriteOutcomeInput[] {
  const outcomes: AgentWriteOutcomeInput[] = [];
  for (const event of [...events].sort((left, right) => left.sequence - right.sequence)) {
    if (
      event.runId !== runId
      || event.kind !== "write_proposal_resolved"
      || (conversationId !== undefined && event.conversationId !== conversationId)
    ) {
      continue;
    }
    const payload = event.payload as WriteProposalResolvedPayload;
    outcomes.push({
      proposalId: payload.proposal_id,
      path: payload.path,
      mode: payload.mode,
      state: payload.state,
      ...(payload.error_code ? { errorCode: payload.error_code } : {}),
    });
  }
  return outcomes;
}

/**
 * Every actual internal ID available on the run, including IDs discarded from
 * projected fields (receipt IDs, embedded artifact resource IDs). Used only as
 * a sanitizer carrier — never rendered.
 */
function collectRunForbiddenDisplayIds(
  activity: RunActivityView,
  writeOutcomes: readonly AgentWriteOutcomeInput[] = [],
): string[] {
  const ids: string[] = [];
  const push = (value: string | undefined | null) => {
    const trimmed = value?.trim();
    if (trimmed && trimmed.length > 0) ids.push(trimmed);
  };

  push(activity.runId);
  for (const outcome of writeOutcomes) {
    push(outcome.proposalId);
  }

  for (const step of activity.plan?.steps ?? []) {
    push(step.id);
  }

  for (const tool of activity.tools) {
    push(tool.toolCallId);
    push(tool.receipt?.receiptId);
    if (tool.receipt?.resource) {
      push(tool.receipt.resource.resourceId);
    }
    for (const resource of tool.resources ?? []) {
      push(resource.resourceId);
    }
  }

  for (const agent of activity.subagents) {
    push(agent.subagentId);
  }

  for (const artifact of activity.artifacts) {
    push(artifact.artifactId);
    if (artifact.resource) {
      push(artifact.resource.resourceId);
    }
  }

  // Deduplicate while preserving first-seen order.
  const seen = new Set<string>();
  const unique: string[] = [];
  for (const id of ids) {
    if (seen.has(id)) continue;
    seen.add(id);
    unique.push(id);
  }
  return unique;
}

/**
 * Resolve a portable relative path back to the matching ResourceReferenceView
 * from the run. Prefer an exact non-DOM source token when present so path
 * collisions do not open the wrong source object.
 *
 * Token rules:
 * - tool-resource: return only that exact matching tool resource
 * - run-artifact: return the artifact's embedded resource only when its
 *   portable path equals the artifact path; otherwise a minimal reference
 *   for the real artifact path
 *
 * Path-only fallback remains for legacy one-argument callbacks/records.
 */
export function resolveResourceFromActivity(
  activity: RunActivityView,
  relativePath: string,
  sourceToken?: string,
): ResourceReferenceView | null {
  if (!isPortableRelativePath(relativePath)) return null;
  const target = relativePath.trim();

  if (sourceToken && sourceToken.trim().length > 0) {
    const byToken = resolveBySourceToken(activity, target, sourceToken.trim());
    if (byToken) return byToken;
    // Unknown/stale token: fall through to safe path-only lookup.
  }

  return resolveByPathOnly(activity, target);
}

function resolveBySourceToken(
  activity: RunActivityView,
  targetPath: string,
  sourceToken: string,
): ResourceReferenceView | null {
  const toolMatch = new RegExp(
    `^${TOOL_RESOURCE_TOKEN}:(\\d+):(\\d+)$`,
  ).exec(sourceToken);
  if (toolMatch) {
    const toolIndex = Number(toolMatch[1]);
    const resourceIndex = Number(toolMatch[2]);
    const tool = activity.tools[toolIndex];
    const resource = tool?.resources?.[resourceIndex];
    if (!resource) return null;
    const path = resource.path?.trim() ?? "";
    if (!isPortableRelativePath(path)) return null;
    // Exact tool-resource identity only — do not substitute another object.
    if (path !== targetPath) return null;
    return resource;
  }

  const artifactMatch = new RegExp(
    `^${RUN_ARTIFACT_TOKEN}:(\\d+)$`,
  ).exec(sourceToken);
  if (artifactMatch) {
    const artifactIndex = Number(artifactMatch[1]);
    const artifact = activity.artifacts[artifactIndex];
    if (!artifact) return null;
    const artPath = artifact.path.trim();
    if (!isPortableRelativePath(artPath)) return null;
    // Open the real artifact path identity (caller path must match).
    if (artPath !== targetPath) return null;

    const embeddedPath = artifact.resource?.path?.trim() ?? "";
    if (
      artifact.resource
      && embeddedPath === artPath
      && isPortableRelativePath(embeddedPath)
    ) {
      return artifact.resource;
    }
    // Path-mismatched or missing embedded resource: minimal reference only.
    return minimalResourceReference(artPath, artifact.kind);
  }

  return null;
}

/**
 * Legacy path-only resolution. First portable match wins; different sources
 * that share a path may be ambiguous without a source token.
 */
function resolveByPathOnly(
  activity: RunActivityView,
  target: string,
): ResourceReferenceView | null {
  for (const tool of activity.tools) {
    for (const resource of tool.resources ?? []) {
      if (resource.path?.trim() === target && isPortableRelativePath(resource.path)) {
        return resource;
      }
    }
    const receiptResource = tool.receipt?.resource;
    if (
      receiptResource?.path?.trim() === target
      && isPortableRelativePath(receiptResource.path)
    ) {
      return receiptResource;
    }
  }

  for (const artifact of activity.artifacts) {
    const artPath = artifact.path.trim();
    if (artPath !== target || !isPortableRelativePath(artPath)) continue;

    const embeddedPath = artifact.resource?.path?.trim() ?? "";
    if (
      artifact.resource
      && embeddedPath === artPath
      && isPortableRelativePath(embeddedPath)
    ) {
      return artifact.resource;
    }
    // Do not borrow a path-mismatched embedded resource's content/label/ref.
    return minimalResourceReference(artPath, artifact.kind);
  }

  return null;
}

function mapTool(
  tool: RunActivityView["tools"][number],
): AgentToolActivityInput {
  const status = tool.status;
  const isErrorMaterial = status === "failed" || status === "cancelled";
  const rawOutput = tool.output?.trim() ? tool.output : undefined;

  const mapped: AgentToolActivityInput = {
    toolCallId: tool.toolCallId,
    name: tool.name,
    title: tool.title,
    status,
  };

  if (isErrorMaterial && rawOutput) {
    // Failed/cancelled tool output is error material → opaque-error path.
    mapped.error = rawOutput;
  } else if (rawOutput) {
    mapped.output = rawOutput;
  }

  if (tool.outputTruncated) {
    mapped.outputTruncated = true;
    mapped.omittedOutputChars = tool.omittedOutputChars ?? 0;
  }

  return mapped;
}

function mapSubagent(
  agent: RunActivityView["subagents"][number],
): AgentSubagentActivityInput {
  // SubagentActivity has title/status/progress only — never invent a role field.
  return {
    subagentId: agent.subagentId,
    title: agent.title,
    status: agent.status,
    ...(agent.progress ? { progress: agent.progress } : {}),
  };
}

function deriveCurrentStep(activity: RunActivityView): string | undefined {
  const active = activity.plan?.steps.find((step) => step.status === "active");
  const title = active?.title?.trim();
  return title && title.length > 0 ? title : undefined;
}

/**
 * Preserve all portable tool resources in tool/resource order, then all
 * portable run artifacts in run order. Do not silently deduplicate different
 * sources solely because their paths match.
 */
function collectPortableArtifacts(
  activity: RunActivityView,
): AgentArtifactActivityInput[] {
  const artifacts: AgentArtifactActivityInput[] = [];

  // Tool resources in tool order, then resource order.
  for (let toolIndex = 0; toolIndex < activity.tools.length; toolIndex += 1) {
    const tool = activity.tools[toolIndex]!;
    const resources = tool.resources ?? [];
    for (let resourceIndex = 0; resourceIndex < resources.length; resourceIndex += 1) {
      const resource = resources[resourceIndex]!;
      const path = resource.path?.trim() ?? "";
      if (!isPortableRelativePath(path)) continue;
      artifacts.push({
        artifactId: resource.resourceId,
        path,
        kind: resource.kind,
        label: resource.label,
        sourceToken: `${TOOL_RESOURCE_TOKEN}:${toolIndex}:${resourceIndex}`,
      });
    }
  }

  // Run artifacts in run order — never collapse into tool-resource entries.
  for (let artifactIndex = 0; artifactIndex < activity.artifacts.length; artifactIndex += 1) {
    const artifact = activity.artifacts[artifactIndex]!;
    const path = artifact.path.trim();
    if (!isPortableRelativePath(path)) continue;

    const embeddedPath = artifact.resource?.path?.trim() ?? "";
    const pathsMatch =
      Boolean(artifact.resource)
      && embeddedPath === path
      && isPortableRelativePath(embeddedPath);

    // When embedded resource path differs, do not borrow its label.
    const label = pathsMatch
      ? (artifact.resource?.label ?? resourceLeafName(path) ?? path)
      : (resourceLeafName(path) ?? path);

    artifacts.push({
      artifactId: artifact.artifactId,
      path,
      kind: artifact.kind,
      label,
      sourceToken: `${RUN_ARTIFACT_TOKEN}:${artifactIndex}`,
    });
  }

  return artifacts;
}

function buildRunNotices(activity: RunActivityView): AgentActivityNoticeInput[] {
  const notices: AgentActivityNoticeInput[] = [];

  if (activity.assistantTextTruncated) {
    notices.push({
      id: "notice-assistant-truncated",
      text: `回复已截断，省略 ${activity.omittedAssistantChars ?? 0} 个字符`,
    });
  }

  // Reasoning body is never exposed — only its truncation count as a summary.
  if (activity.reasoningTextTruncated) {
    notices.push({
      id: "notice-reasoning-truncated",
      text: `推理已截断，省略 ${activity.omittedReasoningChars ?? 0} 个字符`,
    });
  }

  const omittedParts: string[] = [];
  pushOmitted(omittedParts, "计划步骤", activity.omittedPlanStepCount);
  pushOmitted(omittedParts, "工具", activity.omittedToolCount);
  pushOmitted(omittedParts, "子代理", activity.omittedSubagentCount);
  pushOmitted(omittedParts, "产物", activity.omittedArtifactCount);
  pushOmitted(omittedParts, "资源", activity.omittedResourceCount);
  if (omittedParts.length > 0) {
    notices.push({
      id: "notice-items-omitted",
      text: `已省略 ${omittedParts.join("、")}`,
    });
  }

  if ((activity.omittedResourcePreviewChars ?? 0) > 0) {
    notices.push({
      id: "notice-resource-preview-truncated",
      text: `资源预览已截断，省略 ${activity.omittedResourcePreviewChars} 个字符`,
    });
  }

  return notices;
}

function pushOmitted(
  parts: string[],
  label: string,
  count: number | undefined,
): void {
  if ((count ?? 0) > 0) {
    parts.push(`${label} ${count}`);
  }
}

function minimalResourceReference(
  path: string,
  kindHint?: string,
): ResourceReferenceView {
  const kind = mapArtifactKind(kindHint);
  return {
    // Internal key only for the resource pane contract — not activity DOM text.
    resourceId: `path:${path}`,
    kind,
    label: resourceLeafName(path) ?? path,
    path,
    truncated: false,
  };
}

function mapArtifactKind(
  kind: string | undefined,
): ResourceReferenceView["kind"] {
  if (
    kind === "file"
    || kind === "directory"
    || kind === "search_results"
    || kind === "artifact"
    || kind === "receipt"
  ) {
    return kind;
  }
  return "artifact";
}

function resourceLeafName(path: string): string | null {
  const normalized = path.replace(/\\/g, "/");
  const parts = normalized.split("/").filter(Boolean);
  return parts[parts.length - 1] ?? null;
}
