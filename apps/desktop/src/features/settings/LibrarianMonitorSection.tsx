import { useCallback, useEffect, useRef, useState } from "react";
import { TauriLibrarianAuditGateway } from "./librarianAuditGateway";
import { TauriRuntimeSlotsGateway } from "./runtimeSlotsGateway";
import type {
  LibrarianAuditCheckId,
  LibrarianAuditGateway,
  LibrarianAuditReport,
} from "./librarianAuditTypes";
import type { RuntimeSlotsGateway } from "./runtimeSlotsTypes";
import "../../styles/librarian-monitor.css";

export type LibrarianMonitorSectionProps = {
  projectId: string | null;
  auditGateway?: LibrarianAuditGateway;
  runtimeSlotsGateway?: RuntimeSlotsGateway;
};

type LibrarianAssignmentState =
  | { kind: "loading" }
  | { kind: "configured"; names: string[] }
  | { kind: "unconfigured" }
  | { kind: "unavailable" }
  | { kind: "no_project" };

const CHECK_LABELS: Readonly<Record<LibrarianAuditCheckId, string>> = {
  inflight_handoff_consistency: "在途计数与交接一致",
  pending_acceptance_visibility: "待验收任务可见",
  handoff_ledger_references: "交接任务均有台账记录",
  inflight_inventory: "在途清单可枚举",
};

const AUDIT_FAILURE_MESSAGE = "本次检查失败。未显示上一次的结果。";

function defaultRuntimeSlotsGateway(): RuntimeSlotsGateway | null {
  if (
    typeof window !== "undefined"
    && ("__TAURI_INTERNALS__" in window || "__TAURI__" in window)
  ) {
    return new TauriRuntimeSlotsGateway();
  }
  return null;
}

function defaultAuditGateway(): LibrarianAuditGateway | null {
  if (
    typeof window !== "undefined"
    && ("__TAURI_INTERNALS__" in window || "__TAURI__" in window)
  ) {
    return new TauriLibrarianAuditGateway();
  }
  return null;
}

export function LibrarianMonitorSection({
  projectId,
  auditGateway,
  runtimeSlotsGateway,
}: LibrarianMonitorSectionProps) {
  const [auditClient] = useState<LibrarianAuditGateway | null>(
    () => auditGateway ?? defaultAuditGateway(),
  );
  const [slotsGateway] = useState<RuntimeSlotsGateway | null>(
    () => runtimeSlotsGateway ?? defaultRuntimeSlotsGateway(),
  );
  const auditRequest = useRef(0);
  const [report, setReport] = useState<LibrarianAuditReport | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [assignment, setAssignment] = useState<LibrarianAssignmentState>(
    projectId ? { kind: "loading" } : { kind: "no_project" },
  );

  const runAudit = useCallback(async () => {
    const request = ++auditRequest.current;
    setReport(null);
    setError(null);
    if (!projectId || !auditClient) {
      setLoading(false);
      return;
    }

    setLoading(true);
    try {
      const next = await auditClient.runLibrarianAudit(projectId);
      if (request === auditRequest.current) setReport(next);
    } catch {
      if (request === auditRequest.current) setError(AUDIT_FAILURE_MESSAGE);
    } finally {
      if (request === auditRequest.current) setLoading(false);
    }
  }, [auditClient, projectId]);

  useEffect(() => {
    void runAudit();
    return () => {
      auditRequest.current += 1;
    };
  }, [runAudit]);

  useEffect(() => {
    let current = true;
    if (!projectId) {
      setAssignment({ kind: "no_project" });
      return () => {
        current = false;
      };
    }
    if (!slotsGateway) {
      setAssignment({ kind: "unavailable" });
      return () => {
        current = false;
      };
    }

    setAssignment({ kind: "loading" });
    void slotsGateway.listRuntimeSlots(projectId).then(
      (slots) => {
        if (!current) return;
        const names = slots
          .filter((slot) => slot.role === "librarian")
          .map((slot) => slot.displayName.trim())
          .filter((name) => name.length > 0);
        setAssignment(
          names.length > 0
            ? { kind: "configured", names }
            : { kind: "unconfigured" },
        );
      },
      () => {
        if (current) setAssignment({ kind: "unavailable" });
      },
    );
    return () => {
      current = false;
    };
  }, [projectId, slotsGateway]);

  return (
    <section
      aria-labelledby="librarian-monitor-heading"
      className="librarian-monitor"
      data-testid="librarian-monitor-section"
    >
      <div className="librarian-monitor__head">
        <div>
          <h3 id="librarian-monitor-heading">项目体检</h3>
          <p>零 token 核对项目交接与真实台账；打开设置时自动检查。</p>
        </div>
        <button
          disabled={loading || !projectId || !auditClient}
          onClick={() => void runAudit()}
          type="button"
        >
          {loading ? "检查中…" : "重新检查"}
        </button>
      </div>
      {assignment.kind === "loading" ? (
        <p className="librarian-monitor__assignment" role="status">
          正在确认信息管理员…
        </p>
      ) : assignment.kind === "configured" ? (
        <p
          className="librarian-monitor__assignment"
          data-testid="librarian-monitor-responsibility"
        >
          由 {assignment.names.join("、")} 负责（信息管理员）。
        </p>
      ) : assignment.kind === "unconfigured" ? (
        <p
          className="librarian-monitor__assignment librarian-monitor__assignment--guide"
          data-testid="librarian-monitor-no-role"
          role="note"
        >
          尚未设置信息管理员。请在下方运行时槽位中新增或调整角色；
          体检仍然可用，不依赖角色槽位。
        </p>
      ) : assignment.kind === "unavailable" ? (
        <p
          className="librarian-monitor__assignment"
          data-testid="librarian-monitor-role-unavailable"
          role="status"
        >
          暂时无法读取信息管理员配置；这不影响项目体检。
        </p>
      ) : (
        <p className="librarian-monitor__assignment" role="status">
          打开项目后可运行体检并确认负责角色。
        </p>
      )}
      {loading ? <p role="status">正在检查项目…</p> : null}
      {error ? (
        <p aria-label={error} role="alert">
          {error}
        </p>
      ) : null}
      {!loading && !error && !report && projectId && !auditClient ? (
        <p data-testid="librarian-monitor-host-unavailable" role="status">
          宿主尚未提供体检结果，当前没有可展示的检查状态。
        </p>
      ) : null}
      {report ? (
        <ul className="librarian-monitor__checks">
          {report.checks.map((check) => (
            <li
              className="librarian-monitor__check"
              data-status={check.status}
              data-testid={`librarian-check-${check.id}`}
              key={check.id}
            >
              <div className="librarian-monitor__check-head">
                <strong>{CHECK_LABELS[check.id]}</strong>
                <span className="librarian-monitor__badge">
                  {check.status.toUpperCase()}
                </span>
              </div>
              <p>{check.summary}</p>
              {check.status === "fail" ? (
                <details className="librarian-monitor__findings">
                  <summary>查看 {check.findings.length} 条问题</summary>
                  <ul>
                    {check.findings.map((finding, index) => (
                      <li key={`${finding.code}-${index}`}>
                        <code>{finding.code}</code>
                        <span>{finding.summary}</span>
                      </li>
                    ))}
                  </ul>
                </details>
              ) : null}
            </li>
          ))}
        </ul>
      ) : null}
    </section>
  );
}
