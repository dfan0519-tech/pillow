import { FormEvent, useEffect, useRef, useState } from "react";

type NewProjectDialogProps = {
  open: boolean;
  busy: boolean;
  onCancel: () => void;
  /** Name only — host picks parent folder and creates the child directory. */
  onConfirm: (name: string) => Promise<boolean>;
};

/**
 * Name-only new-project dialog. Paths are never typed; the host uses a
 * native folder picker for the parent directory after confirmation.
 */
export function NewProjectDialog({
  open,
  busy,
  onCancel,
  onConfirm,
}: NewProjectDialogProps) {
  const [name, setName] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!open) return;
    setName("");
    const frame = window.requestAnimationFrame(() => {
      inputRef.current?.focus();
    });
    return () => window.cancelAnimationFrame(frame);
  }, [open]);

  if (!open) return null;

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const trimmed = name.trim();
    if (!trimmed || busy) return;
    const ok = await onConfirm(trimmed);
    if (ok) setName("");
  }

  return (
    <div className="modal-backdrop" role="presentation">
      <form
        aria-label="新建项目"
        className="modal-card"
        onSubmit={(event) => void submit(event)}
      >
        <header className="modal-card__head">
          <h2>新建项目</h2>
          <p>只需填写名称。接下来会用系统文件夹选择器选择父目录。</p>
        </header>
        <label className="modal-field">
          项目名称
          <input
            ref={inputRef}
            aria-label="项目名称"
            autoComplete="off"
            disabled={busy}
            onChange={(event) => setName(event.target.value)}
            placeholder="例如 my-app"
            value={name}
          />
        </label>
        <div className="modal-actions">
          <button disabled={busy} onClick={onCancel} type="button">
            取消
          </button>
          <button disabled={busy || !name.trim()} type="submit">
            {busy ? "创建中…" : "选择位置并创建"}
          </button>
        </div>
      </form>
    </div>
  );
}
