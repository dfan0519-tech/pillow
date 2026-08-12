//! Header / error redaction helpers. Never log raw token material.

/// Header names whose values must never appear in Debug, logs, or errors.
pub fn is_sensitive_header_name(name: &str) -> bool {
    let n = name.trim();
    n.eq_ignore_ascii_case("authorization")
        || n.eq_ignore_ascii_case("proxy-authorization")
        || n.eq_ignore_ascii_case("x-api-key")
        || n.eq_ignore_ascii_case("api-key")
}

/// Replace a secret-bearing header value for Debug / Display.
pub fn redact_header_value(value: &str) -> String {
    // Keep the scheme so a reader can tell Bearer from Basic, drop the rest.
    // Never return a prefix of the secret: a partial token is still a token.
    match value.split_once(' ') {
        Some((scheme, _)) if !scheme.is_empty() => format!("{scheme} [REDACTED]"),
        _ => "[REDACTED]".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_bearer_form() {
        let marker = format!("{}{}", "redact-", "gg77hh88");
        let out = redact_header_value(&format!("Bearer {marker}"));
        assert_eq!(out, "Bearer [REDACTED]");
        assert!(!out.contains(&marker));
    }

    #[test]
    fn sensitive_names() {
        assert!(is_sensitive_header_name("Authorization"));
        assert!(is_sensitive_header_name("authorization"));
        assert!(is_sensitive_header_name("X-Api-Key"));
        assert!(!is_sensitive_header_name("Content-Type"));
        assert!(!is_sensitive_header_name("ChatGPT-Account-Id"));
    }
}
