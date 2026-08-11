/**
 * Guidance when conversation fails because no governed runtime is active.
 *
 * Host code: `runtime_unavailable` ("Conversation runtime is unavailable.
 * Start a governed runtime slot."). Renderer must not invent success — only
 * explain the gap and send the user to Settings → runtime slots.
 *
 * Two actionable cases (next step differs):
 * - no governed slots configured → create + start
 * - governed slots exist but none started → start
 *
 * `external_cli` never counts as a chat-capable slot (ungoverned, cannot start).
 */

import type { RuntimeSlotView } from "./runtimeSlotsTypes";

/** Short title for the shell banner. */
export const RUNTIME_SLOT_GUIDANCE_TITLE = "还没有可用的对话运行时";

/**
 * Zero governed slots (empty project, or only external CLI registration).
 * Next step: create a governed Grok slot, then start it.
 */
export const RUNTIME_SLOT_GUIDANCE_NO_SLOTS =
  "当前项目还没有可启动的代理槽位。请打开设置 → 运行时槽位，新增一个受治理的 Grok 槽位（已登录的 Grok，或自有 API key + 端点 + 模型），然后点「启动」。外部 CLI 仅供登记，不能用来对话。";

/**
 * At least one governed slot exists, but conversation runtime is still down.
 * Next step: start an existing governed slot (not create-first).
 */
export const RUNTIME_SLOT_GUIDANCE_NOT_STARTED =
  "当前项目已有代理槽位，但还没有启动。请打开设置 → 运行时槽位，在受治理的 Grok 槽位上点「启动」，激活后再发消息。";

/** CTA shared with other shell banners. */
export const RUNTIME_SLOT_GUIDANCE_OPEN_SETTINGS = "打开设置";

export type RuntimeSlotGuidanceKind = "no_slots" | "not_started";

/** Phrases / codes that mean the conversation runtime is not active. */
const RUNTIME_UNAVAILABLE_PHRASES: readonly string[] = [
  "runtime_unavailable",
  "conversation runtime is unavailable",
  "start a governed runtime slot",
];

function asciiLower(text: string): string {
  return text.toLowerCase();
}

/**
 * True when a send/create failure is the no-active-runtime class.
 * Phrase/code match only — never inspects secrets or machine paths.
 */
export function looksLikeRuntimeUnavailable(
  text: string | null | undefined,
): boolean {
  if (typeof text !== "string" || text.trim().length === 0) return false;
  const lower = asciiLower(text);
  return RUNTIME_UNAVAILABLE_PHRASES.some((phrase) => lower.includes(phrase));
}

/**
 * Classify project slots for the two user-facing next steps.
 * Only `governed_grok` + `governance: "full"` can be started for chat.
 * External CLI alone → treat as no chat-capable slots.
 */
export function classifyRuntimeSlotGuidance(
  slots: readonly RuntimeSlotView[],
): RuntimeSlotGuidanceKind {
  const governed = slots.filter(
    (slot) =>
      slot.kind === "governed_grok" && slot.governance === "full",
  );
  return governed.length === 0 ? "no_slots" : "not_started";
}

/** Map classification to fixed user-facing body copy. */
export function userFacingRuntimeSlotGuidance(
  kind: RuntimeSlotGuidanceKind,
): string {
  return kind === "no_slots"
    ? RUNTIME_SLOT_GUIDANCE_NO_SLOTS
    : RUNTIME_SLOT_GUIDANCE_NOT_STARTED;
}
