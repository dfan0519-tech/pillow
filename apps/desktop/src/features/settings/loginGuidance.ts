/**
 * First-run / conversation login guidance for Grok CLI auth.
 *
 * Product claim (ADR-0003 Q9 / design §4.3):
 * "Grok authentication is managed by the Grok CLI; Pillow does not store that login."
 *
 * Detection uses only stable, non-secret failure phrases and codes already
 * emitted by the adapter health surface. Never opens, reads, or displays
 * auth/token files or account identity.
 */

/** Short title for the shell banner. */
export const GROK_CLI_LOGIN_TITLE = "需要完成 Grok CLI 登录";

/**
 * Fixed operator-facing guidance. Does not embed account names, emails,
 * profile paths, token material, or auth file contents.
 */
export const GROK_CLI_LOGIN_GUIDANCE =
  "Grok 登录由 Grok CLI 管理，Pillow 不存储该登录。请在终端运行 grok login，完成后回到 Pillow 重试发送。";

/** Settings panel static help (same claim, slightly longer). */
export const GROK_CLI_LOGIN_SETTINGS_HELP =
  "对话需要 Grok CLI 已登录。请在终端运行 grok login；Pillow 从不读取或保存该登录。改路径后需重启 Pillow。";

/**
 * Phrases that classify a transport/RPC failure as authentication-related.
 * Mirrors adapter health classification — phrase match only, never credentials.
 */
const AUTH_FAILURE_PHRASES: readonly string[] = [
  "unauthenticated",
  "unauthorized",
  "authentication required",
  "authentication failed",
  "authentication_failed",
  "not authenticated",
  "auth failed",
  "auth_failed",
  "login required",
  "not logged in",
  "invalid token",
  "expired token",
  "token expired",
  "no credentials",
  "sign in required",
  "signin required",
];

/** Stable codes that mean the same class without inspecting message bodies. */
const AUTH_FAILURE_CODES: readonly string[] = [
  "authentication_failed",
  "auth_failed",
  "unauthenticated",
  "unauthorized",
];

function asciiLower(text: string): string {
  return text.toLowerCase();
}

/**
 * True when free-form error text looks like a recognized auth failure.
 * Empty / null → false. Does not parse paths or account identity.
 */
export function looksLikeAuthenticationFailure(
  text: string | null | undefined,
): boolean {
  if (typeof text !== "string" || text.trim().length === 0) return false;
  const lower = asciiLower(text);
  return AUTH_FAILURE_PHRASES.some((phrase) => lower.includes(phrase));
}

/**
 * True when a stable error / probe code is the authentication-failed class.
 */
export function looksLikeAuthenticationFailureCode(
  code: string | null | undefined,
): boolean {
  if (typeof code !== "string" || code.trim().length === 0) return false;
  const lower = asciiLower(code.trim());
  return AUTH_FAILURE_CODES.some(
    (needle) => lower === needle || lower.includes(needle),
  );
}

/**
 * Map a raw send/turn failure into user-facing composer text.
 * Auth-class failures become fixed CLI login guidance (never raw provider text
 * that might carry machine detail). Other failures pass through unchanged.
 */
export function userFacingSendError(rawMessage: string): string {
  if (
    looksLikeAuthenticationFailure(rawMessage)
    || looksLikeAuthenticationFailureCode(rawMessage)
  ) {
    return GROK_CLI_LOGIN_GUIDANCE;
  }
  return rawMessage;
}

/**
 * Classify a turn_failed event payload (code + message) without touching files.
 */
export function isAuthenticationFailedEvent(payload: {
  code?: string | null;
  message?: string | null;
}): boolean {
  return (
    looksLikeAuthenticationFailureCode(payload.code)
    || looksLikeAuthenticationFailure(payload.message)
  );
}

/**
 * Guard used by tests: guidance copy must never look like credential leakage.
 */
export function guidanceLooksSafe(text: string): boolean {
  const lower = asciiLower(text);
  const forbidden = [
    "auth.json",
    "bearer ",
    "sk-",
    "api_key",
    "apikey",
    "@",
    "userprofile",
    "c:\\users",
    "/home/",
  ];
  return !forbidden.some((needle) => lower.includes(needle));
}
