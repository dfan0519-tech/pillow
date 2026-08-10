/**
 * Compact conversation-owned command activity row (C4.0).
 * Progressive disclosure only; no Tauri, fetch, persist or authority.
 */

import type { CommandActivityView, CommandState } from "./executionTypes";
import {
  formatCommandArgvDisplay,
  isShellProgram,
} from "./executionTypes";
import "../../styles/execution.css";

export type CommandActivityRowProps = {
  activity: CommandActivityView;
  /** Opens contextual Terminal output (L2). Parent owns host load. */
  onOpenOutput?: () => void;
};

function originLabel(origin: CommandActivityView["origin"]): string {
  return origin === "user" ? "用户" : "代理";
}

function statusLabel(state: CommandState): string {
  switch (state) {
    case "pending_approval":
      return "待批准";
    case "approved":
      return "已批准";
    case "running":
      return "运行中";
    case "succeeded":
      return "已成功";
    case "failed":
      return "已失败";
    case "rejected":
      return "已拒绝";
    case "expired":
      return "已过期";
    case "cancelled":
      return "已取消";
    case "interrupted":
      return "已中断";
  }
}

export function CommandActivityRow({
  activity,
  onOpenOutput,
}: CommandActivityRowProps) {
  const isShell = isShellProgram(activity.programLabel);
  const argvText = formatCommandArgvDisplay(
    activity.programLabel,
    activity.argv,
  );
  const hasTail =
    (activity.outputTail?.length ?? 0) > 0
    || Boolean(activity.truncated)
    || Boolean(activity.encodingLossy);
  const isFailureLike =
    activity.state === "failed"
    || activity.state === "rejected"
    || activity.state === "expired"
    || activity.state === "cancelled"
    || activity.state === "interrupted";
  const isLive =
    activity.state === "pending_approval"
    || activity.state === "approved"
    || activity.state === "running";

  return (
    <article
      className="command-activity-row"
      data-command-kind={isShell ? "shell" : "structured"}
      data-origin={activity.origin}
      data-status={activity.state}
      data-testid="command-activity-row"
    >
      <details
        className={
          activity.state === "succeeded" && !isFailureLike
            ? "command-activity-row__details command-activity-row__details--concise"
            : "command-activity-row__details"
        }
        data-testid="command-activity-details"
      >
        <summary
          className="command-activity-row__summary"
          data-testid="command-activity-summary"
        >
          <span
            aria-hidden="true"
            className={
              isLive
                ? "command-activity-row__dot command-activity-row__dot--live"
                : isFailureLike
                  ? "command-activity-row__dot command-activity-row__dot--risk"
                  : "command-activity-row__dot"
            }
          />
          <span
            className="command-activity-row__origin"
            data-testid="command-activity-origin"
          >
            {originLabel(activity.origin)}
          </span>
          <span
            className="command-activity-row__status"
            data-testid="command-activity-status"
          >
            {statusLabel(activity.state)}
          </span>
          {isShell ? (
            <span
              className="command-activity-row__kind"
              data-testid="command-activity-kind"
            >
              Shell 命令
            </span>
          ) : (
            <code
              className="command-activity-row__program"
              data-testid="command-activity-program"
            >
              {activity.programLabel}
            </code>
          )}
          <code
            className="command-activity-row__argv"
            data-full-command={argvText}
            data-testid="command-activity-argv"
            title={argvText}
          >
            {argvText}
          </code>
          {activity.exitCode !== undefined && activity.exitCode !== null ? (
            <span
              className="command-activity-row__meta"
              data-testid="command-activity-exit"
            >
              退出 {activity.exitCode}
            </span>
          ) : null}
          {activity.durationMs !== undefined && activity.durationMs !== null ? (
            <span
              className="command-activity-row__meta"
              data-testid="command-activity-duration"
            >
              {activity.durationMs} ms
            </span>
          ) : null}
        </summary>

        {hasTail || onOpenOutput ? (
          <div className="command-activity-row__body">
            {activity.outputTail && activity.outputTail.length > 0 ? (
              <pre
                className="command-activity-row__tail"
                data-testid="command-activity-tail"
                tabIndex={0}
              >
                {activity.outputTail.join("\n")}
              </pre>
            ) : null}
            {activity.truncated ? (
              <p
                className="command-activity-row__notice"
                data-testid="command-activity-truncated"
                role="status"
              >
                输出已截断
              </p>
            ) : null}
            {activity.encodingLossy ? (
              <p
                className="command-activity-row__notice"
                data-testid="command-activity-lossy"
                role="status"
              >
                编码有损
              </p>
            ) : null}
            {onOpenOutput ? (
              <button
                className="command-activity-row__open"
                data-testid="command-activity-open-output"
                onClick={onOpenOutput}
                type="button"
              >
                完整输出
              </button>
            ) : null}
          </div>
        ) : null}
      </details>
    </article>
  );
}
