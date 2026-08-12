//! Credential *shapes* only — no storage, no network, no zeroing via `unsafe`.

use std::fmt;

/// Kind of material an adapter requires before `prepare_request` may succeed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialKind {
    /// Static API key presented as `Authorization: Bearer`.
    ApiKey,
    /// OAuth access token (and optional JWT claims for derived headers).
    OAuth,
}

/// Opaque token material. Never appears in [`Debug`] output.
#[derive(Clone)]
pub struct SecretToken(String);

impl SecretToken {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for SecretToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretToken([REDACTED])")
    }
}

impl Drop for SecretToken {
    fn drop(&mut self) {
        // Best-effort clear without `unsafe`. Not a cryptographic wipe guarantee.
        let len = self.0.len();
        self.0.clear();
        self.0.shrink_to_fit();
        let _ = len;
    }
}

/// Credential presented to [`crate::prepare_request`].
///
/// Callers obtain material from host secret stores; this type only holds
/// in-memory values for pure request construction.
#[derive(Clone)]
pub enum Credential {
    ApiKey {
        token: SecretToken,
    },
    OAuth {
        access_token: SecretToken,
        /// Explicit account id when already known (e.g. stored after token exchange).
        account_id: Option<String>,
        /// Optional ID token; preferred over `access_token` for claim extraction.
        id_token: Option<SecretToken>,
    },
}

impl Credential {
    pub fn kind(&self) -> CredentialKind {
        match self {
            Credential::ApiKey { .. } => CredentialKind::ApiKey,
            Credential::OAuth { .. } => CredentialKind::OAuth,
        }
    }

    /// Raw access material for header injection (never logged by this crate).
    pub(crate) fn access_material(&self) -> &str {
        match self {
            Credential::ApiKey { token } => token.as_str(),
            Credential::OAuth { access_token, .. } => access_token.as_str(),
        }
    }
}

impl fmt::Debug for Credential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Credential::ApiKey { .. } => f
                .debug_struct("ApiKey")
                .field("token", &"SecretToken([REDACTED])")
                .finish(),
            Credential::OAuth {
                account_id,
                id_token,
                ..
            } => f
                .debug_struct("OAuth")
                .field("access_token", &"SecretToken([REDACTED])")
                .field("account_id", account_id)
                .field(
                    "id_token",
                    &id_token.as_ref().map(|_| "SecretToken([REDACTED])"),
                )
                .finish(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_token_debug_never_contains_value() {
        let marker = format!("{}{}", "unit-secret-", "marker-aa11bb22");
        let token = SecretToken::new(marker.clone());
        let rendered = format!("{token:?}");
        assert!(!rendered.contains(&marker), "debug leaked: {rendered}");
        assert!(rendered.contains("REDACTED"));
    }

    #[test]
    fn credential_debug_redacts_tokens() {
        let marker = format!("{}{}", "cred-secret-", "cc33dd44");
        let cred = Credential::OAuth {
            access_token: SecretToken::new(marker.clone()),
            account_id: Some("acct-public-label".into()),
            id_token: Some(SecretToken::new(format!("{marker}-id"))),
        };
        let rendered = format!("{cred:?}");
        assert!(!rendered.contains(&marker), "debug leaked: {rendered}");
        assert!(rendered.contains("acct-public-label"));
    }
}
