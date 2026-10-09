//! GitHub OAuth device authorization; no embedded client secret or callback server.
//!
//! Protocol: https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps
use anyhow::{Result, anyhow, bail};
use reqwest::{blocking::Client, redirect::Policy};
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use zeroize::{Zeroize, Zeroizing};

/// Public OAuth application identifier, never a client secret.
pub const CLIENT_ID: &str = "Ov23liCd4JJ0qhSljlgn";
const DEVICE_ENDPOINT: &str = "https://github.com/login/device/code";
const TOKEN_ENDPOINT: &str = "https://github.com/login/oauth/access_token";
const VERIFICATION_URI: &str = "https://github.com/login/device";
const SCOPES: &str = "notifications repo read:org offline_access";
const MAX_BODY: u64 = 64 * 1024;
const CANCEL_INTERVAL: Duration = Duration::from_millis(100);
const MAX_NETWORK_ERRORS: u8 = 3;

pub fn client_id() -> Option<&'static str> {
    let id = option_env!("HUSH_GITHUB_CLIENT_ID")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(CLIENT_ID)
        .trim();
    (!id.is_empty()).then_some(id)
}

/// The device code stays private and is never shown in the UI or diagnostics.
#[derive(Clone)]
pub struct DeviceCode {
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
    device_code: Zeroizing<String>,
    received_at: Instant,
}

impl Drop for DeviceCode {
    fn drop(&mut self) {
        self.user_code.zeroize();
    }
}

/// Serialize only to the operating system's credential store, never app config.
#[derive(Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_at: Option<i64>,
    #[serde(default)]
    pub refresh_expires_at: Option<i64>,
    pub client_id: String,
}

impl Drop for Credentials {
    fn drop(&mut self) {
        self.access_token.zeroize();
        self.refresh_token.zeroize();
    }
}

#[derive(Deserialize)]
struct DeviceResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    #[serde(default = "default_interval")]
    interval: u64,
}

impl Drop for DeviceResponse {
    fn drop(&mut self) {
        self.device_code.zeroize();
        self.user_code.zeroize();
    }
}

#[derive(Default, Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    token_type: Option<String>,
    expires_in: Option<u64>,
    refresh_token_expires_in: Option<u64>,
    error: Option<String>,
    interval: Option<u64>,
}

impl Drop for TokenResponse {
    fn drop(&mut self) {
        self.access_token.zeroize();
        self.refresh_token.zeroize();
    }
}

fn default_interval() -> u64 {
    5
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .min(i64::MAX as u64) as i64
}

fn validate_client_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id.len() > 200
        || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        bail!("Die GitHub-Anmeldung ist in diesem Build nicht konfiguriert.");
    }
    Ok(())
}

fn valid_secret(value: &str) -> bool {
    !value.is_empty() && value.len() <= 4096 && value.bytes().all(|b| b.is_ascii_graphic())
}

fn expiry(seconds: Option<u64>, at: i64) -> Result<Option<i64>> {
    seconds
        .map(|seconds| {
            let seconds = i64::try_from(seconds)
                .ok()
                .filter(|seconds| *seconds > 0)
                .ok_or_else(|| anyhow!("GitHub hat eine ungültige Token-Laufzeit geliefert."))?;
            at.checked_add(seconds)
                .ok_or_else(|| anyhow!("GitHub hat eine ungültige Token-Laufzeit geliefert."))
        })
        .transpose()
}

fn decode_device(body: &[u8]) -> Result<DeviceCode> {
    // The service's error descriptions and malformed response bodies may contain
    // credentials. Only locally defined messages may escape this module.
    if let Ok(response) = serde_json::from_slice::<TokenResponse>(body)
        && let Some(error) = response.error.as_deref()
    {
        bail!(oauth_error(error));
    }
    let mut response: DeviceResponse = serde_json::from_slice(body)
        .map_err(|_| anyhow!("GitHub hat ungültige Anmeldedaten geliefert."))?;
    if response.verification_uri != VERIFICATION_URI
        || !valid_secret(&response.device_code)
        || response.user_code.len() != 9
        || !response.user_code.bytes().enumerate().all(|(index, byte)| {
            if index == 4 {
                byte == b'-'
            } else {
                byte.is_ascii_uppercase() || byte.is_ascii_digit()
            }
        })
        || response.expires_in == 0
        || response.expires_in > 86_400
        || response.interval == 0
        || response.interval > response.expires_in
    {
        bail!("GitHub hat ungültige Anmeldedaten geliefert.");
    }
    Ok(DeviceCode {
        user_code: std::mem::take(&mut response.user_code),
        verification_uri: std::mem::take(&mut response.verification_uri),
        expires_in: response.expires_in,
        interval: response.interval,
        device_code: Zeroizing::new(std::mem::take(&mut response.device_code)),
        received_at: Instant::now(),
    })
}

enum TokenOutcome {
    Authorized(Credentials),
    Pending,
    SlowDown(Option<u64>),
}

fn decode_token(body: &[u8], client_id: &str, at: i64) -> Result<TokenOutcome> {
    let mut response: TokenResponse = serde_json::from_slice(body)
        .map_err(|_| anyhow!("GitHub hat eine ungültige Anmeldeantwort geliefert."))?;
    if let Some(error) = response.error.as_deref() {
        return match error {
            "authorization_pending" => Ok(TokenOutcome::Pending),
            "slow_down" => Ok(TokenOutcome::SlowDown(response.interval)),
            error => Err(anyhow!(oauth_error(error))),
        };
    }
    if !response
        .token_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("bearer"))
        || !response.access_token.as_deref().is_some_and(valid_secret)
        || response
            .refresh_token
            .as_deref()
            .is_some_and(|token| !valid_secret(token))
        || (response.expires_in.is_some() && response.refresh_token.is_none())
        || (response.refresh_token_expires_in.is_some() && response.refresh_token.is_none())
    {
        bail!("GitHub hat unvollständige Anmeldedaten geliefert. Bitte erneut anmelden.");
    }
    let expires_at = expiry(response.expires_in, at)?;
    let refresh_expires_at = expiry(response.refresh_token_expires_in, at)?;
    Ok(TokenOutcome::Authorized(Credentials {
        access_token: response.access_token.take().unwrap_or_default(),
        refresh_token: response.refresh_token.take(),
        expires_at,
        refresh_expires_at,
        client_id: client_id.to_owned(),
    }))
}

fn oauth_error(code: &str) -> &'static str {
    match code {
        "access_denied" => "Die GitHub-Anmeldung wurde abgelehnt.",
        "expired_token" | "token_expired" | "incorrect_device_code" => {
            "Der GitHub-Anmeldecode ist abgelaufen oder ungültig. Bitte erneut anmelden."
        }
        "device_flow_disabled" => {
            "Die Geräteanmeldung ist für die GitHub-App noch nicht aktiviert."
        }
        "incorrect_client_credentials" | "invalid_client" => {
            "Die GitHub-App-ID ist ungültig. Bitte die App-Konfiguration prüfen."
        }
        "bad_refresh_token" | "invalid_grant" => {
            "Die GitHub-Anmeldung ist abgelaufen. Bitte erneut anmelden."
        }
        _ => "GitHub konnte die Anmeldung nicht abschließen. Bitte erneut versuchen.",
    }
}

enum RequestError {
    Retryable,
    Fatal(&'static str),
    Cancelled,
    Expired,
}

impl RequestError {
    fn into_error(self) -> anyhow::Error {
        anyhow!(match self {
            Self::Retryable => "GitHub ist nicht erreichbar. Bitte später erneut versuchen.",
            Self::Fatal(message) => message,
            Self::Cancelled => "Die GitHub-Anmeldung wurde abgebrochen.",
            Self::Expired => "Der GitHub-Anmeldecode ist abgelaufen. Bitte erneut anmelden.",
        })
    }
}

struct SecretForm(Vec<(&'static str, String)>);

impl Drop for SecretForm {
    fn drop(&mut self) {
        for (_, value) in &mut self.0 {
            value.zeroize();
        }
    }
}

fn http_client() -> Result<Client> {
    Client::builder()
        .https_only(true)
        .redirect(Policy::none())
        .no_proxy()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(20))
        .user_agent(concat!("Hush/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| anyhow!("Die sichere Verbindung zu GitHub konnte nicht vorbereitet werden."))
}

fn post(
    client: &Client,
    endpoint: &'static str,
    form: SecretForm,
) -> std::result::Result<Zeroizing<Vec<u8>>, RequestError> {
    // Endpoints are static and requests never follow redirects or environment proxies.
    let response = client
        .post(endpoint)
        .header(reqwest::header::ACCEPT, "application/json")
        .form(&form.0)
        .send()
        .map_err(|_| RequestError::Retryable)?;
    let status = response.status();
    if status.is_server_error() || status.as_u16() == 429 {
        return Err(RequestError::Retryable);
    }
    if status.is_redirection() {
        return Err(RequestError::Fatal(
            "GitHub hat die Anmeldung unerwartet umgeleitet.",
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_BODY)
    {
        return Err(RequestError::Fatal(
            "Die GitHub-Anmeldeantwort ist zu groß.",
        ));
    }
    let mut body = Zeroizing::new(Vec::new());
    response
        .take(MAX_BODY + 1)
        .read_to_end(&mut body)
        .map_err(|_| RequestError::Retryable)?;
    if body.len() as u64 > MAX_BODY {
        return Err(RequestError::Fatal(
            "Die GitHub-Anmeldeantwort ist zu groß.",
        ));
    }
    // OAuth errors may use 400; parse only known error codes, never their prose.
    if !status.is_success()
        && !serde_json::from_slice::<TokenResponse>(&body)
            .ok()
            .is_some_and(|response| response.error.is_some())
    {
        return Err(RequestError::Fatal(
            "GitHub konnte die Anmeldung nicht abschließen.",
        ));
    }
    Ok(body)
}

pub fn start(client_id: &str) -> Result<DeviceCode> {
    validate_client_id(client_id)?;
    let client = http_client()?;
    let body = post(
        &client,
        DEVICE_ENDPOINT,
        SecretForm(vec![
            ("client_id", client_id.to_owned()),
            ("scope", SCOPES.to_owned()),
        ]),
    )
    .map_err(RequestError::into_error)?;
    decode_device(&body)
}

fn check_active(cancel: &AtomicBool, deadline: Instant) -> std::result::Result<(), RequestError> {
    if cancel.load(Ordering::Relaxed) {
        Err(RequestError::Cancelled)
    } else if Instant::now() >= deadline {
        Err(RequestError::Expired)
    } else {
        Ok(())
    }
}

fn wait_active(
    duration: Duration,
    cancel: &AtomicBool,
    deadline: Instant,
) -> std::result::Result<(), RequestError> {
    let until = Instant::now()
        .checked_add(duration)
        .unwrap_or(deadline)
        .min(deadline);
    loop {
        check_active(cancel, deadline)?;
        let remaining = until.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok(());
        }
        thread::sleep(remaining.min(CANCEL_INTERVAL));
    }
}

fn interruptible_post(
    client: Client,
    form: SecretForm,
    cancel: &AtomicBool,
    deadline: Instant,
) -> std::result::Result<Zeroizing<Vec<u8>>, RequestError> {
    check_active(cancel, deadline)?;
    let (send, receive) = mpsc::sync_channel(1);
    // reqwest blocking cannot interrupt an in-flight request. A bounded worker
    // keeps cancellation responsive; any late response is dropped and zeroized.
    thread::Builder::new()
        .name("hush-oauth-request".to_owned())
        .spawn(move || {
            let _ = send.send(post(&client, TOKEN_ENDPOINT, form));
        })
        .map_err(|_| RequestError::Fatal("Die GitHub-Anmeldung konnte nicht gestartet werden."))?;
    receive_active(receive, cancel, deadline)
}

fn receive_active(
    receive: mpsc::Receiver<std::result::Result<Zeroizing<Vec<u8>>, RequestError>>,
    cancel: &AtomicBool,
    deadline: Instant,
) -> std::result::Result<Zeroizing<Vec<u8>>, RequestError> {
    loop {
        check_active(cancel, deadline)?;
        match receive
            .recv_timeout(CANCEL_INTERVAL.min(deadline.saturating_duration_since(Instant::now())))
        {
            Ok(response) => {
                check_active(cancel, deadline)?;
                return response;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(RequestError::Fatal(
                    "Die GitHub-Anmeldung wurde unterbrochen.",
                ));
            }
        }
    }
}

fn poll_with(
    client_id: &str,
    interval: u64,
    mut request: impl FnMut() -> std::result::Result<Zeroizing<Vec<u8>>, RequestError>,
    mut wait: impl FnMut(Duration) -> std::result::Result<(), RequestError>,
    mut time: impl FnMut() -> i64,
) -> Result<Credentials> {
    let mut interval = interval;
    let mut network_errors = 0;
    loop {
        wait(Duration::from_secs(interval)).map_err(RequestError::into_error)?;
        let body = match request() {
            Ok(body) => {
                network_errors = 0;
                body
            }
            Err(RequestError::Retryable) => {
                network_errors += 1;
                if network_errors >= MAX_NETWORK_ERRORS {
                    return Err(RequestError::Retryable.into_error());
                }
                interval = interval.saturating_mul(2);
                continue;
            }
            Err(error) => return Err(error.into_error()),
        };
        match decode_token(&body, client_id, time())? {
            TokenOutcome::Authorized(credentials) => return Ok(credentials),
            TokenOutcome::Pending => {}
            TokenOutcome::SlowDown(server_interval) => {
                interval = interval.saturating_add(5).max(server_interval.unwrap_or(0));
            }
        }
    }
}

pub fn poll(client_id: &str, device: &DeviceCode, cancel: &AtomicBool) -> Result<Credentials> {
    validate_client_id(client_id)?;
    let deadline = device.received_at + Duration::from_secs(device.expires_in);
    check_active(cancel, deadline).map_err(RequestError::into_error)?;
    let client = http_client()?;
    poll_with(
        client_id,
        device.interval,
        || {
            interruptible_post(
                client.clone(),
                SecretForm(vec![
                    ("client_id", client_id.to_owned()),
                    ("device_code", device.device_code.to_string()),
                    (
                        "grant_type",
                        "urn:ietf:params:oauth:grant-type:device_code".to_owned(),
                    ),
                ]),
                cancel,
                deadline,
            )
        },
        |duration| wait_active(duration, cancel, deadline),
        now,
    )
}

/// Refresh once: callers must persist the rotated pair before making API calls.
/// Do not retry automatically: the previous refresh token is consumed on success.
pub fn refresh(credentials: &Credentials) -> Result<Credentials> {
    validate_client_id(&credentials.client_id)?;
    let at = now();
    let token = credentials
        .refresh_token
        .as_deref()
        .filter(|token| valid_secret(token))
        .ok_or_else(|| {
            anyhow!("Die GitHub-Anmeldung kann nicht erneuert werden. Bitte erneut anmelden.")
        })?;
    if credentials
        .refresh_expires_at
        .is_some_and(|expiry| expiry <= at)
    {
        bail!("Die GitHub-Anmeldung ist abgelaufen. Bitte erneut anmelden.");
    }
    let body = post(
        &http_client()?,
        TOKEN_ENDPOINT,
        SecretForm(vec![
            ("client_id", credentials.client_id.clone()),
            ("refresh_token", token.to_owned()),
            ("grant_type", "refresh_token".to_owned()),
        ]),
    )
    .map_err(RequestError::into_error)?;
    match decode_token(&body, &credentials.client_id, now())? {
        TokenOutcome::Authorized(refreshed) if refreshed.refresh_token.is_some() => Ok(refreshed),
        _ => bail!("GitHub konnte die Anmeldung nicht erneuern. Bitte erneut anmelden."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, collections::VecDeque};

    const DEVICE: &[u8] = br#"{"device_code":"private-device-secret","user_code":"ABCD-1234","verification_uri":"https://github.com/login/device","expires_in":900,"interval":5}"#;
    const TOKEN: &[u8] = br#"{"access_token":"test-access-secret","token_type":"bearer","expires_in":28800,"refresh_token":"test-refresh-secret","refresh_token_expires_in":15897600}"#;

    fn body(bytes: &[u8]) -> std::result::Result<Zeroizing<Vec<u8>>, RequestError> {
        Ok(Zeroizing::new(bytes.to_vec()))
    }

    #[test]
    fn device_uri_must_be_exact_github_verification_page() {
        let device = decode_device(DEVICE).unwrap();
        assert_eq!(device.user_code, "ABCD-1234");
        assert_eq!(device.interval, 5);
        for uri in [
            "https://github.com.evil.example/login/device",
            "https://github.com/login/device?redirect=evil",
            "http://github.com/login/device",
        ] {
            let changed = String::from_utf8(DEVICE.to_vec())
                .unwrap()
                .replace(VERIFICATION_URI, uri);
            assert!(decode_device(changed.as_bytes()).is_err());
        }
    }

    #[test]
    fn tokens_have_absolute_expiry_and_optional_legacy_refresh() {
        let TokenOutcome::Authorized(credentials) = decode_token(TOKEN, "client123", 100).unwrap()
        else {
            panic!("Expected credentials");
        };
        assert_eq!(credentials.expires_at, Some(28_900));
        assert_eq!(credentials.refresh_expires_at, Some(15_897_700));
        assert_eq!(credentials.client_id, "client123");
        let serialized = Zeroizing::new(serde_json::to_string(&credentials).unwrap());
        let restored: Credentials = serde_json::from_str(&serialized).unwrap();
        assert_eq!(
            restored.refresh_token.as_deref(),
            Some("test-refresh-secret")
        );
        let TokenOutcome::Authorized(legacy) = decode_token(
            br#"{"access_token":"test-legacy","token_type":"bearer"}"#,
            "client123",
            100,
        )
        .unwrap() else {
            panic!("Expected credentials");
        };
        assert_eq!(legacy.expires_at, None);
        assert_eq!(legacy.refresh_token, None);
    }

    #[test]
    fn malformed_tokens_and_server_errors_do_not_disclose_secrets() {
        for bytes in [
            br#"{"access_token":"test-access-secret","token_type":"bearer","expires_in":0,"refresh_token":"test-refresh-secret"}"#.as_slice(),
            br#"{"access_token":"test-access-secret","token_type":"bearer","expires_in":10}"#,
            br#"{"access_token":"test-access-secret","token_type":"other"}"#,
            br#"{"error":"test-access-secret","error_description":"test-refresh-secret"}"#,
            br#"invalid test-access-secret"#,
        ] {
            let error = decode_token(bytes, "client123", 100).err().unwrap().to_string();
            assert!(!error.contains("test-access-secret"));
            assert!(!error.contains("test-refresh-secret"));
        }
        assert!(expiry(Some(u64::MAX), 100).is_err());
    }

    #[test]
    fn polling_honors_pending_and_cumulative_slow_down_intervals() {
        let mut responses = VecDeque::from([
            body(br#"{"error":"authorization_pending"}"#),
            body(br#"{"error":"slow_down","interval":10}"#),
            body(br#"{"error":"slow_down","interval":1}"#),
            body(TOKEN),
        ]);
        let waits = RefCell::new(Vec::new());
        let credentials = poll_with(
            "client123",
            5,
            || responses.pop_front().unwrap(),
            |duration| {
                waits.borrow_mut().push(duration.as_secs());
                Ok(())
            },
            || 100,
        )
        .unwrap();
        assert_eq!(*waits.borrow(), vec![5, 5, 10, 15]);
        assert_eq!(credentials.access_token, "test-access-secret");
    }

    #[test]
    fn polling_retries_network_errors_with_backoff_and_stops_after_three() {
        let waits = RefCell::new(Vec::new());
        let error = poll_with(
            "client123",
            5,
            || Err(RequestError::Retryable),
            |duration| {
                waits.borrow_mut().push(duration.as_secs());
                Ok(())
            },
            || 100,
        )
        .err()
        .unwrap();
        assert_eq!(*waits.borrow(), vec![5, 10, 20]);
        assert!(error.to_string().contains("nicht erreichbar"));
    }

    #[test]
    fn denied_expired_and_cancelled_flows_stop_without_extra_requests() {
        for response in [
            br#"{"error":"access_denied"}"#.as_slice(),
            br#"{"error":"expired_token"}"#,
        ] {
            let mut requests = 0;
            assert!(
                poll_with(
                    "client123",
                    5,
                    || {
                        requests += 1;
                        body(response)
                    },
                    |_| Ok(()),
                    || 100
                )
                .is_err()
            );
            assert_eq!(requests, 1);
        }
        for stop in [RequestError::Cancelled, RequestError::Expired] {
            let mut stop = Some(stop);
            assert!(
                poll_with(
                    "client123",
                    5,
                    || panic!("No request after stop"),
                    |_| Err(stop.take().unwrap()),
                    || 100
                )
                .is_err()
            );
        }
    }

    #[test]
    fn cancellation_and_expiry_are_checked_before_any_network_request() {
        let device = decode_device(DEVICE).unwrap();
        let cancelled = AtomicBool::new(true);
        assert!(
            poll("client123", &device, &cancelled)
                .err()
                .unwrap()
                .to_string()
                .contains("abgebrochen")
        );
        let mut expired = device.clone();
        expired.received_at = Instant::now() - Duration::from_secs(901);
        assert!(
            poll("client123", &expired, &AtomicBool::new(false))
                .err()
                .unwrap()
                .to_string()
                .contains("abgelaufen")
        );
    }

    #[test]
    fn cancellation_interrupts_waiting_for_a_pending_response() {
        let cancel = AtomicBool::new(false);
        let (send, receive) = mpsc::sync_channel(1);
        thread::scope(|scope| {
            scope.spawn(|| {
                thread::sleep(Duration::from_millis(20));
                cancel.store(true, Ordering::Relaxed);
            });
            let started = Instant::now();
            let result = receive_active(receive, &cancel, started + Duration::from_secs(5));
            assert!(matches!(result, Err(RequestError::Cancelled)));
            assert!(started.elapsed() < Duration::from_millis(750));
        });
        // The worker's late response cannot be accepted after cancellation.
        assert!(send.send(body(TOKEN)).is_err());
    }

    #[test]
    fn cancelled_flow_does_not_accept_a_response_that_is_already_queued() {
        let (send, receive) = mpsc::sync_channel(1);
        send.send(body(TOKEN))
            .unwrap_or_else(|_| panic!("Receiver exists"));
        assert!(matches!(
            receive_active(
                receive,
                &AtomicBool::new(true),
                Instant::now() + Duration::from_secs(5)
            ),
            Err(RequestError::Cancelled)
        ));
    }
}
