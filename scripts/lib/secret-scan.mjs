/**
 * Secret-pattern scan for harness output. Never tries to open auth files;
 * only scans provided text for known leakage patterns.
 */

const PATTERNS = [
  { id: "api_key_assignment", re: /\b(?:api[_-]?key|apikey)\s*[:=]\s*['"]?[A-Za-z0-9_\-]{8,}/iu },
  { id: "bearer_token", re: /\bBearer\s+[A-Za-z0-9\-._~+/]+=*/iu },
  { id: "authorization_header", re: /\bAuthorization\s*[:=]/iu },
  { id: "xai_api_key", re: /\bXAI_API_KEY\b/iu },
  { id: "openai_api_key", re: /\bOPENAI_API_KEY\b/iu },
  { id: "sk_live_token", re: /\bsk-(?:live|proj|ant)-[A-Za-z0-9]{10,}\b/u },
  { id: "jwt_blob", re: /\beyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\b/u },
  { id: "auth_json_path", re: /auth\.json/iu },
  { id: "raw_acp_envelope", re: /"jsonrpc"\s*:\s*"2\.0"/iu },
  { id: "acp_session_id_field", re: /"sessionId"\s*:\s*"[0-9a-f-]{8,}"/iu },
  { id: "account_email", re: /\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b/u },
  { id: "password_assignment", re: /\bpassword\s*[:=]\s*\S+/iu },
];

/**
 * Scan text for secret / identity / raw-envelope patterns.
 * @param {string} text
 * @returns {{ clean: boolean, findings: Array<{ id: string, index: number }> }}
 */
export function scanForSecrets(text) {
  const source = typeof text === "string" ? text : String(text ?? "");
  const findings = [];
  for (const pattern of PATTERNS) {
    const match = pattern.re.exec(source);
    if (match) {
      findings.push({ id: pattern.id, index: match.index });
    }
  }
  return { clean: findings.length === 0, findings };
}

/**
 * Assert text is free of secret patterns; throw with redacted summary.
 * @param {string} text
 * @param {string} [label]
 */
export function assertNoSecrets(text, label = "output") {
  const result = scanForSecrets(text);
  if (!result.clean) {
    const ids = result.findings.map((f) => f.id).join(", ");
    throw new Error(
      `Secret-pattern scan failed for ${label}: [${ids}]. `
        + "Harness must not print tokens, account identity, raw ACP envelopes, or auth paths.",
    );
  }
  return result;
}
