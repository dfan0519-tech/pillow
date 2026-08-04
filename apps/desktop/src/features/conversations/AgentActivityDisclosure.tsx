/**
 * In-conversation agent activity disclosure (Wave C2-C2).
 * Progressive disclosure of conversation-owned run/tool/sub-agent activity.
 * Does not poll global background tasks or fabricate progress.
 */

import { useId } from "react";
import { projectAgentActivity, isPortableRelativePath } from "./agentActivityProjection";
import type {
  AgentActivityDisclosureProps,
  AgentActivityProjection,
} from "./agentActivityTypes";
import "../../styles/agent-activity.css";

export type { AgentActivityDisclosureProps } from "./agentActivityTypes";

export function AgentActivityDisclosure({
  conversationId,
  activity,
  onOpenArtifact,
}: AgentActivityDisclosureProps) {
  const projections = projectAgentActivity(conversationId, activity);
  if (projections.length === 0) return null;

  const headingId = useId();

  return (
    <section
      aria-labelledby={headingId}
      className="agent-activity"
      data-testid="agent-activity-disclosure"
    >
      <h2 className="agent-activity__sr-only" id={headingId}>
        代理活动
      </h2>
      <ul className="agent-activity__list">
        {projections.map((projection) => (
          <li
            className="agent-activity__item"
            data-status={projection.status}
            // React-internal only — never text, attributes, titles, or test IDs.
            key={projection.recordKey}
          >
            <ActivityDisclosureItem
              onOpenArtifact={onOpenArtifact}
              projection={projection}
            />
          </li>
        ))}
      </ul>
    </section>
  );
}

function ActivityDisclosureItem({
  projection,
  onOpenArtifact,
}: {
  projection: AgentActivityProjection;
  onOpenArtifact?: (relativePath: string, sourceToken?: string) => void;
}) {
  const hasBody =
    projection.steps.length > 0
    || projection.tools.length > 0
    || projection.subagents.length > 0
    || projection.artifacts.length > 0
    || projection.testResults.length > 0
    || projection.notices.length > 0
    || Boolean(projection.errorMessage);

  return (
    <>
    <details
      className={
        projection.conciseSuccess
          ? "agent-activity__details agent-activity__details--concise"
          : "agent-activity__details"
      }
      data-testid="agent-activity-details"
    >
      <summary
        className="agent-activity__summary"
        data-testid="agent-activity-summary"
      >
        <span
          aria-hidden="true"
          className={
            !projection.isTerminal
              ? "agent-activity__dot agent-activity__dot--live"
              : projection.isFailureLike
                ? "agent-activity__dot agent-activity__dot--risk"
                : "agent-activity__dot"
          }
        />
        <span className="agent-activity__summary-text">
          {projection.summaryLine}
        </span>
        <span className="agent-activity__status" data-status={projection.status}>
          {projection.statusLabel}
        </span>
      </summary>

      {hasBody ? (
        <div className="agent-activity__body">
          {projection.errorMessage ? (
            <p
              className="agent-activity__error"
              data-testid="agent-activity-error"
              role="alert"
            >
              {projection.errorMessage}
            </p>
          ) : null}

          {projection.notices.length > 0 ? (
            <div
              className="agent-activity__block"
              data-testid="agent-activity-notices"
            >
              {projection.notices.map((notice) => (
                <p
                  className="agent-activity__notice"
                  // React key only — never data-* or visible id.
                  key={notice.key}
                >
                  {notice.text}
                </p>
              ))}
            </div>
          ) : null}

          {projection.steps.length > 0 ? (
            <div
              className="agent-activity__block"
              data-testid="agent-activity-steps"
            >
              <h3 className="agent-activity__block-title">步骤</h3>
              <ol className="agent-activity__ordered">
                {projection.steps.map((step) => (
                  <li data-status={step.status} key={step.key}>
                    <span className="agent-activity__row-title">{step.title}</span>
                    <span className="agent-activity__row-meta">
                      {step.statusLabel}
                    </span>
                  </li>
                ))}
              </ol>
            </div>
          ) : null}

          {projection.tools.length > 0 ? (
            <div
              className="agent-activity__block"
              data-testid="agent-activity-tools"
            >
              <h3 className="agent-activity__block-title">工具</h3>
              <ul className="agent-activity__rows">
                {projection.tools.map((tool) => (
                  <li data-status={tool.status} key={tool.key}>
                    <span className="agent-activity__row-title">{tool.title}</span>
                    <span className="agent-activity__row-meta">
                      {tool.statusLabel}
                    </span>
                    {tool.output ? (
                      <details className="agent-activity__nested">
                        <summary className="agent-activity__nested-summary">
                          输出
                        </summary>
                        <pre
                          className="agent-activity__nested-body"
                          tabIndex={0}
                        >
                          {tool.output}
                        </pre>
                      </details>
                    ) : null}
                    {tool.truncationNotice ? (
                      <p className="agent-activity__notice">
                        {tool.truncationNotice}
                      </p>
                    ) : null}
                  </li>
                ))}
              </ul>
            </div>
          ) : null}

          {projection.subagents.length > 0 ? (
            <div
              className="agent-activity__block"
              data-testid="agent-activity-subagents"
            >
              <h3 className="agent-activity__block-title">子代理</h3>
              <ul className="agent-activity__rows">
                {projection.subagents.map((agent) => (
                  <li data-status={agent.status} key={agent.key}>
                    <span className="agent-activity__row-title">
                      {agent.title}
                    </span>
                    <span className="agent-activity__row-meta">
                      {agent.statusLabel}
                    </span>
                    {agent.progress ? (
                      <p className="agent-activity__row-detail">{agent.progress}</p>
                    ) : null}
                  </li>
                ))}
              </ul>
            </div>
          ) : null}

          {projection.artifacts.length > 0 ? (
            <details className="agent-activity__block agent-activity__nested">
              <summary
                className="agent-activity__nested-summary"
                data-testid="agent-activity-artifacts-toggle"
              >
                产物
              </summary>
              <ul
                className="agent-activity__rows"
                data-testid="agent-activity-artifacts"
              >
                {projection.artifacts.map((artifact) => (
                  <li key={artifact.key}>
                    <span className="agent-activity__row-title">
                      {artifact.label}
                    </span>
                    {onOpenArtifact ? (
                      <button
                        className="agent-activity__artifact-btn"
                        onClick={() => {
                          if (!isPortableRelativePath(artifact.relativePath)) {
                            return;
                          }
                          // Pass sourceToken only in-memory; never as DOM attr/text.
                          if (artifact.sourceToken) {
                            onOpenArtifact(
                              artifact.relativePath,
                              artifact.sourceToken,
                            );
                          } else {
                            onOpenArtifact(artifact.relativePath);
                          }
                        }}
                        type="button"
                      >
                        打开 {artifact.label}
                      </button>
                    ) : null}
                  </li>
                ))}
              </ul>
            </details>
          ) : null}

          {projection.testResults.length > 0 ? (
            <details className="agent-activity__block agent-activity__nested">
              <summary className="agent-activity__nested-summary">
                测试结果
              </summary>
              <ul
                className="agent-activity__rows"
                data-testid="agent-activity-tests"
              >
                {projection.testResults.map((result) => (
                  <li data-status={result.status} key={result.key}>
                    <span className="agent-activity__row-title">
                      {result.name}
                    </span>
                    <span className="agent-activity__row-meta">
                      {result.statusLabel}
                    </span>
                    {result.summary ? (
                      <p className="agent-activity__row-detail">
                        {result.summary}
                      </p>
                    ) : null}
                  </li>
                ))}
              </ul>
            </details>
          ) : null}
        </div>
      ) : null}
    </details>
    {projection.writeOutcomes.length > 0 ? (
      <ul
        className="write-outcome-list"
        data-testid="write-outcome-list"
      >
        {projection.writeOutcomes.map((outcome) => (
          <li
            className="write-outcome"
            data-status={outcome.state}
            data-testid="write-outcome-row"
            key={outcome.key}
          >
            <p className="write-outcome__message">{outcome.message}</p>
            {outcome.canOpenFile
            && outcome.relativePath
            && onOpenArtifact ? (
              <button
                className="write-outcome__open"
                data-testid="write-outcome-open-file"
                onClick={() => {
                  if (!isPortableRelativePath(outcome.relativePath!)) return;
                  onOpenArtifact(outcome.relativePath!);
                }}
                type="button"
              >
                打开文件
              </button>
            ) : null}
          </li>
        ))}
      </ul>
    ) : null}
    </>
  );
}
