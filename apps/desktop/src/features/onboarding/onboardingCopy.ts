/**
 * First-run onboarding copy.
 *
 * Product claim (ADR-0003 Q9): Grok login is managed by the Grok CLI;
 * Pillow does not store that login. Never imply credential storage.
 *
 * This is **guidance for never-configured strangers**, not an error path.
 * Misconfiguration uses the existing degraded banner instead.
 */

/** Region title — neutral welcome, not an error headline. */
export const FIRSTRUN_TITLE = "欢迎使用 Pillow";

/**
 * Project-scoped product fact + primary next step.
 * Pillow works per project; empty shell needs an explicit open action.
 */
export const FIRSTRUN_PROJECT_BODY =
  "Pillow 以项目为单位工作。打开一个本地文件夹即可开始协作与对话。";

/**
 * Grok readiness tip for first run when the runtime is not degraded.
 * Status guidance only — not presented as a failure.
 * Matches ADR-0003 §4.3 / Q9 external claim.
 */
export const FIRSTRUN_GROK_BODY =
  "对话需要本机已安装 Grok CLI。Grok 登录由 Grok CLI 管理，Pillow 不存储该登录。请在终端运行 grok login 后再发消息。";

/** Primary CTA — open/register a project folder. */
export const FIRSTRUN_OPEN_PROJECT_LABEL = "打开项目";

/** Secondary CTA — reach path settings without implying credential entry. */
export const FIRSTRUN_OPEN_SETTINGS_LABEL = "打开设置";

/** Dismiss / skip — durable via settings `ui.onboarding.dismissed`. */
export const FIRSTRUN_DISMISS_LABEL = "跳过";

/** Accessible name for the close control (same dismiss path). */
export const FIRSTRUN_CLOSE_LABEL = "关闭引导";
