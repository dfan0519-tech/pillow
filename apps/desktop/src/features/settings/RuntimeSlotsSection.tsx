/**
 * Project-scoped runtime slots: create, edit (API-key keep-on-blank), probe.
 * Secrets never enter localStorage / sessionStorage / URL; drafts clear on save.
 * Host errors are never console-logged (may carry secret-shaped detail).
 */

import { useCallback, useEffect, useState, type FormEvent } from "react";
import { RuntimeSlotModelPicker } from "./RuntimeSlotModelPicker";
import {
  MockRuntimeSlotsGateway,
  TauriRuntimeSlotsGateway,
  classifyExternalCliStatus,
  mapGrokProbeFailureMessage,
} from "./runtimeSlotsGateway";
import {
  EXTERNAL_CLI_UNGOVERNED_BANNER,
  EXTERNAL_CLI_UNGOVERNED_BODY,
  EXTERNAL_CLI_UNGOVERNED_HEADING,
  GOVERNED_GROK_BADGE,
  SECRET_CONFIGURED_LABEL,
  SECRET_NOT_CONFIGURED_LABEL,
  type GrokSlotAuthMode,
  type RuntimeSlotKind,
  type RuntimeSlotModelSwitchView,
  type RuntimeSlotView,
  type RuntimeSlotsGateway,
  RuntimeSlotsGatewayError,
} from "./runtimeSlotsTypes";
import "../../styles/runtime-slots.css";

export type RuntimeSlotsSectionProps = {
  projectId: string | null;
  gateway?: RuntimeSlotsGateway;
};

type FormMode = "closed" | "create" | "edit";

type CreateKind = RuntimeSlotKind;
type CreateAuth = GrokSlotAuthMode;

type ProbeResult = {
  slotId: string;
  ok: boolean;
  text: string;
};

function defaultSlotsGateway(): RuntimeSlotsGateway {
  if (
    typeof window !== "undefined"
    && ("__TAURI_INTERNALS__" in window || "__TAURI__" in window)
  ) {
    return new TauriRuntimeSlotsGateway();
  }
  return new MockRuntimeSlotsGateway();
}

/** Split argv fields: whitespace-separated; empty → []. */
export function parseArgvField(value: string): string[] {
  return value
    .trim()
    .split(/\s+/u)
    .filter((part) => part.length > 0);
}

export function RuntimeSlotsSection({
  projectId,
  gateway: gatewayProp,
}: RuntimeSlotsSectionProps) {
  const [gateway] = useState<RuntimeSlotsGateway>(
    () => gatewayProp ?? defaultSlotsGateway(),
  );
  const [slots, setSlots] = useState<RuntimeSlotView[]>([]);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [probe, setProbe] = useState<ProbeResult | null>(null);
  /**
   * Last successfully started governed slot in this panel session.
   * Host list DTO has no active flag — session-local only so Start stays
   * obvious for slots that have never been started here.
   */
  const [startedSlotId, setStartedSlotId] = useState<string | null>(null);

  const [formMode, setFormMode] = useState<FormMode>("closed");
  const [editingSlotId, setEditingSlotId] = useState<string | null>(null);
  const [createKind, setCreateKind] = useState<CreateKind>("governed_grok");
  const [createAuth, setCreateAuth] = useState<CreateAuth>("api_key");
  const [displayName, setDisplayName] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [modelId, setModelId] = useState("");
  /** Draft only — never persisted; cleared after successful save. */
  const [apiKeyDraft, setApiKeyDraft] = useState("");
  const [executablePath, setExecutablePath] = useState("");
  const [launchArgsText, setLaunchArgsText] = useState("");
  const [statusArgsText, setStatusArgsText] = useState("");

  const reload = useCallback(async () => {
    if (!projectId) {
      setSlots([]);
      setLoading(false);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const list = await gateway.listRuntimeSlots(projectId);
      setSlots(list);
    } catch (cause) {
      // Never log the caught value — host errors may carry secrets.
      const message =
        cause instanceof Error ? cause.message : "加载运行时槽位失败。";
      setError(message);
    } finally {
      setLoading(false);
    }
  }, [gateway, projectId]);

  useEffect(() => {
    setStartedSlotId(null);
    void reload();
  }, [reload]);

  function resetFormDrafts(): void {
    setDisplayName("");
    setBaseUrl("");
    setModelId("");
    setApiKeyDraft("");
    setExecutablePath("");
    setLaunchArgsText("");
    setStatusArgsText("");
    setEditingSlotId(null);
    setCreateKind("governed_grok");
    setCreateAuth("api_key");
  }

  function openCreate(): void {
    resetFormDrafts();
    setFormMode("create");
    setNotice(null);
    setError(null);
    setProbe(null);
  }

  function openEdit(slot: RuntimeSlotView): void {
    if (slot.kind !== "governed_grok" || slot.authMode !== "api_key") {
      return;
    }
    setFormMode("edit");
    setEditingSlotId(slot.slotId);
    setCreateKind("governed_grok");
    setCreateAuth("api_key");
    setDisplayName(slot.displayName);
    setBaseUrl(slot.baseUrl ?? "");
    setModelId(slot.modelId ?? "");
    // Leave secret field blank = keep stored value.
    setApiKeyDraft("");
    setNotice(null);
    setError(null);
    setProbe(null);
  }

  function closeForm(): void {
    setFormMode("closed");
    resetFormDrafts();
  }

  async function handleSave(event: FormEvent): Promise<void> {
    event.preventDefault();
    if (!projectId || busy) return;

    const name = displayName.trim();
    if (!name) {
      setError("显示名称不能为空。");
      return;
    }

    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      if (formMode === "edit" && editingSlotId) {
        await gateway.updateGrokApiKeySlot({
          projectId,
          slotId: editingSlotId,
          displayName: name,
          baseUrl: baseUrl.trim(),
          modelId: modelId.trim(),
          apiKey: apiKeyDraft,
        });
        // Clear draft key from React state immediately after send.
        setApiKeyDraft("");
        setNotice("槽位已更新。");
      } else if (createKind === "governed_grok" && createAuth === "login") {
        await gateway.createGrokLoginSlot({
          projectId,
          displayName: name,
        });
        setNotice("已创建 Grok 登录槽位（使用默认 profile）。");
      } else if (createKind === "governed_grok" && createAuth === "api_key") {
        const key = apiKeyDraft.trim();
        if (!key) {
          setError("创建 API-key 槽位时必须填写 API key。");
          setBusy(false);
          return;
        }
        if (!baseUrl.trim() || !modelId.trim()) {
          setError("请填写端点 base_url 与模型 id。");
          setBusy(false);
          return;
        }
        await gateway.createGrokApiKeySlot({
          projectId,
          displayName: name,
          baseUrl: baseUrl.trim(),
          modelId: modelId.trim(),
          apiKey: key,
        });
        setApiKeyDraft("");
        setNotice("已创建 API-key 槽位。密钥已交由宿主保管，界面仅显示「已配置」。");
      } else {
        const statusArgs = parseArgvField(statusArgsText);
        if (!executablePath.trim()) {
          setError("请填写可执行文件的绝对路径。");
          setBusy(false);
          return;
        }
        if (statusArgs.length === 0) {
          setError("请填写至少一项账号状态命令参数（例如 login status）。");
          setBusy(false);
          return;
        }
        await gateway.createExternalCliSlot({
          projectId,
          displayName: name,
          executablePath: executablePath.trim(),
          launchArgs: parseArgvField(launchArgsText),
          statusArgs,
        });
        setNotice("已登记外部 CLI 槽位（不受治理）。");
      }
      closeForm();
      await reload();
    } catch (cause) {
      const message =
        cause instanceof Error ? cause.message : "保存槽位失败。";
      setError(message);
    } finally {
      setBusy(false);
    }
  }

  async function handleProbe(slot: RuntimeSlotView): Promise<void> {
    if (!projectId || busy) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    setProbe(null);
    try {
      if (slot.kind === "external_cli") {
        const status = await gateway.getExternalCliStatus(
          projectId,
          slot.slotId,
        );
        const classified = classifyExternalCliStatus(status);
        const raw = [status.stdout, status.stderr]
          .map((part) => part.trim())
          .filter(Boolean)
          .join("\n");
        setProbe({
          slotId: slot.slotId,
          ok: classified.ok,
          text: raw
            ? `${classified.summary}\n${raw}`
            : classified.summary,
        });
      } else {
        const started = await gateway.startRuntimeSlot(
          projectId,
          slot.slotId,
        );
        setStartedSlotId(slot.slotId);
        setProbe({
          slotId: slot.slotId,
          ok: true,
          text: `探活成功：槽位已激活（${started.status}）。`,
        });
      }
    } catch (cause) {
      const code =
        cause instanceof RuntimeSlotsGatewayError
          ? cause.code
          : "runtime_slots_command_failed";
      const message =
        slot.kind === "governed_grok"
          ? mapGrokProbeFailureMessage(code)
          : cause instanceof Error
            ? cause.message
            : "探活失败。";
      setProbe({
        slotId: slot.slotId,
        ok: false,
        text: message,
      });
    } finally {
      setBusy(false);
    }
  }

  /**
   * After quick model switch: update list modelId from host result, then
   * reload for full consistency. Switch notice lives on the picker itself
   * (runtimeRestarted honesty).
   */
  async function handleModelSwitched(
    result: RuntimeSlotModelSwitchView,
  ): Promise<void> {
    setSlots((prev) =>
      prev.map((slot) =>
        slot.slotId === result.slotId
          ? { ...slot, modelId: result.modelId }
          : slot
      ),
    );
    // Quiet reload — picker already showed restart/save outcome.
    try {
      if (projectId) {
        const list = await gateway.listRuntimeSlots(projectId);
        setSlots(list);
      }
    } catch {
      // Keep optimistic modelId; list errors surface on next full reload.
    }
  }

  if (!projectId) {
    return (
      <section
        className="runtime-slots"
        data-testid="runtime-slots-section"
      >
        <div className="runtime-slots__head">
          <h3>运行时槽位</h3>
          <p>打开项目后可新增与配置项目级运行时槽位。</p>
        </div>
        <p
          className="runtime-slots__hint"
          data-testid="runtime-slots-no-project"
          role="status"
        >
          需要先打开项目：槽位按项目隔离，不能跨项目复用。
        </p>
      </section>
    );
  }

  return (
    <section
      className="runtime-slots"
      data-testid="runtime-slots-section"
    >
      <div className="runtime-slots__head">
        <h3>运行时槽位</h3>
        <p>
          为当前项目配置受治理的 Grok 槽位，或登记不受治理的外部 CLI。
          API key 仅在提交时发送给宿主，保存后界面只显示「已配置」。
        </p>
      </div>

      {loading ? (
        <p className="runtime-slots__status">正在加载槽位…</p>
      ) : slots.length === 0 ? (
        <p
          className="runtime-slots__hint"
          data-testid="runtime-slots-empty"
        >
          尚未配置槽位。点击下方按钮新增。
        </p>
      ) : (
        <ul className="runtime-slots-list" data-testid="runtime-slots-list">
          {slots.map((slot) => (
            <li
              className="runtime-slots-list__item"
              data-slot-id={slot.slotId}
              data-testid={`runtime-slot-${slot.slotId}`}
              key={slot.slotId}
            >
              <div className="runtime-slots-list__row">
                <div className="runtime-slots-list__meta">
                  <span className="runtime-slots-list__name">
                    {slot.displayName}
                  </span>
                  {slot.governance === "full" ? (
                    <span
                      className="runtime-slots-badge runtime-slots-badge--governed"
                      data-testid={`runtime-slot-governed-${slot.slotId}`}
                    >
                      {GOVERNED_GROK_BADGE}
                    </span>
                  ) : (
                    <span
                      className="runtime-slots-badge runtime-slots-badge--ungoverned"
                      data-testid={`runtime-slot-ungoverned-badge-${slot.slotId}`}
                    >
                      不受治理
                    </span>
                  )}
                  <span className="runtime-slots-badge">
                    {slot.kind === "governed_grok"
                      ? slot.authMode === "login"
                        ? "Grok · 登录"
                        : "Grok · API key"
                      : "外部 CLI"}
                  </span>
                  {slot.kind === "governed_grok"
                  && slot.governance === "full" ? (
                    startedSlotId === slot.slotId ? (
                      <span
                        className="runtime-slots-badge runtime-slots-badge--started"
                        data-testid={`runtime-slot-started-${slot.slotId}`}
                      >
                        已启动
                      </span>
                    ) : (
                      <span
                        className="runtime-slots-badge runtime-slots-badge--needs-start"
                        data-testid={`runtime-slot-needs-start-${slot.slotId}`}
                      >
                        未启动 · 需启动后才能对话
                      </span>
                    )
                  ) : null}
                </div>
                <div className="runtime-slots-list__actions">
                  {slot.kind === "governed_grok"
                  && slot.authMode === "api_key" ? (
                    <button
                      data-testid={`runtime-slot-edit-${slot.slotId}`}
                      disabled={busy}
                      onClick={() => openEdit(slot)}
                      type="button"
                    >
                      编辑
                    </button>
                  ) : null}
                  {slot.kind === "external_cli" ? (
                    <button
                      data-testid={`runtime-slot-probe-${slot.slotId}`}
                      disabled={busy}
                      onClick={() => void handleProbe(slot)}
                      type="button"
                    >
                      查询账号状态
                    </button>
                  ) : (
                    <button
                      className={
                        startedSlotId === slot.slotId
                          ? "runtime-slots-start"
                          : "runtime-slots-start runtime-slots-start--primary"
                      }
                      data-testid={`runtime-slot-probe-${slot.slotId}`}
                      disabled={busy}
                      onClick={() => void handleProbe(slot)}
                      type="button"
                    >
                      {startedSlotId === slot.slotId ? "重新启动" : "启动"}
                    </button>
                  )}
                </div>
              </div>

              {slot.governance === "none" ? (
                <p
                  className="runtime-slots-ungoverned"
                  data-testid={`runtime-slot-ungoverned-${slot.slotId}`}
                  role="note"
                >
                  <strong>{EXTERNAL_CLI_UNGOVERNED_HEADING}</strong>
                  。{EXTERNAL_CLI_UNGOVERNED_BODY}
                </p>
              ) : null}

              {slot.kind === "governed_grok" && slot.authMode === "api_key" ? (
                <p className="runtime-slots-list__detail">
                  端点：{slot.baseUrl ?? "—"}
                  {" · "}
                  密钥：
                  <span data-testid={`runtime-slot-secret-status-${slot.slotId}`}>
                    {slot.secretConfigured
                      ? SECRET_CONFIGURED_LABEL
                      : SECRET_NOT_CONFIGURED_LABEL}
                  </span>
                </p>
              ) : null}

              {/*
                Quick model switch on the list row — core of "each slot like
                desktop model switch". external_cli must not get a picker.
              */}
              {slot.kind === "governed_grok" && projectId ? (
                <RuntimeSlotModelPicker
                  currentModelId={slot.modelId}
                  disabled={busy}
                  gateway={gateway}
                  onSwitched={(result) => {
                    void handleModelSwitched(result);
                  }}
                  projectId={projectId}
                  slotId={slot.slotId}
                />
              ) : null}

              {slot.kind === "external_cli" ? (
                <p className="runtime-slots-list__detail">
                  可执行文件：{slot.executablePath ?? "—"}
                  {slot.statusArgs && slot.statusArgs.length > 0
                    ? ` · 状态参数：${slot.statusArgs.join(" ")}`
                    : null}
                </p>
              ) : null}

              {probe && probe.slotId === slot.slotId ? (
                <p
                  className={
                    probe.ok
                      ? "runtime-slots-probe runtime-slots-probe--ok"
                      : "runtime-slots-probe runtime-slots-probe--fail"
                  }
                  data-testid={`runtime-slot-probe-result-${slot.slotId}`}
                  role={probe.ok ? "status" : "alert"}
                >
                  {probe.text}
                </p>
              ) : null}
            </li>
          ))}
        </ul>
      )}

      {formMode === "closed" ? (
        <button
          className="runtime-slots__add"
          data-testid="runtime-slots-add"
          disabled={busy || loading}
          onClick={openCreate}
          type="button"
        >
          增加新槽位
        </button>
      ) : (
        <form
          className="runtime-slots-form"
          data-testid="runtime-slots-form"
          onSubmit={(event) => void handleSave(event)}
        >
          <p className="runtime-slots-form__title">
            {formMode === "edit" ? "编辑 API-key 槽位" : "新增槽位"}
          </p>

          {formMode === "create" ? (
            <div
              className="runtime-slots-form__kind"
              role="group"
              aria-label="槽位类型"
            >
              <button
                aria-pressed={createKind === "governed_grok"}
                data-testid="runtime-slots-kind-grok"
                onClick={() => setCreateKind("governed_grok")}
                type="button"
              >
                受治理 Grok
              </button>
              <button
                aria-pressed={createKind === "external_cli"}
                data-testid="runtime-slots-kind-external"
                onClick={() => setCreateKind("external_cli")}
                type="button"
              >
                外部 CLI
              </button>
            </div>
          ) : null}

          {formMode === "create" && createKind === "external_cli" ? (
            <p
              className="runtime-slots-ungoverned"
              data-testid="runtime-slots-create-ungoverned-warning"
              role="note"
            >
              <strong>{EXTERNAL_CLI_UNGOVERNED_HEADING}</strong>
              。{EXTERNAL_CLI_UNGOVERNED_BODY}
            </p>
          ) : null}

          {formMode === "create" && createKind === "governed_grok" ? (
            <div
              className="runtime-slots-form__kind"
              role="group"
              aria-label="Grok 认证方式"
            >
              <button
                aria-pressed={createAuth === "api_key"}
                data-testid="runtime-slots-auth-api-key"
                onClick={() => setCreateAuth("api_key")}
                type="button"
              >
                API key
              </button>
              <button
                aria-pressed={createAuth === "login"}
                data-testid="runtime-slots-auth-login"
                onClick={() => setCreateAuth("login")}
                type="button"
              >
                登录 profile
              </button>
            </div>
          ) : null}

          <label className="settings-field">
            <span className="settings-field__label">显示名称</span>
            <input
              aria-label="槽位显示名称"
              autoComplete="off"
              className="settings-field__control"
              data-testid="runtime-slots-display-name"
              disabled={busy}
              onChange={(event) => setDisplayName(event.target.value)}
              spellCheck={false}
              type="text"
              value={displayName}
            />
          </label>

          {(formMode === "edit"
            || (formMode === "create"
              && createKind === "governed_grok"
              && createAuth === "api_key")) ? (
            <>
              <label className="settings-field">
                <span className="settings-field__label">端点 base_url</span>
                <input
                  aria-label="端点 base_url"
                  autoComplete="off"
                  className="settings-field__control"
                  data-testid="runtime-slots-base-url"
                  disabled={busy}
                  onChange={(event) => setBaseUrl(event.target.value)}
                  placeholder="https://…"
                  spellCheck={false}
                  type="text"
                  value={baseUrl}
                />
              </label>
              <label className="settings-field">
                <span className="settings-field__label">
                  模型 id
                  <span className="runtime-slots-badge">
                    创建/编辑表单手输；已有槽位可在列表中快速切换
                  </span>
                </span>
                <input
                  aria-label="模型 id"
                  autoComplete="off"
                  className="settings-field__control"
                  data-testid="runtime-slots-model-id"
                  disabled={busy}
                  onChange={(event) => setModelId(event.target.value)}
                  spellCheck={false}
                  type="text"
                  value={modelId}
                />
              </label>
              <label className="settings-field">
                <span className="settings-field__label">
                  API key
                  {formMode === "edit" ? (
                    <span className="runtime-slots-badge">
                      留空则保持原值
                    </span>
                  ) : null}
                </span>
                <input
                  aria-label="API key"
                  autoComplete="new-password"
                  className="settings-field__control"
                  data-testid="runtime-slots-api-key"
                  disabled={busy}
                  onChange={(event) => setApiKeyDraft(event.target.value)}
                  placeholder={
                    formMode === "edit"
                      ? "留空表示不修改已保存的密钥"
                      : "输入后不回显；保存后仅显示「已配置」"
                  }
                  spellCheck={false}
                  type="password"
                  value={apiKeyDraft}
                />
              </label>
            </>
          ) : null}

          {formMode === "create" && createKind === "external_cli" ? (
            <>
              <label className="settings-field">
                <span className="settings-field__label">可执行文件路径</span>
                <input
                  aria-label="外部 CLI 可执行文件路径"
                  autoComplete="off"
                  className="settings-field__control"
                  data-testid="runtime-slots-executable"
                  disabled={busy}
                  onChange={(event) => setExecutablePath(event.target.value)}
                  placeholder="绝对路径，例如 C:\\tools\\codex.exe"
                  spellCheck={false}
                  type="text"
                  value={executablePath}
                />
              </label>
              <label className="settings-field">
                <span className="settings-field__label">
                  调用参数（空格分隔，可空）
                </span>
                <input
                  aria-label="外部 CLI 调用参数"
                  autoComplete="off"
                  className="settings-field__control"
                  data-testid="runtime-slots-launch-args"
                  disabled={busy}
                  onChange={(event) => setLaunchArgsText(event.target.value)}
                  spellCheck={false}
                  type="text"
                  value={launchArgsText}
                />
              </label>
              <label className="settings-field">
                <span className="settings-field__label">
                  账号状态参数（空格分隔，必填）
                </span>
                <input
                  aria-label="外部 CLI 账号状态参数"
                  autoComplete="off"
                  className="settings-field__control"
                  data-testid="runtime-slots-status-args"
                  disabled={busy}
                  onChange={(event) => setStatusArgsText(event.target.value)}
                  placeholder="例如 login status"
                  spellCheck={false}
                  type="text"
                  value={statusArgsText}
                />
              </label>
            </>
          ) : null}

          {formMode === "create"
          && createKind === "governed_grok"
          && createAuth === "login" ? (
            <p className="runtime-slots__hint" role="note">
              使用本机默认 Grok 登录 profile。登录由 Grok CLI 管理，Pillow
              不存储该登录。
            </p>
          ) : null}

          {/* Hidden marker for tests asserting ungoverned copy constant. */}
          <span hidden data-testid="runtime-slots-ungoverned-copy">
            {EXTERNAL_CLI_UNGOVERNED_BANNER}
          </span>

          <div className="settings-actions modal-actions">
            <button
              data-testid="runtime-slots-form-cancel"
              disabled={busy}
              onClick={closeForm}
              type="button"
            >
              取消
            </button>
            <button
              data-testid="runtime-slots-form-save"
              disabled={busy}
              type="submit"
            >
              {busy ? "保存中…" : "保存"}
            </button>
          </div>
        </form>
      )}

      {error ? (
        <p
          className="runtime-slots__error"
          data-testid="runtime-slots-error"
          role="alert"
        >
          {error}
        </p>
      ) : null}
      {notice ? (
        <p
          className="runtime-slots__notice"
          data-testid="runtime-slots-notice"
          role="status"
        >
          {notice}
        </p>
      ) : null}
    </section>
  );
}
