/**
 * Bounded Terminal-output surface for the contextual pane (C4.0).
 * Not an interactive shell — displays host-sanitized chunks only.
 */

import type { TerminalOutputModel } from "./executionTypes";
import {
  formatCommandArgvDisplay,
  isShellProgram,
} from "./executionTypes";
import "../../styles/execution.css";

export type TerminalOutputViewProps = {
  output: TerminalOutputModel;
};

export function TerminalOutputView({ output }: TerminalOutputViewProps) {
  // Host-supplied chunks only — never reconstruct transcript from metadata.
  const body = [...output.chunks]
    .sort((a, b) => a.sequence - b.sequence)
    .map((chunk) => chunk.text)
    .join("");
  const isShell = isShellProgram(output.programLabel);
  const commandText = formatCommandArgvDisplay(
    output.programLabel,
    output.argv,
  );

  return (
    <section
      aria-label="终端输出"
      className="terminal-output-view"
      data-command-kind={isShell ? "shell" : "structured"}
      data-status={output.state}
      data-live-retained={output.liveRetained ? "true" : "false"}
      data-testid="terminal-output-view"
    >
      <header className="terminal-output-view__header">
        <h3 className="terminal-output-view__title">终端输出</h3>
        <p className="terminal-output-view__command">
          {isShell ? (
            <span
              className="terminal-output-view__kind"
              data-testid="terminal-output-kind"
            >
              Shell 命令
            </span>
          ) : (
            <code data-testid="terminal-output-program">{output.programLabel}</code>
          )}
          <span aria-hidden="true" className="terminal-output-view__sep">
            {" "}
          </span>
          <code
            data-full-command={commandText}
            data-testid="terminal-output-argv"
            title={commandText}
          >
            {commandText}
          </code>
        </p>
        <p className="terminal-output-view__meta">
          <span data-testid="terminal-output-bytes">
            {output.capturedBytes} 字节
          </span>
          {output.exitCode !== undefined && output.exitCode !== null ? (
            <>
              <span aria-hidden="true" className="terminal-output-view__sep">
                ·
              </span>
              <span data-testid="terminal-output-exit">
                退出 {output.exitCode}
              </span>
            </>
          ) : null}
          {output.durationMs !== undefined && output.durationMs !== null ? (
            <>
              <span aria-hidden="true" className="terminal-output-view__sep">
                ·
              </span>
              <span data-testid="terminal-output-duration">
                {output.durationMs} ms
              </span>
            </>
          ) : null}
        </p>
        {!output.liveRetained ? (
          <p
            className="terminal-output-view__notice"
            data-testid="terminal-output-durable-only"
            role="status"
          >
            实时输出未保留，仅显示持久化摘要
          </p>
        ) : null}
        {output.truncated ? (
          <p
            className="terminal-output-view__notice"
            data-testid="terminal-output-truncated"
            role="status"
          >
            输出已截断
          </p>
        ) : null}
        {output.encodingLossy ? (
          <p
            className="terminal-output-view__notice"
            data-testid="terminal-output-lossy"
            role="status"
          >
            编码有损
          </p>
        ) : null}
      </header>
      <pre
        className="terminal-output-view__body"
        data-testid="terminal-output-body"
        tabIndex={0}
      >
        {body}
      </pre>
    </section>
  );
}
