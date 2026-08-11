/**
 * First-run onboarding copy.
 *
 * Product claim (ADR-0003 Q9): Grok login is managed by the Grok CLI;
 * Pillow does not store that login. Never imply credential storage for the
 * login path. API-key slots hand the key to the host once; the UI never
 * re-displays it.
 *
 * This is **guidance for never-configured strangers**, not an error path.
 * Misconfiguration uses the existing degraded banner instead.
 *
 * Chat path fact: the only way to get a usable agent is to create and start
 * a governed runtime slot. External CLI is registration-only — never listed
 * here as a way to chat.
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
 * Slot setup step — required after open project, before chat works.
 * Covers the two governed paths only (login profile / API key).
 */
export const FIRSTRUN_SLOT_BODY =
  "能对话之前，还需在「设置 → 运行时槽位」配置并启动一个代理槽位。两种方式：① 使用已登录的 Grok（新建「登录 profile」槽位，无需填写 API key，然后点「启动」）；② 使用自己的 API key（新建「API key」槽位，填写端点与模型，保存后点「启动」）。外部 CLI 仅供登记与账号状态查询，不能用来对话。";

/**
 * @deprecated Prefer FIRSTRUN_SLOT_BODY. Kept as alias so older imports that
 * still reference the Grok tip name resolve to the slot step (same product fact).
 */
export const FIRSTRUN_GROK_BODY = FIRSTRUN_SLOT_BODY;

/** Primary CTA — open/register a project folder. */
export const FIRSTRUN_OPEN_PROJECT_LABEL = "打开项目";

/** Secondary CTA — reach runtime slots / path settings. */
export const FIRSTRUN_OPEN_SETTINGS_LABEL = "打开设置";

/** Dismiss / skip — durable via settings `ui.onboarding.dismissed`. */
export const FIRSTRUN_DISMISS_LABEL = "跳过";

/** Accessible name for the close control (same dismiss path). */
export const FIRSTRUN_CLOSE_LABEL = "关闭引导";
