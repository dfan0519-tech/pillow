//! Account-label sanitization: keep short non-secret identifiers only.

/// Maximum retained account label length (Unicode scalars).
pub const ACCOUNT_LABEL_MAX_CHARS: usize = 128;

/// Sanitize a candidate account identifier.
///
/// Keeps short labels that look like email addresses, organization / plan /
/// provider names. Rejects token-shaped material, long opaque strings, and
/// anything containing secret-looking prefixes.
pub fn sanitize_account_label(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.chars().count() > ACCOUNT_LABEL_MAX_CHARS {
        return None;
    }
    if looks_like_secret(trimmed) {
        return None;
    }
    // Allow only a conservative character set for the retained label.
    if !trimmed.chars().all(allowed_label_char) {
        return None;
    }
    // Must contain at least one letter (reject pure punctuation / digits).
    if !trimmed.chars().any(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    Some(trimmed.to_string())
}

fn allowed_label_char(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(
            c,
            '@' | '.' | '_' | '-' | '+' | ' ' | '/' | '(' | ')' | '[' | ']'
        )
}

fn looks_like_secret(s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    if lower.contains("sk-")
        || lower.contains("bearer ")
        || lower.contains("api_key")
        || lower.contains("apikey")
        || lower.contains("access_token")
        || lower.contains("refresh_token")
        || lower.contains("client_secret")
        || lower.contains("cookie")
        || lower.starts_with("eyj")
    // JWT header prefix
    {
        return true;
    }
    // Long opaque token-like run (no spaces, mostly base64/url-safe).
    if s.chars().count() >= 32 && !s.contains(['@', ' ', '.', '/']) && is_opaque_token_charset(s) {
        return true;
    }
    // Embedded long token substring.
    for part in s.split(|c: char| c.is_whitespace() || c == '=' || c == ':' || c == ',') {
        let part = part.trim_matches(|c: char| matches!(c, '"' | '\'' | '`' | ';' | ','));
        if part.len() >= 32 && is_opaque_token_charset(part) && !part.contains('@') {
            return true;
        }
        if part.to_ascii_lowercase().starts_with("sk-") && part.len() >= 12 {
            return true;
        }
        if part.to_ascii_lowercase().starts_with("eyj") && part.len() >= 20 {
            return true;
        }
    }
    false
}

fn is_opaque_token_charset(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '+' | '/' | '=' | '.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_email_and_plan_labels() {
        assert_eq!(
            sanitize_account_label("dev@example.com").as_deref(),
            Some("dev@example.com")
        );
        assert_eq!(
            sanitize_account_label("  ChatGPT  ").as_deref(),
            Some("ChatGPT")
        );
        assert_eq!(
            sanitize_account_label("grok.com").as_deref(),
            Some("grok.com")
        );
        assert_eq!(
            sanitize_account_label("Anthropic (Pro)").as_deref(),
            Some("Anthropic (Pro)")
        );
    }

    #[test]
    fn drops_token_shaped_material() {
        // Assembled at runtime: a literal here is itself secret-shaped and
        // trips the repo secret scanner. The input under test is unchanged.
        let fake_key = format!("sk-{}", "fakeTESTKEY_not_real_aaaaaaaaaaaaaaaa");
        assert!(sanitize_account_label(&fake_key).is_none());
        assert!(sanitize_account_label(&format!("user={fake_key}")).is_none());
        let fake_bearer = format!("Bearer {}", "FAKESECRET_g2h3i4j5k6l7m8n9o0p1");
        assert!(sanitize_account_label(&fake_bearer).is_none());
        let jwtish = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.aaaa.bbbb";
        assert!(sanitize_account_label(jwtish).is_none());
        let long_opaque = "a".repeat(48);
        assert!(sanitize_account_label(&long_opaque).is_none());
    }

    #[test]
    fn drops_overlong_and_empty() {
        assert!(sanitize_account_label("").is_none());
        assert!(sanitize_account_label("   ").is_none());
        let long = format!("{}@example.com", "a".repeat(ACCOUNT_LABEL_MAX_CHARS));
        assert!(sanitize_account_label(&long).is_none());
    }
}
