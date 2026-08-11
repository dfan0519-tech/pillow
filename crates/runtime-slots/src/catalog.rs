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

pub(crate) fn fetch_openai_model_catalog(
    base_url: &str,
    api_key: &SecretBytes,
) -> Result<ModelCatalog, RuntimeSlotError> {
    let url = models_url(base_url)?;
    let response = http_get(&url, api_key)?;
    parse_openai_response(response.status, &response.body)
}

pub(crate) fn fetch_grok_model_catalog(
    executable: &Path,
    project_root: &Path,
) -> Result<ModelCatalog, RuntimeSlotError> {
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
    if receipt.exit_code != Some(0)
        || receipt.timed_out
        || !receipt.cleanup_proven
        || receipt.stdout.truncated
        || receipt.stdout.lossy_utf8
    {
        return Err(RuntimeSlotError::ModelCatalogUnavailable);
    }
    parse_grok_models_output(&receipt.stdout.text)
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

fn parse_grok_models_output(output: &str) -> Result<ModelCatalog, RuntimeSlotError> {
    if output.len() > MODEL_CATALOG_RESPONSE_BYTE_CAP {
        return Err(RuntimeSlotError::ModelCatalogInvalid);
    }
    let model_ids = validate_model_ids(
        output
            .lines()
            .filter(|line| !line.is_empty())
            .map(str::to_owned),
    )?;
    Ok(ModelCatalog::available(
        ModelCatalogSource::GrokCli,
        model_ids,
    ))
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

struct HttpResponse {
    status: u16,
    body: Vec<u8>,
}

#[cfg(not(windows))]
fn http_get(_: &str, _: &SecretBytes) -> Result<HttpResponse, RuntimeSlotError> {
    Err(RuntimeSlotError::ModelCatalogUnavailable)
}

#[cfg(windows)]
fn http_get(url: &str, api_key: &SecretBytes) -> Result<HttpResponse, RuntimeSlotError> {
    windows_http::get(url, api_key)
}

#[cfg(windows)]
mod windows_http {
    use std::ffi::{OsStr, c_void};
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::{null, null_mut};
    use std::sync::atomic::{Ordering, compiler_fence};

    use windows_sys::Win32::Networking::WinHttp::{
        URL_COMPONENTS, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_DISABLE_REDIRECTS,
        WINHTTP_FLAG_SECURE, WINHTTP_INTERNET_SCHEME_HTTP, WINHTTP_INTERNET_SCHEME_HTTPS,
        WINHTTP_OPTION_DISABLE_FEATURE, WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
        WinHttpCloseHandle, WinHttpConnect, WinHttpCrackUrl, WinHttpOpen, WinHttpOpenRequest,
        WinHttpQueryDataAvailable, WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse,
        WinHttpSendRequest, WinHttpSetOption, WinHttpSetTimeouts,
    };

    use super::{HttpResponse, MODEL_CATALOG_RESPONSE_BYTE_CAP};
    use crate::{RuntimeSlotError, SecretBytes};

    struct InternetHandle(*mut c_void);

    impl InternetHandle {
        fn new(value: *mut c_void) -> Result<Self, RuntimeSlotError> {
            if value.is_null() {
                Err(RuntimeSlotError::ModelCatalogUnavailable)
            } else {
                Ok(Self(value))
            }
        }
    }

    impl Drop for InternetHandle {
        fn drop(&mut self) {
            // SAFETY: the handle was returned by WinHTTP and is owned here.
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }

    struct SensitiveWide(Vec<u16>);

    impl SensitiveWide {
        fn authorization(api_key: &SecretBytes) -> Result<Self, RuntimeSlotError> {
            let key = std::str::from_utf8(api_key.as_bytes())
                .map_err(|_| RuntimeSlotError::ModelCatalogUnavailable)?;
            if key.is_empty() || key.chars().any(char::is_control) {
                return Err(RuntimeSlotError::ModelCatalogUnavailable);
            }
            let mut value = "Authorization: Bearer ".encode_utf16().collect::<Vec<_>>();
            value.extend(key.encode_utf16());
            value.extend("\r\nAccept: application/json\r\n".encode_utf16());
            value.push(0);
            Ok(Self(value))
        }
    }

    impl Drop for SensitiveWide {
        fn drop(&mut self) {
            for unit in &mut self.0 {
                // SAFETY: `unit` is a uniquely borrowed initialized u16.
                unsafe { std::ptr::write_volatile(unit, 0) };
            }
            compiler_fence(Ordering::SeqCst);
        }
    }

    struct CrackedUrl {
        host: Vec<u16>,
        object: Vec<u16>,
        port: u16,
        secure: bool,
    }

    pub(super) fn get(url: &str, api_key: &SecretBytes) -> Result<HttpResponse, RuntimeSlotError> {
        let cracked = crack_url(url)?;
        let agent = wide("Pillow/0.1");
        // SAFETY: all pointers are null or point to live NUL-terminated UTF-16.
        let session = InternetHandle::new(unsafe {
            WinHttpOpen(
                agent.as_ptr(),
                WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                null(),
                null(),
                0,
            )
        })?;
        // SAFETY: session is live and timeout values are bounded milliseconds.
        if unsafe { WinHttpSetTimeouts(session.0, 5_000, 5_000, 10_000, 10_000) } == 0 {
            return Err(RuntimeSlotError::ModelCatalogUnavailable);
        }
        // SAFETY: session and host remain live for the returned connection.
        let connection = InternetHandle::new(unsafe {
            WinHttpConnect(session.0, cracked.host.as_ptr(), cracked.port, 0)
        })?;
        let verb = wide("GET");
        let flags = if cracked.secure {
            WINHTTP_FLAG_SECURE
        } else {
            0
        };
        // SAFETY: connection and request strings remain live through the call.
        let request = InternetHandle::new(unsafe {
            WinHttpOpenRequest(
                connection.0,
                verb.as_ptr(),
                cracked.object.as_ptr(),
                null(),
                null(),
                null(),
                flags,
            )
        })?;
        let disable_redirects = WINHTTP_DISABLE_REDIRECTS;
        // Do not forward a bearer secret across an endpoint-controlled redirect.
        // SAFETY: request is live and option points to a u32 of the declared size.
        if unsafe {
            WinHttpSetOption(
                request.0,
                WINHTTP_OPTION_DISABLE_FEATURE,
                (&disable_redirects as *const u32).cast(),
                std::mem::size_of::<u32>() as u32,
            )
        } == 0
        {
            return Err(RuntimeSlotError::ModelCatalogUnavailable);
        }
        let headers = SensitiveWide::authorization(api_key)?;
        // SAFETY: request and NUL-terminated header buffer are live; there is no body.
        if unsafe { WinHttpSendRequest(request.0, headers.0.as_ptr(), u32::MAX, null(), 0, 0, 0) }
            == 0
        {
            return Err(RuntimeSlotError::ModelCatalogUnavailable);
        }
        // Erase the temporary wide header before receiving or parsing response data.
        drop(headers);
        // SAFETY: request is live and synchronous receive takes no reserved data.
        if unsafe { WinHttpReceiveResponse(request.0, null_mut()) } == 0 {
            return Err(RuntimeSlotError::ModelCatalogUnavailable);
        }
        let status = query_status(request.0)?;
        if matches!(status, 404 | 405 | 501) || !(200..300).contains(&status) {
            return Ok(HttpResponse {
                status,
                body: Vec::new(),
            });
        }
        let body = read_bounded_body(request.0)?;
        Ok(HttpResponse { status, body })
    }

    fn query_status(request: *mut c_void) -> Result<u16, RuntimeSlotError> {
        let mut status = 0_u32;
        let mut size = std::mem::size_of::<u32>() as u32;
        // SAFETY: request is live; output points to an initialized u32-sized buffer.
        if unsafe {
            WinHttpQueryHeaders(
                request,
                WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                null(),
                (&mut status as *mut u32).cast(),
                &mut size,
                null_mut(),
            )
        } == 0
        {
            return Err(RuntimeSlotError::ModelCatalogUnavailable);
        }
        u16::try_from(status).map_err(|_| RuntimeSlotError::ModelCatalogUnavailable)
    }

    fn read_bounded_body(request: *mut c_void) -> Result<Vec<u8>, RuntimeSlotError> {
        let mut body = Vec::new();
        loop {
            let mut available = 0_u32;
            // SAFETY: request is live and output points to a u32.
            if unsafe { WinHttpQueryDataAvailable(request, &mut available) } == 0 {
                return Err(RuntimeSlotError::ModelCatalogUnavailable);
            }
            if available == 0 {
                return Ok(body);
            }
            let remaining = MODEL_CATALOG_RESPONSE_BYTE_CAP
                .saturating_add(1)
                .saturating_sub(body.len());
            if remaining == 0 {
                return Err(RuntimeSlotError::ModelCatalogInvalid);
            }
            let requested = (available as usize).min(8_192).min(remaining);
            let mut chunk = vec![0_u8; requested];
            let mut read = 0_u32;
            // SAFETY: request is live and chunk exposes `requested` writable bytes.
            if unsafe {
                WinHttpReadData(
                    request,
                    chunk.as_mut_ptr().cast(),
                    requested as u32,
                    &mut read,
                )
            } == 0
            {
                return Err(RuntimeSlotError::ModelCatalogUnavailable);
            }
            if read == 0 {
                return Ok(body);
            }
            chunk.truncate(read as usize);
            body.extend_from_slice(&chunk);
            if body.len() > MODEL_CATALOG_RESPONSE_BYTE_CAP {
                return Err(RuntimeSlotError::ModelCatalogInvalid);
            }
        }
    }

    fn crack_url(url: &str) -> Result<CrackedUrl, RuntimeSlotError> {
        let wide_url = wide(url);
        // SAFETY: URL_COMPONENTS is a C POD whose all-zero state is its documented initializer.
        let mut components: URL_COMPONENTS = unsafe { std::mem::zeroed() };
        components.dwStructSize = std::mem::size_of::<URL_COMPONENTS>() as u32;
        components.dwSchemeLength = u32::MAX;
        components.dwHostNameLength = u32::MAX;
        components.dwUserNameLength = u32::MAX;
        components.dwPasswordLength = u32::MAX;
        components.dwUrlPathLength = u32::MAX;
        components.dwExtraInfoLength = u32::MAX;
        // SAFETY: input is live NUL-terminated UTF-16 and output has a valid size.
        if unsafe { WinHttpCrackUrl(wide_url.as_ptr(), 0, 0, &mut components) } == 0 {
            return Err(RuntimeSlotError::ModelCatalogUnavailable);
        }
        if components.dwUserNameLength != 0 || components.dwPasswordLength != 0 {
            return Err(RuntimeSlotError::ModelCatalogUnavailable);
        }
        let secure = match components.nScheme {
            WINHTTP_INTERNET_SCHEME_HTTP => false,
            WINHTTP_INTERNET_SCHEME_HTTPS => true,
            _ => return Err(RuntimeSlotError::ModelCatalogUnavailable),
        };
        let mut host = copy_component(components.lpszHostName, components.dwHostNameLength)?;
        if host.is_empty() {
            return Err(RuntimeSlotError::ModelCatalogUnavailable);
        }
        host.push(0);
        let mut object = copy_component(components.lpszUrlPath, components.dwUrlPathLength)?;
        object.extend(copy_component(
            components.lpszExtraInfo,
            components.dwExtraInfoLength,
        )?);
        if object.is_empty() {
            object.push('/' as u16);
        }
        object.push(0);
        Ok(CrackedUrl {
            host,
            object,
            port: components.nPort,
            secure,
        })
    }

    fn copy_component(pointer: *mut u16, length: u32) -> Result<Vec<u16>, RuntimeSlotError> {
        if length == 0 {
            return Ok(Vec::new());
        }
        if pointer.is_null() {
            return Err(RuntimeSlotError::ModelCatalogUnavailable);
        }
        // SAFETY: WinHttpCrackUrl returned this pointer/length into the live URL buffer.
        Ok(unsafe { std::slice::from_raw_parts(pointer, length as usize) }.to_vec())
    }

    fn wide(value: &str) -> Vec<u16> {
        OsStr::new(value).encode_wide().chain(Some(0)).collect()
    }
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

    #[test]
    fn normal_grok_cli_output_produces_a_real_model_list() {
        let catalog = parse_grok_models_output("grok-4.5\n").unwrap();
        assert_eq!(catalog.source, ModelCatalogSource::GrokCli);
        assert_eq!(catalog.availability, ModelCatalogAvailability::Available);
        assert_eq!(catalog.model_ids, ["grok-4.5"]);
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
