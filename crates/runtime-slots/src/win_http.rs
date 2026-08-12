//! Shared Windows WinHTTP client for runtime-slots.
//!
//! Single transport path for model-catalog GET and OAuth token POST.
//! Never logs request or response bodies (may contain secrets).

use crate::RuntimeSlotError;
use crate::secret::SecretBytes;

/// Bounded HTTP response. Body may be empty on non-success for some callers.
/// No Debug: body may hold tokens.
pub(crate) struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

/// Timeouts in milliseconds for WinHttpSetTimeouts
/// (resolve, connect, send, receive).
#[derive(Debug, Clone, Copy)]
pub struct HttpTimeouts {
    pub resolve_ms: i32,
    pub connect_ms: i32,
    pub send_ms: i32,
    pub receive_ms: i32,
}

impl Default for HttpTimeouts {
    fn default() -> Self {
        Self {
            resolve_ms: 5_000,
            connect_ms: 5_000,
            send_ms: 10_000,
            receive_ms: 10_000,
        }
    }
}

/// Low-level transport failure (before a status line is obtained).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TransportFailure {
    Timeout,
    Unavailable,
}

impl TransportFailure {
    pub(crate) fn into_catalog_error(self) -> RuntimeSlotError {
        RuntimeSlotError::ModelCatalogUnavailable
    }

    pub(crate) fn into_oauth_error(self) -> RuntimeSlotError {
        match self {
            Self::Timeout => RuntimeSlotError::OauthHttpTimeout,
            Self::Unavailable => RuntimeSlotError::OauthHttpUnavailable,
        }
    }
}

#[cfg(not(windows))]
pub(crate) fn get_bearer(
    _: &str,
    _: &SecretBytes,
    _: HttpTimeouts,
    _: usize,
) -> Result<HttpResponse, TransportFailure> {
    Err(TransportFailure::Unavailable)
}

#[cfg(not(windows))]
pub(crate) fn post_form(
    _: &str,
    _: &[u8],
    _: HttpTimeouts,
    _: usize,
) -> Result<HttpResponse, TransportFailure> {
    Err(TransportFailure::Unavailable)
}

#[cfg(windows)]
pub(crate) fn get_bearer(
    url: &str,
    api_key: &SecretBytes,
    timeouts: HttpTimeouts,
    body_cap: usize,
) -> Result<HttpResponse, TransportFailure> {
    platform::request(
        url,
        "GET",
        Some(api_key),
        None,
        timeouts,
        body_cap,
        /* return_body_on_non_success */ false,
    )
}

#[cfg(windows)]
pub(crate) fn post_form(
    url: &str,
    form_body: &[u8],
    timeouts: HttpTimeouts,
    body_cap: usize,
) -> Result<HttpResponse, TransportFailure> {
    platform::request(
        url,
        "POST",
        None,
        Some(form_body),
        timeouts,
        body_cap,
        /* return_body_on_non_success */ true,
    )
}

#[cfg(windows)]
mod platform {
    use std::ffi::{OsStr, c_void};
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::{null, null_mut};
    use std::sync::atomic::{Ordering, compiler_fence};

    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::Networking::WinHttp::{
        URL_COMPONENTS, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_DISABLE_REDIRECTS,
        WINHTTP_FLAG_SECURE, WINHTTP_INTERNET_SCHEME_HTTP, WINHTTP_INTERNET_SCHEME_HTTPS,
        WINHTTP_OPTION_DISABLE_FEATURE, WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
        WinHttpCloseHandle, WinHttpConnect, WinHttpCrackUrl, WinHttpOpen, WinHttpOpenRequest,
        WinHttpQueryDataAvailable, WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse,
        WinHttpSendRequest, WinHttpSetOption, WinHttpSetTimeouts,
    };

    /// Win32 `ERROR_WINHTTP_TIMEOUT` (0x2EE2).
    const ERROR_WINHTTP_TIMEOUT: u32 = 12_002;

    use super::{HttpResponse, HttpTimeouts, TransportFailure};
    use crate::secret::SecretBytes;

    struct InternetHandle(*mut c_void);

    impl InternetHandle {
        fn new(value: *mut c_void) -> Result<Self, TransportFailure> {
            if value.is_null() {
                Err(map_last_error())
            } else {
                Ok(Self(value))
            }
        }
    }

    impl Drop for InternetHandle {
        fn drop(&mut self) {
            // SAFETY: handle was returned by WinHTTP and is owned here.
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }

    struct SensitiveWide(Vec<u16>);

    impl SensitiveWide {
        fn authorization(api_key: &SecretBytes) -> Result<Self, TransportFailure> {
            let key = std::str::from_utf8(api_key.as_bytes())
                .map_err(|_| TransportFailure::Unavailable)?;
            if key.is_empty() || key.chars().any(char::is_control) {
                return Err(TransportFailure::Unavailable);
            }
            let mut value = "Authorization: Bearer ".encode_utf16().collect::<Vec<_>>();
            value.extend(key.encode_utf16());
            value.extend("\r\nAccept: application/json\r\n".encode_utf16());
            value.push(0);
            Ok(Self(value))
        }

        fn form_headers() -> Self {
            let mut value =
                "Content-Type: application/x-www-form-urlencoded\r\nAccept: application/json\r\n"
                    .encode_utf16()
                    .collect::<Vec<_>>();
            value.push(0);
            Self(value)
        }
    }

    impl Drop for SensitiveWide {
        fn drop(&mut self) {
            for unit in &mut self.0 {
                // SAFETY: unit is a uniquely borrowed initialized u16.
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

    pub(super) fn request(
        url: &str,
        verb: &str,
        bearer: Option<&SecretBytes>,
        body: Option<&[u8]>,
        timeouts: HttpTimeouts,
        body_cap: usize,
        return_body_on_non_success: bool,
    ) -> Result<HttpResponse, TransportFailure> {
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
        if unsafe {
            WinHttpSetTimeouts(
                session.0,
                timeouts.resolve_ms,
                timeouts.connect_ms,
                timeouts.send_ms,
                timeouts.receive_ms,
            )
        } == 0
        {
            return Err(map_last_error());
        }
        // SAFETY: session and host remain live for the returned connection.
        let connection = InternetHandle::new(unsafe {
            WinHttpConnect(session.0, cracked.host.as_ptr(), cracked.port, 0)
        })?;
        let verb_w = wide(verb);
        let flags = if cracked.secure {
            WINHTTP_FLAG_SECURE
        } else {
            0
        };
        // SAFETY: connection and request strings remain live through the call.
        let request = InternetHandle::new(unsafe {
            WinHttpOpenRequest(
                connection.0,
                verb_w.as_ptr(),
                cracked.object.as_ptr(),
                null(),
                null(),
                null(),
                flags,
            )
        })?;
        let disable_redirects = WINHTTP_DISABLE_REDIRECTS;
        // Do not follow redirects (token endpoints must not bounce secrets).
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
            return Err(map_last_error());
        }

        let headers = match bearer {
            Some(key) => SensitiveWide::authorization(key)?,
            None => SensitiveWide::form_headers(),
        };

        let (body_ptr, body_len) = match body {
            Some(bytes) if !bytes.is_empty() => (bytes.as_ptr() as *mut c_void, bytes.len() as u32),
            _ => (null_mut(), 0_u32),
        };

        // SAFETY: request and header buffer are live; body points to form bytes or null.
        if unsafe {
            WinHttpSendRequest(
                request.0,
                headers.0.as_ptr(),
                u32::MAX,
                body_ptr,
                body_len,
                body_len,
                0,
            )
        } == 0
        {
            drop(headers);
            return Err(map_last_error());
        }
        drop(headers);

        // SAFETY: request is live and synchronous receive takes no reserved data.
        if unsafe { WinHttpReceiveResponse(request.0, null_mut()) } == 0 {
            return Err(map_last_error());
        }
        let status = query_status(request.0)?;
        if !return_body_on_non_success
            && (matches!(status, 404 | 405 | 501) || !(200..300).contains(&status))
        {
            return Ok(HttpResponse {
                status,
                body: Vec::new(),
            });
        }
        let body = read_bounded_body(request.0, body_cap)?;
        Ok(HttpResponse { status, body })
    }

    fn map_last_error() -> TransportFailure {
        // SAFETY: GetLastError is process-local and called immediately after failure.
        let code = unsafe { GetLastError() };
        if code == ERROR_WINHTTP_TIMEOUT {
            TransportFailure::Timeout
        } else {
            TransportFailure::Unavailable
        }
    }

    fn query_status(request: *mut c_void) -> Result<u16, TransportFailure> {
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
            return Err(map_last_error());
        }
        u16::try_from(status).map_err(|_| TransportFailure::Unavailable)
    }

    fn read_bounded_body(
        request: *mut c_void,
        body_cap: usize,
    ) -> Result<Vec<u8>, TransportFailure> {
        let mut body = Vec::new();
        loop {
            let mut available = 0_u32;
            // SAFETY: request is live and output points to a u32.
            if unsafe { WinHttpQueryDataAvailable(request, &mut available) } == 0 {
                return Err(map_last_error());
            }
            if available == 0 {
                return Ok(body);
            }
            let remaining = body_cap.saturating_add(1).saturating_sub(body.len());
            if remaining == 0 {
                return Err(TransportFailure::Unavailable);
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
                return Err(map_last_error());
            }
            if read == 0 {
                return Ok(body);
            }
            chunk.truncate(read as usize);
            body.extend_from_slice(&chunk);
            if body.len() > body_cap {
                return Err(TransportFailure::Unavailable);
            }
        }
    }

    fn crack_url(url: &str) -> Result<CrackedUrl, TransportFailure> {
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
            return Err(map_last_error());
        }
        if components.dwUserNameLength != 0 || components.dwPasswordLength != 0 {
            return Err(TransportFailure::Unavailable);
        }
        let secure = match components.nScheme {
            WINHTTP_INTERNET_SCHEME_HTTP => false,
            WINHTTP_INTERNET_SCHEME_HTTPS => true,
            _ => return Err(TransportFailure::Unavailable),
        };
        let mut host = copy_component(components.lpszHostName, components.dwHostNameLength)?;
        if host.is_empty() {
            return Err(TransportFailure::Unavailable);
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

    fn copy_component(pointer: *mut u16, length: u32) -> Result<Vec<u16>, TransportFailure> {
        if length == 0 {
            return Ok(Vec::new());
        }
        if pointer.is_null() {
            return Err(TransportFailure::Unavailable);
        }
        // SAFETY: WinHttpCrackUrl returned this pointer/length into the live URL buffer.
        Ok(unsafe { std::slice::from_raw_parts(pointer, length as usize) }.to_vec())
    }

    fn wide(value: &str) -> Vec<u16> {
        OsStr::new(value).encode_wide().chain(Some(0)).collect()
    }
}
