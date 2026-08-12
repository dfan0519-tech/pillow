use pillow_runtime_slots::{
    AccountIdentifierStrategy, ClientIdProfile, ClientIdSource, OauthTransportParameters,
    ProfileField, ProviderLoginConversionError, ProviderLoginMethod, ProviderLoginProfile,
    ProviderLoginSupplement, provider_login_profile, provider_login_profiles,
    resolve_oauth_transport_parameters,
};

#[test]
fn catalog_covers_required_providers_and_login_methods() {
    let profiles = provider_login_profiles();
    let ids = profiles
        .iter()
        .map(|profile| profile.provider_id)
        .collect::<Vec<_>>();

    assert_eq!(
        ids,
        [
            "xai",
            "github-copilot",
            "anthropic",
            "openai",
            "custom-oidc",
        ]
    );
    assert_eq!(
        provider_login_profile("xai").unwrap().login_method,
        ProviderLoginMethod::DeviceCode
    );
    assert_eq!(
        provider_login_profile("github-copilot")
            .unwrap()
            .login_method,
        ProviderLoginMethod::DeviceCode
    );
    assert_eq!(
        provider_login_profile("anthropic").unwrap().login_method,
        ProviderLoginMethod::ApiKeyOnly
    );
    assert_eq!(
        provider_login_profile("openai").unwrap().login_method,
        ProviderLoginMethod::Pkce
    );
    assert_eq!(
        provider_login_profile("custom-oidc").unwrap().login_method,
        ProviderLoginMethod::Pkce
    );
}

#[test]
fn unverified_upstream_client_ids_are_unknown_and_not_embedded() {
    for provider_id in ["xai", "github-copilot", "openai"] {
        let profile = provider_login_profile(provider_id).unwrap();
        assert_eq!(profile.client_id.source, ClientIdSource::Unknown);
        assert_eq!(profile.client_id.value, ProfileField::Unknown);
    }

    let custom = provider_login_profile("custom-oidc").unwrap();
    assert_eq!(custom.client_id.source, ClientIdSource::UserSupplied);
    assert_eq!(custom.client_id.value, ProfileField::UserSupplied);
}

#[test]
fn account_identifier_strategy_is_explicit_and_never_inferred() {
    for provider_id in ["xai", "github-copilot", "anthropic"] {
        assert_eq!(
            provider_login_profile(provider_id)
                .unwrap()
                .account_identifier,
            AccountIdentifierStrategy::Unknown
        );
    }

    assert!(matches!(
        provider_login_profile("openai").unwrap().account_identifier,
        AccountIdentifierStrategy::TokenClaims { .. }
    ));
    assert_eq!(
        provider_login_profile("custom-oidc")
            .unwrap()
            .account_identifier,
        AccountIdentifierStrategy::UserSupplied
    );
}

#[test]
fn every_profile_has_relative_or_official_evidence() {
    for profile in provider_login_profiles() {
        assert!(!profile.evidence.is_empty(), "{}", profile.provider_id);
        for evidence in profile.evidence {
            assert!(!evidence.fields.is_empty(), "{}", profile.provider_id);
            assert!(
                evidence.source.starts_with("../_ref/opencode/")
                    || evidence.source.starts_with("https://"),
                "unexpected evidence source for {}",
                profile.provider_id
            );
            assert!(!evidence.source.contains(":\\"));
            assert!(!evidence.source.starts_with("F:/"));
        }
    }
}

#[test]
fn every_known_or_user_supplied_profile_field_has_evidence() {
    for profile in provider_login_profiles() {
        let evidenced = |field| {
            profile
                .evidence
                .iter()
                .any(|entry| entry.fields.contains(&field))
        };

        assert!(evidenced(
            pillow_runtime_slots::ProviderLoginField::ProviderId
        ));
        assert!(evidenced(
            pillow_runtime_slots::ProviderLoginField::DisplayName
        ));
        assert!(evidenced(
            pillow_runtime_slots::ProviderLoginField::LoginMethod
        ));

        for (field, value) in [
            (
                pillow_runtime_slots::ProviderLoginField::Issuer,
                profile.issuer,
            ),
            (
                pillow_runtime_slots::ProviderLoginField::AuthorizeEndpoint,
                profile.authorize_endpoint,
            ),
            (
                pillow_runtime_slots::ProviderLoginField::DeviceAuthorizationEndpoint,
                profile.device_authorization_endpoint,
            ),
            (
                pillow_runtime_slots::ProviderLoginField::TokenEndpoint,
                profile.token_endpoint,
            ),
        ] {
            if matches!(value, ProfileField::Known(_) | ProfileField::UserSupplied) {
                assert!(evidenced(field), "{}: {field:?}", profile.provider_id);
            }
        }
        if matches!(
            profile.scopes,
            ProfileField::Known(_) | ProfileField::UserSupplied
        ) {
            assert!(evidenced(pillow_runtime_slots::ProviderLoginField::Scopes));
        }
        if profile.client_id.source != ClientIdSource::Unknown
            || matches!(profile.client_id.value, ProfileField::Known(_))
        {
            assert!(evidenced(
                pillow_runtime_slots::ProviderLoginField::ClientId
            ));
        }
        if !matches!(
            profile.account_identifier,
            AccountIdentifierStrategy::Unknown
        ) {
            assert!(evidenced(
                pillow_runtime_slots::ProviderLoginField::AccountIdentifier
            ));
        }
    }
}

#[test]
fn known_openai_profile_expands_to_pkce_transport_parameters() {
    let profile = provider_login_profile("openai").unwrap();
    let client_id = runtime_text(&["user-owned-openai-", "client"]);
    let supplement = ProviderLoginSupplement {
        client_id: Some(client_id.clone()),
        issuer: Some(runtime_text(&["https://ignored.", "example.invalid"])),
        authorize_endpoint: Some(runtime_text(&[
            "https://ignored.",
            "example.invalid/authorize",
        ])),
        token_endpoint: Some(runtime_text(&["https://ignored.", "example.invalid/token"])),
        scopes: Some(vec!["ignored".into()]),
        ..ProviderLoginSupplement::default()
    };

    assert_eq!(
        resolve_oauth_transport_parameters(profile, &supplement).unwrap(),
        OauthTransportParameters::Pkce {
            issuer: "https://auth.openai.com".into(),
            authorize_endpoint: "https://auth.openai.com/oauth/authorize".into(),
            token_endpoint: "https://auth.openai.com/oauth/token".into(),
            scopes: vec![
                "openid".into(),
                "profile".into(),
                "email".into(),
                "offline_access".into(),
            ],
            client_id,
        }
    );
}

#[test]
fn xai_profile_expands_to_device_code_transport_parameters() {
    let profile = provider_login_profile("xai").unwrap();
    let client_id = runtime_text(&["user-owned-xai-", "client"]);
    let supplement = ProviderLoginSupplement {
        issuer: Some("https://auth.x.ai".into()),
        client_id: Some(client_id.clone()),
        ..ProviderLoginSupplement::default()
    };

    assert_eq!(
        resolve_oauth_transport_parameters(profile, &supplement).unwrap(),
        OauthTransportParameters::DeviceCode {
            issuer: "https://auth.x.ai".into(),
            device_authorization_endpoint: "https://auth.x.ai/oauth2/device/code".into(),
            token_endpoint: "https://auth.x.ai/oauth2/token".into(),
            scopes: vec![
                "openid".into(),
                "profile".into(),
                "email".into(),
                "offline_access".into(),
                "grok-cli:access".into(),
                "api:access".into(),
            ],
            client_id,
        }
    );
}

#[test]
fn custom_oidc_uses_only_user_supplied_public_metadata() {
    let profile = provider_login_profile("custom-oidc").unwrap();
    let supplement = complete_custom_oidc_supplement();
    let issuer = supplement.issuer.clone().unwrap();
    let authorize_endpoint = supplement.authorize_endpoint.clone().unwrap();
    let token_endpoint = supplement.token_endpoint.clone().unwrap();
    let scopes = supplement.scopes.clone().unwrap();
    let client_id = supplement.client_id.clone().unwrap();

    assert_eq!(
        resolve_oauth_transport_parameters(profile, &supplement).unwrap(),
        OauthTransportParameters::Pkce {
            issuer,
            authorize_endpoint,
            token_endpoint,
            scopes,
            client_id,
        }
    );
}

#[test]
fn conversion_rejects_missing_client_id() {
    let error = resolve_oauth_transport_parameters(
        provider_login_profile("openai").unwrap(),
        &ProviderLoginSupplement::default(),
    )
    .unwrap_err();

    assert_eq!(error, ProviderLoginConversionError::MissingClientId);
}

#[test]
fn conversion_ignores_embedded_client_id_when_source_is_unknown() {
    let embedded: &'static str =
        Box::leak(runtime_text(&["untrusted-upstream-", "client"]).into_boxed_str());
    let profile = ProviderLoginProfile {
        client_id: ClientIdProfile {
            source: ClientIdSource::Unknown,
            value: ProfileField::Known(embedded),
        },
        ..*provider_login_profile("openai").unwrap()
    };

    let error = resolve_oauth_transport_parameters(&profile, &ProviderLoginSupplement::default())
        .unwrap_err();

    assert_eq!(error, ProviderLoginConversionError::MissingClientId);
    assert!(!error.to_string().contains(embedded));
}

#[test]
fn conversion_rejects_missing_issuer() {
    let error = resolve_oauth_transport_parameters(
        provider_login_profile("xai").unwrap(),
        &ProviderLoginSupplement {
            client_id: Some(runtime_text(&["user-owned-xai-", "client"])),
            ..ProviderLoginSupplement::default()
        },
    )
    .unwrap_err();

    assert_eq!(error, ProviderLoginConversionError::MissingIssuer);
}

#[test]
fn conversion_rejects_api_key_only_method_before_oauth_fields() {
    let error = resolve_oauth_transport_parameters(
        provider_login_profile("anthropic").unwrap(),
        &ProviderLoginSupplement::default(),
    )
    .unwrap_err();

    assert_eq!(
        error,
        ProviderLoginConversionError::UnsupportedLoginMethod(ProviderLoginMethod::ApiKeyOnly)
    );
}

#[test]
fn conversion_rejects_each_other_missing_transport_field() {
    let profile = provider_login_profile("custom-oidc").unwrap();

    let mut missing_authorize = complete_custom_oidc_supplement();
    missing_authorize.authorize_endpoint = None;
    assert_eq!(
        resolve_oauth_transport_parameters(profile, &missing_authorize).unwrap_err(),
        ProviderLoginConversionError::MissingAuthorizeEndpoint
    );

    let mut missing_token = complete_custom_oidc_supplement();
    missing_token.token_endpoint = None;
    assert_eq!(
        resolve_oauth_transport_parameters(profile, &missing_token).unwrap_err(),
        ProviderLoginConversionError::MissingTokenEndpoint
    );

    let mut missing_scopes = complete_custom_oidc_supplement();
    missing_scopes.scopes = Some(Vec::new());
    assert_eq!(
        resolve_oauth_transport_parameters(profile, &missing_scopes).unwrap_err(),
        ProviderLoginConversionError::MissingScopes
    );

    let device_profile = ProviderLoginProfile {
        provider_id: "test-device",
        display_name: "Test Device",
        login_method: ProviderLoginMethod::DeviceCode,
        issuer: ProfileField::UserSupplied,
        authorize_endpoint: ProfileField::NotApplicable,
        device_authorization_endpoint: ProfileField::UserSupplied,
        token_endpoint: ProfileField::UserSupplied,
        scopes: ProfileField::UserSupplied,
        client_id: ClientIdProfile {
            source: ClientIdSource::UserSupplied,
            value: ProfileField::UserSupplied,
        },
        account_identifier: AccountIdentifierStrategy::Unknown,
        evidence: &[],
    };
    let missing_device = complete_custom_oidc_supplement();
    assert_eq!(
        resolve_oauth_transport_parameters(&device_profile, &missing_device).unwrap_err(),
        ProviderLoginConversionError::MissingDeviceAuthorizationEndpoint
    );
}

fn complete_custom_oidc_supplement() -> ProviderLoginSupplement {
    ProviderLoginSupplement {
        issuer: Some(runtime_text(&["https://issuer.", "example.invalid"])),
        authorize_endpoint: Some(runtime_text(&[
            "https://issuer.",
            "example.invalid/authorize",
        ])),
        device_authorization_endpoint: None,
        token_endpoint: Some(runtime_text(&["https://issuer.", "example.invalid/token"])),
        scopes: Some(vec!["openid".into(), "offline_access".into()]),
        client_id: Some(runtime_text(&["user-owned-custom-", "client"])),
    }
}

fn runtime_text(parts: &[&str]) -> String {
    parts.concat()
}
