/**
 * In-conversation role-dispatch entry + outcome list.
 *
 * Propose only reserves a process proposal. Approve/reject is **never** here —
 * the existing CommandApprovalBar is the sole decision surface (one authority).
 * Ungoverned (LocalCli) targets are labeled explicitly; never painted as governed.
 */

import {
  useCallback,
  useEffect,
  useId,
  useMemo,
  useState,
  type FormEvent,
} from "react";
import {
  ROLE_DISPATCH_GOVERNED_LABEL,
  ROLE_DISPATCH_PENDING_APPROVAL_NOTICE,
  ROLE_DISPATCH_UNGOVERNED_HINT,
  ROLE_DISPATCH_UNGOVERNED_LABEL,
  RoleDispatchGatewayError,
  normalizeRoleDispatchError,
  type RoleDispatchGateway,
  type RoleDispatchProposalView,
  type RoleDispatchRecordView,
} from "../../lib/roleDispatchGateway";
import type {
  RuntimeSlotRole,
  RuntimeSlotView,
  RuntimeSlotsGateway,
} from "../settings/runtimeSlotsTypes";
import "../../styles/role-dispatch.css";

export type RoleDispatchPanelProps = {
  conversationId: string | null;
  projectId: string | null;
  roleDispatchGateway?: RoleDispatchGateway;
  runtimeSlotsGateway?: RuntimeSlotsGateway;
  /**
   * Bumped when process lifecycle changes so outcomes re-list without polling.
   * Does not invent terminal state — host list is authority.
   */
  refreshToken?: number;
  /**
   * After a successful propose (pending_approval only), parent re-lists the
   * existing process approval bar. Never pass a grant or execute path here.
   */
  onProposed?: (proposal: RoleDispatchProposalView) => void;
  disabled?: boolean;
};

type TargetOption = {
  slotId: string;
  label: string;
  role: RuntimeSlotRole;
  roleCustomName: string | null;
  governance: RuntimeSlotView["governance"];
};

function roleLabel(slot: Pick<RuntimeSlotView, "role" | "roleCustomName">): string {
  if (slot.role === "controller") return "总控";
  if (slot.role === "librarian") return "信息管理员";
  return slot.roleCustomName?.trim() || "自定义";
}

function toTargetOption(slot: RuntimeSlotView): TargetOption {
  return {
    slotId: slot.slotId,
    label: `${slot.displayName}（${roleLabel(slot)}）`,
    role: slot.role,
    roleCustomName: slot.roleCustomName,
    governance: slot.governance,
  };
}

/**
 * Dispatch targets exclude the controller (host rejects self-dispatch).
 * Prefer librarian / custom slots only.
 */
export function selectDispatchTargets(
  slots: readonly RuntimeSlotView[],
): TargetOption[] {
  return slots
    .filter((slot) => slot.role !== "controller")
    .map(toTargetOption);
}

export function selectControllerSlot(
  slots: readonly RuntimeSlotView[],
): RuntimeSlotView | null {
  return slots.find((slot) => slot.role === "controller") ?? null;
}

export function governanceBadgeLabel(
  governance: RuntimeSlotView["governance"],
): string {
  return governance === "full"
    ? ROLE_DISPATCH_GOVERNED_LABEL
    : ROLE_DISPATCH_UNGOVERNED_LABEL;
}

/** Outcome row must never claim governed execution for governance=none. */
export function outcomeClaimsGoverned(
  record: Pick<
    RoleDispatchRecordView,
    "governance" | "claimedGovernedExecution"
  >,
): boolean {
  if (record.governance !== "full") return false;
  return record.claimedGovernedExecution === true;
}

export function RoleDispatchPanel({
  conversationId,
  projectId,
  roleDispatchGateway,
  runtimeSlotsGateway,
  refreshToken = 0,
  onProposed,
  disabled = false,
}: RoleDispatchPanelProps) {
  const formId = useId();
  const [open, setOpen] = useState(false);
  const [slots, setSlots] = useState<RuntimeSlotView[]>([]);
  const [slotsLoading, setSlotsLoading] = useState(false);
  const [slotsError, setSlotsError] = useState<string | null>(null);
  const [targetSlotId, setTargetSlotId] = useState("");
  const [taskText, setTaskText] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [statusMessage, setStatusMessage] = useState<string | null>(null);
  const [statusKind, setStatusKind] = useState<"pending" | "error" | null>(
    null,
  );
  const [records, setRecords] = useState<RoleDispatchRecordView[]>([]);
  const [recordsError, setRecordsError] = useState<string | null>(null);

  const controller = useMemo(() => selectControllerSlot(slots), [slots]);
  const targets = useMemo(() => selectDispatchTargets(slots), [slots]);
  const selectedTarget = useMemo(
    () => targets.find((t) => t.slotId === targetSlotId) ?? null,
    [targets, targetSlotId],
  );

  const canSubmit =
    Boolean(roleDispatchGateway)
    && Boolean(conversationId)
    && Boolean(projectId)
    && Boolean(controller)
    && Boolean(targetSlotId)
    && taskText.trim().length > 0
    && !disabled
    && !submitting;

  const loadSlots = useCallback(async () => {
    if (!runtimeSlotsGateway || !projectId) {
      setSlots([]);
      return;
    }
    setSlotsLoading(true);
    setSlotsError(null);
    try {
      const listed = await runtimeSlotsGateway.listRuntimeSlots(projectId);
      setSlots(listed);
      setTargetSlotId((current) => {
        if (current && listed.some((s) => s.slotId === current && s.role !== "controller")) {
          return current;
        }
        const first = selectDispatchTargets(listed)[0];
        return first?.slotId ?? "";
      });
    } catch {
      setSlots([]);
      setSlotsError("无法读取项目槽位。请打开设置检查角色配置。");
    } finally {
      setSlotsLoading(false);
    }
  }, [runtimeSlotsGateway, projectId]);

  const loadRecords = useCallback(async () => {
    if (!roleDispatchGateway || !conversationId) {
      setRecords([]);
      return;
    }
    try {
      const listed = await roleDispatchGateway.listRoleDispatches(conversationId);
      setRecords(listed);
      setRecordsError(null);
    } catch {
      // Never log host values — may carry sensitive paths.
      setRecordsError("无法读取派单记录。");
    }
  }, [roleDispatchGateway, conversationId]);

  useEffect(() => {
    if (!open) return;
    void loadSlots();
  }, [open, loadSlots]);

  useEffect(() => {
    if (!open) return;
    void loadRecords();
  }, [open, loadRecords, refreshToken]);

  // Clear status when conversation/project identity changes.
  useEffect(() => {
    setStatusMessage(null);
    setStatusKind(null);
    setTaskText("");
    setRecords([]);
  }, [conversationId, projectId]);

  async function handleSubmit(event: FormEvent) {
    event.preventDefault();
    if (
      !canSubmit
      || !roleDispatchGateway
      || !conversationId
      || !controller
      || !targetSlotId
    ) {
      return;
    }

    const text = taskText.trim();
    setSubmitting(true);
    setStatusMessage(null);
    setStatusKind(null);
    try {
      const proposed = await roleDispatchGateway.proposeRoleDispatch({
        conversationId,
        initiatorSlotId: controller.slotId,
        targetSlotId,
        taskText: text,
      });

      // Authority: only pending_approval is accepted by the gateway normalizer.
      // UI must never claim executed / "已派出".
      if (proposed.proposal.state !== "pending_approval") {
        setStatusKind("error");
        setStatusMessage("宿主返回了非待批准状态，已忽略，请勿当作已执行。");
        return;
      }

      setTaskText("");
      setStatusKind("pending");
      setStatusMessage(ROLE_DISPATCH_PENDING_APPROVAL_NOTICE);
      onProposed?.(proposed);
      void loadRecords();
    } catch (error) {
      const normalized =
        error instanceof RoleDispatchGatewayError
          ? error
          : normalizeRoleDispatchError(error);
      setStatusKind("error");
      setStatusMessage(normalized.message);
    } finally {
      setSubmitting(false);
    }
  }

  if (!roleDispatchGateway) {
    return null;
  }

  return (
    <details
      className="role-dispatch-panel"
      data-testid="role-dispatch-panel"
      onToggle={(event) => {
        setOpen((event.currentTarget as HTMLDetailsElement).open);
      }}
    >
      <summary
        className="role-dispatch-panel__summary"
        data-testid="role-dispatch-toggle"
      >
        派单给其他角色
      </summary>
      <div className="role-dispatch-panel__body">
        <p className="role-dispatch-panel__hint">
          选择目标角色并描述任务。提交后进入<strong>现有命令批准条</strong>
          ；未批准前不会执行。不会单独再造一套审批。
        </p>

        {slotsLoading ? (
          <p className="role-dispatch-panel__status" role="status">
            正在加载槽位…
          </p>
        ) : null}
        {slotsError ? (
          <p
            className="role-dispatch-panel__status role-dispatch-panel__status--error"
            data-testid="role-dispatch-slots-error"
            role="alert"
          >
            {slotsError}
          </p>
        ) : null}

        {!slotsLoading && !controller ? (
          <p
            className="role-dispatch-panel__status role-dispatch-panel__status--error"
            data-testid="role-dispatch-no-controller"
            role="alert"
          >
            当前项目没有总控槽位，无法发起派单。请先在设置中指定总控。
          </p>
        ) : null}

        {!slotsLoading && controller && targets.length === 0 ? (
          <p
            className="role-dispatch-panel__status role-dispatch-panel__status--error"
            data-testid="role-dispatch-no-targets"
            role="alert"
          >
            没有可派单的目标角色（总控不能作为目标）。请先创建信息管理员或其他角色槽位。
          </p>
        ) : null}

        <form
          aria-label="派单给其他角色"
          data-testid="role-dispatch-form"
          id={formId}
          onSubmit={(event) => {
            void handleSubmit(event);
          }}
        >
          <div className="role-dispatch-panel__field">
            <label
              className="role-dispatch-panel__label"
              htmlFor={`${formId}-target`}
            >
              目标角色
            </label>
            <select
              className="role-dispatch-panel__select"
              data-testid="role-dispatch-target"
              disabled={disabled || submitting || targets.length === 0}
              id={`${formId}-target`}
              onChange={(event) => setTargetSlotId(event.target.value)}
              value={targetSlotId}
            >
              {targets.length === 0 ? (
                <option value="">无可选目标</option>
              ) : null}
              {targets.map((target) => (
                <option key={target.slotId} value={target.slotId}>
                  {target.label}
                  {target.governance === "none" ? " · 不受治理" : ""}
                </option>
              ))}
            </select>
          </div>

          {selectedTarget ? (
            <div
              className="role-dispatch-panel__target-meta"
              data-testid="role-dispatch-target-meta"
            >
              <span
                className={
                  selectedTarget.governance === "full"
                    ? "role-dispatch-badge role-dispatch-badge--governed"
                    : "role-dispatch-badge role-dispatch-badge--ungoverned"
                }
                data-testid="role-dispatch-target-governance"
              >
                {governanceBadgeLabel(selectedTarget.governance)}
              </span>
              {selectedTarget.governance === "none" ? (
                <p
                  className="role-dispatch-panel__ungoverned-note"
                  data-testid="role-dispatch-ungoverned-hint"
                  role="note"
                >
                  {ROLE_DISPATCH_UNGOVERNED_HINT}
                </p>
              ) : null}
            </div>
          ) : null}

          <div className="role-dispatch-panel__field">
            <label
              className="role-dispatch-panel__label"
              htmlFor={`${formId}-task`}
            >
              任务描述
            </label>
            <textarea
              className="role-dispatch-panel__textarea"
              data-testid="role-dispatch-task"
              disabled={disabled || submitting || !controller}
              id={`${formId}-task`}
              onChange={(event) => setTaskText(event.target.value)}
              placeholder="描述要派给目标角色的工作（提交后仍需在命令批准条中批准）"
              value={taskText}
            />
          </div>

          <div className="role-dispatch-panel__actions">
            <button
              className="role-dispatch-panel__submit"
              data-testid="role-dispatch-submit"
              disabled={!canSubmit}
              type="submit"
            >
              {submitting ? "提交中…" : "提交派单提案"}
            </button>
          </div>
        </form>

        {statusMessage ? (
          <p
            className={
              statusKind === "error"
                ? "role-dispatch-panel__status role-dispatch-panel__status--error"
                : "role-dispatch-panel__status role-dispatch-panel__status--pending"
            }
            data-testid={
              statusKind === "error"
                ? "role-dispatch-error"
                : "role-dispatch-pending-notice"
            }
            role={statusKind === "error" ? "alert" : "status"}
          >
            {statusMessage}
          </p>
        ) : null}

        {/* Explicit: this panel never hosts Approve/Reject — one authority. */}
        <div
          aria-hidden="true"
          data-testid="role-dispatch-no-second-approval"
          hidden
        />

        <div
          className="role-dispatch-panel__results"
          data-testid="role-dispatch-results"
        >
          <h3 className="role-dispatch-panel__results-title">派单结果</h3>
          {recordsError ? (
            <p
              className="role-dispatch-panel__status role-dispatch-panel__status--error"
              data-testid="role-dispatch-records-error"
              role="alert"
            >
              {recordsError}
            </p>
          ) : null}
          {records.length === 0 && !recordsError ? (
            <p
              className="role-dispatch-result__empty"
              data-testid="role-dispatch-results-empty"
            >
              尚无派单记录。批准并执行后会在此显示退出码与治理标注。
            </p>
          ) : null}
          {records.map((record, index) => {
            const governed = outcomeClaimsGoverned(record);
            const ungoverned = record.governance === "none";
            return (
              <article
                className="role-dispatch-result"
                data-governance={record.governance}
                data-testid="role-dispatch-result-row"
                // assignmentId is internal; index is stable for list paint only.
                key={`${record.callId}-${index}`}
              >
                <div className="role-dispatch-result__meta">
                  <span
                    className={
                      ungoverned
                        ? "role-dispatch-badge role-dispatch-badge--ungoverned"
                        : governed
                          ? "role-dispatch-badge role-dispatch-badge--governed"
                          : "role-dispatch-badge role-dispatch-badge--governed"
                    }
                    data-testid="role-dispatch-result-governance"
                  >
                    {ungoverned
                      ? ROLE_DISPATCH_UNGOVERNED_LABEL
                      : governed
                        ? ROLE_DISPATCH_GOVERNED_LABEL
                        : ROLE_DISPATCH_GOVERNED_LABEL}
                  </span>
                  {record.exitCode !== null ? (
                    <span data-testid="role-dispatch-result-exit">
                      退出 {record.exitCode}
                    </span>
                  ) : (
                    <span data-testid="role-dispatch-result-pending">
                      尚未产生退出码（可能仍待批准或运行中）
                    </span>
                  )}
                  {record.durationMs !== null ? (
                    <span data-testid="role-dispatch-result-duration">
                      {record.durationMs} ms
                    </span>
                  ) : null}
                </div>
                {ungoverned ? (
                  <p
                    className="role-dispatch-panel__ungoverned-note"
                    data-testid="role-dispatch-result-ungoverned-note"
                    role="note"
                  >
                    {ROLE_DISPATCH_UNGOVERNED_HINT}
                  </p>
                ) : null}
                {ungoverned && record.claimedGovernedExecution === true ? (
                  // Gateway rejects this shape; belt-and-suspenders for honesty.
                  <p
                    className="role-dispatch-panel__status role-dispatch-panel__status--error"
                    data-testid="role-dispatch-result-elevation-blocked"
                    role="alert"
                  >
                    记录声称受治理执行，但目标为不受治理 — 已按不受治理显示。
                  </p>
                ) : null}
              </article>
            );
          })}
        </div>
      </div>
    </details>
  );
}
