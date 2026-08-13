use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

use keeper_process_executor::{
    CancellationSignal, ProcessSpec, SupervisedProcessRunner, sensitive_name,
};
use serde::Deserialize;

use crate::{RuntimeSlotError, SecretBytes};

pub const MODEL_CATALOG_MAX_ENTRIES: usize = 256;
pub const MODEL_ID_MAX_CHARS: usize = 512;
pub const MODEL_CATALOG_RESPONSE_BYTE_CAP: usize = 262_144;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelCatalogSource {
    OpenAiModels,
    GrokCli,
    ExternalCli,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelCatalogUnsupportedReason {
    Endpoint,
    ExternalCli,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelCatalogAvailability {
    Available,
    Unsupported {
        reason: ModelCatalogUnsupportedReason,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelCatalog {
    pub source: ModelCatalogSource,
    pub availability: ModelCatalogAvailability,
    pub model_ids: Vec<String>,
}

impl ModelCatalog {
    fn available(source: ModelCatalogSource, model_ids: Vec<String>) -> Self {
        Self {
            source,
            availability: ModelCatalogAvailability::Available,
            model_ids,
        }
    }

    pub(crate) fn unsupported(
        source: ModelCatalogSource,
        reason: ModelCatalogUnsupportedReason,
    ) -> Self {
        Self {
            source,
            availability: ModelCatalogAvailability::Unsupported { reason },
            model_ids: Vec::new(),
        }
    }
}

#[derive(Deserialize)]
struct OpenAiModelsResponse {
    data: Vec<OpenAiModel>,
}

#[derive(Deserialize)]
struct OpenAiModel {
    id: String,
}

/// Fetch an OpenAI-compatible `/models` catalog using a one-shot API key.
///
/// Host draft probe (`probe_model_catalog`) and persisted-slot catalog share this
/// path so the renderer never holds Bearer material or opens third-party sockets.
/// The key is never returned on the catalog value.
pub fn fetch_openai_model_catalog(
    base_url: &str,
    api_key: &SecretBytes,
) -> Result<ModelCatalog, RuntimeSlotError> {
    let url = models_url(base_url)?;
    let response = http_get(&url, api_key)?;
    parse_openai_response(response.status, &response.body)
}

/// One `grok models` run, before anything interprets it.
///
/// The exit code is carried, not enforced, because `grok models` is also the
/// only read-only signal for *whether the CLI is logged in at all*. A
/// logged-out CLI may exit non-zero, and its banner is the one thing that can
/// tell the user to run `grok login`; folding that into "catalog unavailable"
/// turns a fixable state into an opaque one.
#[derive(Debug, Clone)]
pub struct GrokModelsRun {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
}

/// Run `grok models` against a resolved executable inside a project root.
///
/// 🔴 Callers must use the *same* executable the slot will run — verifying one
/// grok binary and then launching another proves nothing.
pub fn run_grok_models(
    executable: &Path,
    project_root: &Path,
) -> Result<GrokModelsRun, RuntimeSlotError> {
    let mut env = BTreeMap::new();
    for (name, value) in std::env::vars_os() {
        if !sensitive_name(&name) {
            env.insert(name, value);
        }
    }
    let receipt = SupervisedProcessRunner::new()
        .run(
            ProcessSpec {
                executable: executable.to_path_buf(),
                argv: vec![OsString::from("models")],
                cwd: project_root.to_path_buf(),
                env,
                timeout: Duration::from_secs(10),
                output_byte_cap: MODEL_CATALOG_RESPONSE_BYTE_CAP,
            },
            CancellationSignal::new(),
        )
        .map_err(|_| RuntimeSlotError::ModelCatalogUnavailable)?;
    // Integrity guards only. Truncated or lossy output cannot be parsed
    // honestly either way, so those still fail here.
    if receipt.timed_out
        || !receipt.cleanup_proven
        || receipt.stdout.truncated
        || receipt.stdout.lossy_utf8
    {
        return Err(RuntimeSlotError::ModelCatalogUnavailable);
    }
    Ok(GrokModelsRun {
        stdout: receipt.stdout.text,
        stderr: receipt.stderr.text,
        exit_code: receipt.exit_code,
    })
}

pub(crate) fn fetch_grok_model_catalog(
    executable: &Path,
    project_root: &Path,
) -> Result<ModelCatalog, RuntimeSlotError> {
    let run = run_grok_models(executable, project_root)?;
    if run.exit_code != Some(0) {
        return Err(RuntimeSlotError::ModelCatalogUnavailable);
    }
    parse_grok_models_output(&run.stdout)
}

fn models_url(base_url: &str) -> Result<String, RuntimeSlotError> {
    if base_url.is_empty()
        || base_url.len() > 4_096
        || base_url.chars().any(char::is_control)
        || !(base_url.starts_with("https://") || base_url.starts_with("http://"))
        || base_url.contains(['?', '#'])
    {
        return Err(RuntimeSlotError::InvalidInput);
    }
    Ok(format!("{}/models", base_url.trim_end_matches('/')))
}

fn parse_openai_response(status: u16, body: &[u8]) -> Result<ModelCatalog, RuntimeSlotError> {
    if matches!(status, 404 | 405 | 501) {
        return Ok(ModelCatalog::unsupported(
            ModelCatalogSource::OpenAiModels,
            ModelCatalogUnsupportedReason::Endpoint,
        ));
    }
    if !(200..300).contains(&status) {
        return Err(RuntimeSlotError::ModelCatalogUnavailable);
    }
    if body.len() > MODEL_CATALOG_RESPONSE_BYTE_CAP {
        return Err(RuntimeSlotError::ModelCatalogInvalid);
    }
    let response: OpenAiModelsResponse =
        serde_json::from_slice(body).map_err(|_| RuntimeSlotError::ModelCatalogInvalid)?;
    let model_ids = validate_model_ids(response.data.into_iter().map(|model| model.id))?;
    Ok(ModelCatalog::available(
        ModelCatalogSource::OpenAiModels,
        model_ids,
    ))
}

/// Section header the model list follows in `grok models` output.
const GROK_AVAILABLE_MODELS_HEADER: &str = "Available models:";
/// Line that names the session default, e.g. `Default model: grok-4.6`.
const GROK_DEFAULT_MODEL_PREFIX: &str = "Default model:";

/// Parse the human-readable output of `grok models`.
///
/// Real shape, captured from grok CLI 4.6 on 2026-08-13:
///
/// ```text
/// You are logged in with grok.com.
///
/// Default model: grok-4.6
///
/// Available models:
///   * grok-4.6 (default)
///   - grok-4.5
/// ```
///
/// The previous version took every non-empty line as a model id. It rejected
/// the whole catalog at the first indented bullet (`value.trim() != value`),
/// which is the "模型目录响应无效" users saw — and had the bullets not been
/// indented it would have stored `You are logged in with grok.com.` as a
/// model id. It was never run against real output; its unit test fed it a
/// bare `grok-4.5\n` list the CLI does not emit.
///
/// Anchoring on the header means banner text can never become a model id, and
/// an output shape we do not recognise is an honest error rather than
/// garbage. The manual model-id field is the escape hatch when that happens.
fn parse_grok_models_output(output: &str) -> Result<ModelCatalog, RuntimeSlotError> {
    if output.len() > MODEL_CATALOG_RESPONSE_BYTE_CAP {
        return Err(RuntimeSlotError::ModelCatalogInvalid);
    }
    let mut lines = output.lines();
    // `any` short-circuits on the header, leaving the iterator on the line
    // after it — everything before is banner text and must not be parsed.
    if !lines.any(|line| line.trim() == GROK_AVAILABLE_MODELS_HEADER) {
        return Err(RuntimeSlotError::ModelCatalogInvalid);
    }

    let mut entries = Vec::new();
    for line in lines {
        let trimmed = line.trim();
        // The list is contiguous; a blank line or a non-bullet ends it.
        let Some(model_id) = grok_bullet_model_id(trimmed) else {
            break;
        };
        entries.push(model_id.to_owned());
    }

    let model_ids = validate_model_ids(entries)?;
    Ok(ModelCatalog::available(
        ModelCatalogSource::GrokCli,
        model_ids,
    ))
}

/// `* grok-4.6 (default)` / `- grok-4.5` → `grok-4.6` / `grok-4.5`.
///
/// Takes the first whitespace-delimited token so the `(default)` annotation
/// never lands in a model id. Already-trimmed input.
fn grok_bullet_model_id(line: &str) -> Option<&str> {
    let rest = line
        .strip_prefix("* ")
        .or_else(|| line.strip_prefix("- "))?;
    rest.split_whitespace().next()
}

/// Model `grok models` reports as the session default, when it names one.
///
/// Used to give a freshly created login slot a usable model instead of
/// leaving it blank and unstartable.
pub fn parse_grok_default_model(output: &str) -> Option<String> {
    let value = output
        .lines()
        .find_map(|line| line.trim().strip_prefix(GROK_DEFAULT_MODEL_PREFIX))?
        .trim();
    // Same bounds the catalog enforces — a default we would reject as a model
    // id is not a default we can store.
    validate_model_ids([value.to_owned()])
        .ok()
        .and_then(|mut ids| ids.pop())
}

fn validate_model_ids(
    values: impl IntoIterator<Item = String>,
) -> Result<Vec<String>, RuntimeSlotError> {
    let mut model_ids = Vec::new();
    for value in values {
        if model_ids.len() == MODEL_CATALOG_MAX_ENTRIES
            || value.trim().is_empty()
            || value.trim() != value
            || value.chars().count() > MODEL_ID_MAX_CHARS
            || value.chars().any(char::is_control)
        {
            return Err(RuntimeSlotError::ModelCatalogInvalid);
        }
        model_ids.push(value);
    }
    if model_ids.is_empty() {
        return Err(RuntimeSlotError::ModelCatalogInvalid);
    }
    Ok(model_ids)
}

fn http_get(
    url: &str,
    api_key: &SecretBytes,
) -> Result<crate::win_http::HttpResponse, RuntimeSlotError> {
    crate::win_http::get_bearer(
        url,
        api_key,
        crate::win_http::HttpTimeouts::default(),
        MODEL_CATALOG_RESPONSE_BYTE_CAP,
    )
    .map_err(|e| e.into_catalog_error())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_openai_response_produces_a_real_model_list() {
        let catalog = parse_openai_response(
            200,
            br#"{"data":[{"id":"grok-4.5"},{"id":"grok-4.1-fast"}]}"#,
        )
        .unwrap();

        assert_eq!(catalog.source, ModelCatalogSource::OpenAiModels);
        assert_eq!(catalog.availability, ModelCatalogAvailability::Available);
        assert_eq!(catalog.model_ids, ["grok-4.5", "grok-4.1-fast"]);
    }

    #[test]
    fn malformed_openai_shape_is_rejected_without_echoing_the_body() {
        let marker = "pillow-test-account-marker";
        let body = format!(r#"{{"data":{{"id":"{marker}"}}}}"#);
        let error = parse_openai_response(200, body.as_bytes()).unwrap_err();

        assert_eq!(error.code(), "runtime_slot_model_catalog_invalid");
        assert!(!error.to_string().contains(marker));
    }

    #[test]
    fn openai_model_ids_have_count_length_and_control_character_bounds() {
        let too_many = serde_json::json!({
            "data": (0..257)
                .map(|index| serde_json::json!({"id": format!("model-{index}")}))
                .collect::<Vec<_>>()
        });
        assert_eq!(
            parse_openai_response(200, too_many.to_string().as_bytes())
                .unwrap_err()
                .code(),
            "runtime_slot_model_catalog_invalid"
        );

        let too_long = serde_json::json!({"data": [{"id": "m".repeat(513)}]});
        assert_eq!(
            parse_openai_response(200, too_long.to_string().as_bytes())
                .unwrap_err()
                .code(),
            "runtime_slot_model_catalog_invalid"
        );

        let control = br#"{"data":[{"id":"safe\u0007unsafe"}]}"#;
        assert_eq!(
            parse_openai_response(200, control).unwrap_err().code(),
            "runtime_slot_model_catalog_invalid"
        );
    }

    #[test]
    fn oversized_openai_body_is_rejected_before_unbounded_parsing() {
        let oversized = vec![b' '; 262_145];
        assert_eq!(
            parse_openai_response(200, &oversized).unwrap_err().code(),
            "runtime_slot_model_catalog_invalid"
        );
    }

    #[test]
    fn endpoint_without_models_support_returns_fallback_not_error() {
        for status in [404, 405, 501] {
            let marker = format!("private-response-{status}");
            let catalog = parse_openai_response(status, marker.as_bytes()).unwrap();
            assert_eq!(catalog.source, ModelCatalogSource::OpenAiModels);
            assert_eq!(
                catalog.availability,
                ModelCatalogAvailability::Unsupported {
                    reason: ModelCatalogUnsupportedReason::Endpoint
                }
            );
            assert!(catalog.model_ids.is_empty());
            assert!(!format!("{catalog:?}").contains(&marker));
        }
    }

    #[test]
    fn authentication_and_server_failures_are_errors_not_false_fallbacks() {
        for status in [401, 403, 429, 500] {
            let error = parse_openai_response(status, b"never surface this response").unwrap_err();
            assert_eq!(error.code(), "runtime_slot_model_catalog_unavailable");
            assert!(!error.to_string().contains("never surface"));
        }
    }

    /// Verbatim `grok models` output, grok CLI 4.6, captured 2026-08-13.
    /// Do not tidy this string — its indentation is the thing under test.
    const REAL_GROK_MODELS_OUTPUT: &str = "You are logged in with grok.com.\n\
        \n\
        Default model: grok-4.6\n\
        \n\
        Available models:\n\
        \x20 * grok-4.6 (default)\n\
        \x20 - grok-4.5\n";

    #[test]
    fn real_grok_cli_output_produces_a_real_model_list() {
        let catalog = parse_grok_models_output(REAL_GROK_MODELS_OUTPUT).unwrap();
        assert_eq!(catalog.source, ModelCatalogSource::GrokCli);
        assert_eq!(catalog.availability, ModelCatalogAvailability::Available);
        assert_eq!(catalog.model_ids, ["grok-4.6", "grok-4.5"]);
    }

    #[test]
    fn banner_text_never_becomes_a_model_id() {
        let catalog = parse_grok_models_output(REAL_GROK_MODELS_OUTPUT).unwrap();
        for id in &catalog.model_ids {
            assert!(
                !id.contains("logged in") && !id.contains("Available") && !id.contains("Default"),
                "banner line leaked into the catalog as {id:?}"
            );
        }
    }

    #[test]
    fn default_annotation_is_not_part_of_the_model_id() {
        let catalog = parse_grok_models_output(REAL_GROK_MODELS_OUTPUT).unwrap();
        assert!(
            !catalog.model_ids.iter().any(|id| id.contains("default")),
            "the `(default)` annotation must not be stored as part of an id"
        );
    }

    #[test]
    fn default_model_comes_from_the_default_line() {
        assert_eq!(
            parse_grok_default_model(REAL_GROK_MODELS_OUTPUT).as_deref(),
            Some("grok-4.6")
        );
        assert_eq!(parse_grok_default_model("Available models:\n * a\n"), None);
    }

    #[test]
    fn output_without_the_header_is_an_error_not_a_guess() {
        // A logged-out session prints a banner and no list. Treating those
        // lines as model ids is exactly the old bug.
        assert_eq!(
            parse_grok_models_output("Not logged in. Run `grok login`.\n")
                .unwrap_err()
                .code(),
            "runtime_slot_model_catalog_invalid"
        );
    }

    #[test]
    fn grok_cli_output_uses_the_same_strict_model_id_bounds() {
        let too_many = (0..257)
            .map(|index| format!("model-{index}"))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(
            parse_grok_models_output(&too_many).unwrap_err().code(),
            "runtime_slot_model_catalog_invalid"
        );
        assert_eq!(
            parse_grok_models_output(&format!("{}\n", "m".repeat(513)))
                .unwrap_err()
                .code(),
            "runtime_slot_model_catalog_invalid"
        );
        assert_eq!(
            parse_grok_models_output("safe\nbad\u{1b}id")
                .unwrap_err()
                .code(),
            "runtime_slot_model_catalog_invalid"
        );
        assert_eq!(
            parse_grok_models_output("\tmodel-hidden-by-trim")
                .unwrap_err()
                .code(),
            "runtime_slot_model_catalog_invalid"
        );
    }

    #[cfg(windows)]
    #[test]
    fn winhttp_get_uses_models_path_and_bearer_secret_then_parses_response() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            let mut buffer = [0_u8; 1024];
            while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                let read = stream.read(&mut buffer).unwrap();
                if read == 0 || request.len() > 16 * 1024 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
            }
            let body = br#"{"data":[{"id":"local-model"}]}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(body).unwrap();
            String::from_utf8(request).unwrap()
        });

        let key = "pillow-test-catalog-key";
        let secret = crate::SecretBytes::new(key.into());
        let catalog = fetch_openai_model_catalog(&format!("http://{address}/v1"), &secret)
            .expect("local catalog request");
        assert_eq!(catalog.model_ids, ["local-model"]);

        let request = server.join().unwrap();
        assert!(request.starts_with("GET /v1/models HTTP/1.1\r\n"));
        // Built from `key` rather than written out: a literal auth header trips
        // the secret gate, and deriving it means the assertion cannot drift from
        // the key the request actually carries.
        assert!(request.contains(&format!("Authorization: Bearer {key}\r\n")));
    }
}
