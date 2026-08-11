/**
 * Per-slot model picker for governed Grok slots.
 * Loads host catalog; unsupported/fail falls back to manual entry.
 * Switch results report runtimeRestarted honestly — no optimistic "session applied".
 */

import { useCallback, useEffect, useId, useState } from "react";
import {
  mapModelCatalogUnsupportedReason,
  mapModelSwitchResultMessage,
  mapRuntimeSlotErrorMessage,
  truncateModelIdForDisplay,
} from "./runtimeSlotsGateway";
import {
  type RuntimeSlotModelCatalogView,
  type RuntimeSlotModelSwitchView,
  type RuntimeSlotsGateway,
  RuntimeSlotsGatewayError,
} from "./runtimeSlotsTypes";

export type RuntimeSlotModelPickerProps = {
  projectId: string;
  slotId: string;
  currentModelId: string | null;
  gateway: RuntimeSlotsGateway;
  disabled?: boolean;
  /** Called after a successful switch with the host result (parent reloads list). */
  onSwitched?: (result: RuntimeSlotModelSwitchView) => void;
};

type CatalogPhase =
  | { kind: "loading" }
  | { kind: "available"; catalog: RuntimeSlotModelCatalogView }
  | { kind: "unsupported"; catalog: RuntimeSlotModelCatalogView }
  | { kind: "error"; message: string };

const CUSTOM_SELECT_VALUE = "__pillow_custom_model__";

export function RuntimeSlotModelPicker({
  projectId,
  slotId,
  currentModelId,
  gateway,
  disabled = false,
  onSwitched,
}: RuntimeSlotModelPickerProps) {
  const selectId = useId();
  const manualId = useId();
  const [phase, setPhase] = useState<CatalogPhase>({ kind: "loading" });
  const [manualMode, setManualMode] = useState(false);
  const [manualDraft, setManualDraft] = useState(currentModelId ?? "");
  const [busy, setBusy] = useState(false);
  const [switchNotice, setSwitchNotice] = useState<string | null>(null);
  const [switchError, setSwitchError] = useState<string | null>(null);
  const [loadGeneration, setLoadGeneration] = useState(0);

  const loadCatalog = useCallback(async () => {
    setPhase({ kind: "loading" });
    setSwitchError(null);
    try {
      const catalog = await gateway.getRuntimeSlotModelCatalog(
        projectId,
        slotId,
      );
      if (catalog.status === "available") {
        setPhase({ kind: "available", catalog });
        // Prefer catalog select after a successful load/refresh. Keep manual only
        // when the current id is set but not in the remote list (or list empty).
        if (catalog.models.length === 0) {
          setManualMode(true);
        } else if (
          currentModelId
          && !catalog.models.includes(currentModelId)
        ) {
          setManualMode(true);
        } else {
          setManualMode(false);
        }
      } else {
        setPhase({ kind: "unsupported", catalog });
        setManualMode(true);
      }
    } catch (cause) {
      const message =
        cause instanceof RuntimeSlotsGatewayError
          ? cause.message
          : mapRuntimeSlotErrorMessage("runtime_slot_model_catalog_unavailable");
      setPhase({ kind: "error", message });
      setManualMode(true);
    }
  }, [gateway, projectId, slotId, currentModelId]);

  useEffect(() => {
    void loadCatalog();
  }, [loadCatalog, loadGeneration]);

  useEffect(() => {
    setManualDraft(currentModelId ?? "");
  }, [currentModelId]);

  async function applyModel(modelId: string): Promise<void> {
    const trimmed = modelId.trim();
    if (!trimmed || busy || disabled) return;
    if (trimmed === (currentModelId ?? "")) {
      setSwitchNotice(null);
      setSwitchError(null);
      return;
    }
    setBusy(true);
    setSwitchError(null);
    setSwitchNotice(null);
    try {
      const result = await gateway.setRuntimeSlotModel(
        projectId,
        slotId,
        trimmed,
      );
      // Honest host-driven copy only.
      setSwitchNotice(mapModelSwitchResultMessage(result));
      setManualDraft(result.modelId);
      onSwitched?.(result);
    } catch (cause) {
      const message =
        cause instanceof RuntimeSlotsGatewayError
          ? cause.message
          : mapRuntimeSlotErrorMessage("runtime_slots_command_failed");
      setSwitchError(message);
    } finally {
      setBusy(false);
    }
  }

  const showManual =
    manualMode
    || phase.kind === "unsupported"
    || phase.kind === "error"
    || (phase.kind === "available" && phase.catalog.models.length === 0);

  const selectValue = (() => {
    if (phase.kind !== "available") return CUSTOM_SELECT_VALUE;
    if (manualMode) return CUSTOM_SELECT_VALUE;
    if (currentModelId && phase.catalog.models.includes(currentModelId)) {
      return currentModelId;
    }
    if (currentModelId) return CUSTOM_SELECT_VALUE;
    return "";
  })();

  return (
    <div
      className="runtime-slot-model-picker"
      data-testid={`runtime-slot-model-picker-${slotId}`}
    >
      <div className="runtime-slot-model-picker__row">
        <span className="runtime-slot-model-picker__label">模型</span>
        <span
          className="runtime-slot-model-picker__current"
          data-testid={`runtime-slot-model-current-${slotId}`}
          title={currentModelId ?? undefined}
        >
          当前：
          {currentModelId
            ? truncateModelIdForDisplay(currentModelId)
            : "—"}
        </span>
        <button
          className="runtime-slot-model-picker__refresh"
          data-testid={`runtime-slot-model-refresh-${slotId}`}
          disabled={disabled || busy || phase.kind === "loading"}
          onClick={() => setLoadGeneration((n) => n + 1)}
          type="button"
        >
          刷新目录
        </button>
      </div>

      {phase.kind === "loading" ? (
        <p
          className="runtime-slot-model-picker__status"
          data-testid={`runtime-slot-model-catalog-loading-${slotId}`}
          role="status"
        >
          正在加载模型目录…
        </p>
      ) : null}

      {phase.kind === "available" && !showManual ? (
        <label className="runtime-slot-model-picker__field" htmlFor={selectId}>
          <span className="settings-field__label">从目录选择</span>
          <select
            className="settings-field__control runtime-slot-model-picker__select"
            data-testid={`runtime-slot-model-select-${slotId}`}
            disabled={disabled || busy}
            id={selectId}
            onChange={(event) => {
              const value = event.target.value;
              if (value === CUSTOM_SELECT_VALUE) {
                setManualMode(true);
                return;
              }
              if (value) {
                void applyModel(value);
              }
            }}
            value={selectValue}
          >
            <option disabled value="">
              选择模型…
            </option>
            {phase.catalog.models.map((model) => (
              <option key={model} title={model} value={model}>
                {truncateModelIdForDisplay(model)}
              </option>
            ))}
            <option value={CUSTOM_SELECT_VALUE}>手输未列出的模型 id…</option>
          </select>
        </label>
      ) : null}

      {phase.kind === "available" && showManual ? (
        <p
          className="runtime-slot-model-picker__hint"
          data-testid={`runtime-slot-model-catalog-available-hint-${slotId}`}
        >
          目录已加载（{phase.catalog.models.length} 个模型）。可选手输列表外的 id。
          <button
            className="runtime-slot-model-picker__link"
            data-testid={`runtime-slot-model-use-catalog-${slotId}`}
            disabled={disabled || busy || phase.catalog.models.length === 0}
            onClick={() => setManualMode(false)}
            type="button"
          >
            返回目录选择
          </button>
        </p>
      ) : null}

      {phase.kind === "unsupported" ? (
        <p
          className="runtime-slot-model-picker__hint"
          data-testid={`runtime-slot-model-catalog-unsupported-${slotId}`}
          role="status"
        >
          {mapModelCatalogUnsupportedReason(phase.catalog.reasonCode)}
        </p>
      ) : null}

      {phase.kind === "error" ? (
        <p
          className="runtime-slot-model-picker__error"
          data-testid={`runtime-slot-model-catalog-error-${slotId}`}
          role="alert"
        >
          {phase.message}
        </p>
      ) : null}

      {showManual ? (
        <div className="runtime-slot-model-picker__manual">
          <label className="runtime-slot-model-picker__field" htmlFor={manualId}>
            <span className="settings-field__label">模型 id（手输）</span>
            <input
              autoComplete="off"
              className="settings-field__control runtime-slot-model-picker__input"
              data-testid={`runtime-slot-model-manual-${slotId}`}
              disabled={disabled || busy}
              id={manualId}
              onChange={(event) => setManualDraft(event.target.value)}
              spellCheck={false}
              type="text"
              value={manualDraft}
            />
          </label>
          <button
            data-testid={`runtime-slot-model-apply-${slotId}`}
            disabled={
              disabled
              || busy
              || !manualDraft.trim()
              || manualDraft.trim() === (currentModelId ?? "")
            }
            onClick={() => void applyModel(manualDraft)}
            type="button"
          >
            {busy ? "切换中…" : "应用模型"}
          </button>
        </div>
      ) : null}

      {switchNotice ? (
        <p
          className="runtime-slot-model-picker__notice"
          data-testid={`runtime-slot-model-switch-notice-${slotId}`}
          role="status"
        >
          {switchNotice}
        </p>
      ) : null}
      {switchError ? (
        <p
          className="runtime-slot-model-picker__error"
          data-testid={`runtime-slot-model-switch-error-${slotId}`}
          role="alert"
        >
          {switchError}
        </p>
      ) : null}
    </div>
  );
}
