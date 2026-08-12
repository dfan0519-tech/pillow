/**
 * Permission settings copy — two independent axes (approval vs access range).
 * Keep free of "全开 = unbounded" language; deny / receipt claims stay explicit.
 */

import type { PermissionModeLabel } from "./settingsTypes";

/** Primary two-axis claim (security category banner). */
export const PERMISSION_TWO_AXIS_NOTE =
  "权限模式只决定要不要问你；文件默认仍只能动当前项目目录。要动项目外，必须另开范围。";

/** Deny-priority + durable receipt claim (kept from prior UI). */
export const PERMISSION_DENY_RECEIPT_NOTE =
  "deny 规则始终优先；自动批准仍会生成提案、执行记录与持久收据。";

/** Global layer: whether Bypass may be selected at all. */
export const BYPASS_AVAILABLE_LABEL = "允许使用 Bypass";

export const BYPASS_AVAILABLE_HELP =
  "全局层：决定 Bypass 是否可用。关闭后，任何项目都不能选择 Bypass；项目不能突破全局。";

export const BYPASS_PROJECT_LOCKED_NOTE =
  "全局已关闭 Bypass，此项目不可选择该模式。请先在全局层开启「允许使用 Bypass」。";

export const PROJECT_MODE_NO_PROJECT_NOTE =
  "打开项目后可在项目层选择权限模式。全局层只控制 Bypass 是否可用。";

export const PROJECT_MODE_HELP =
  "项目层：在全局允许的前提下选择本项目的批准策略。有效模式 = 全局允许 Bypass ∧ 项目选择。";

export type PermissionModeOption = {
  value: PermissionModeLabel;
  /** Short select label — must not imply unbounded file access. */
  label: string;
  /** One-line: what this mode controls and what it does not. */
  detail: string;
};

/**
 * Mode options. Labels describe approval only; each detail repeats the
 * project-boundary claim so "auto-approve" is never read as "no boundaries".
 */
export const PERMISSION_MODE_OPTIONS: readonly PermissionModeOption[] = [
  {
    value: "default",
    label: "默认：写入与命令都询问",
    detail: "每次写入与命令都询问。只改「要不要问你」，不放宽项目目录边界。",
  },
  {
    value: "accept_edits",
    label: "接受改动：写入自动，命令询问",
    detail:
      "写入自动批准，命令仍询问。只改「要不要问你」，文件默认仍只能动当前项目目录。",
  },
  {
    value: "bypass",
    label: "Bypass：写入与命令都自动批准",
    detail:
      "写入与命令都自动批准（仍生成收据；deny 优先）。只改「要不要问你」，不解除项目目录边界。",
  },
] as const;

export function permissionModeOption(
  mode: PermissionModeLabel,
): PermissionModeOption {
  return (
    PERMISSION_MODE_OPTIONS.find((option) => option.value === mode)
    ?? PERMISSION_MODE_OPTIONS[0]!
  );
}
