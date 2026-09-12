//! The HuggingFace OAuth 2.0 PKCE flow: build the authorize URL, open the
//! system browser to it, catch the redirect on a local `TcpListener`, trade
//! the authorization code for an access token, and resolve the signed-in
//! username via HuggingFace's `whoami-v2`.
//!
//! whspr is a native **public** client, so there is no client secret: the
//! flow uses PKCE (`code_challenge`/`code_verifier`) for the proof instead.
//! The one thing the user must supply is an OAuth **client id**, created once
//! by registering a Connected App on HuggingFace -- see the [crate-level
//! docs](crate) for the exact registration steps. The client id reaches this
//! module from `whspr_config::HuggingFaceSettings::oauth_client_id` (the GUI
//! passes it in as [`OauthConfig::client_id`]).
//!
//! # Testability
//!
//! The pure pieces -- the redirect URI, the authorize-URL structure, and the
//! callback-query/percent-decode parsers -- are unit-tested. The parts that
//! touch the network, a real browser, or a bound socket ([`run_login`],
//! [`whoami`], [`accept_callback`]) are deliberately left untested here; they
//! only ever run against the live HuggingFace endpoints.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

use oauth2::basic::BasicClient;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, CsrfToken, PkceCodeChallenge, PkceCodeVerifier,
    RedirectUrl, Scope, TokenResponse, TokenUrl,
};
use whspr_core::{Result, WhsprError};

/// HuggingFace's OAuth authorization endpoint.
pub const HF_AUTHORIZE_URL: &str = "https://huggingface.co/oauth/authorize";
/// HuggingFace's OAuth token endpoint.
pub const HF_TOKEN_URL: &str = "https://huggingface.co/oauth/token";
/// HuggingFace's authenticated-user endpoint; returns the username in `name`.
pub const HF_WHOAMI_URL: &str = "https://huggingface.co/api/whoami-v2";
/// Default localhost port the redirect listener binds. The registered OAuth
/// app's redirect URI must match [`redirect_uri`] for this port.
pub const DEFAULT_REDIRECT_PORT: u16 = 8788;
/// Scopes whspr requests: OIDC identity plus read access to repos (so gated
/// model downloads work with the same token).
pub const DEFAULT_SCOPES: &[&str] = &["openid", "profile", "read-repos"];
/// How long [`run_login`] waits for the browser redirect before giving up.
pub const DEFAULT_LOGIN_TIMEOUT: Duration = Duration::from_secs(300);
/// whspr's built-in HuggingFace OAuth **client id**, so "Sign in with
/// HuggingFace" works out of the box with no per-user setup. It is a *public*
/// identifier, not a secret: whspr is a native public client, so the flow uses
/// PKCE (see the module docs) and there is deliberately no client secret here.
/// Registered as a Connected App whose redirect URI is [`redirect_uri`] on
/// [`DEFAULT_REDIRECT_PORT`] with [`DEFAULT_SCOPES`].
/// `whspr_config::HuggingFaceSettings::oauth_client_id` overrides it for anyone
/// running their own OAuth app.
pub const BUILTIN_CLIENT_ID: &str = "a89b3ec5-447f-4205-b738-785b78b09276";

/// The `http://localhost:<port>/callback` redirect URI for `port`. This exact
/// string must be registered on the HuggingFace OAuth app.
pub fn redirect_uri(port: u16) -> String {
    format!("http://localhost:{port}/callback")
}

/// What the GUI needs to start an OAuth login: the registered client id and
/// which localhost port to catch the redirect on.
#[derive(Debug, Clone)]
pub struct OauthConfig {
    /// The OAuth app's client id (from config/env; see crate docs).
    pub client_id: String,
    /// Localhost port for the redirect listener. Defaults to
    /// [`DEFAULT_REDIRECT_PORT`].
    pub redirect_port: u16,
}

impl OauthConfig {
    /// A config for `client_id` using the default redirect port.
    pub fn new(client_id: impl Into<String>) -> Self {
        Self {
            client_id: client_id.into(),
            redirect_port: DEFAULT_REDIRECT_PORT,
        }
    }

    /// This config's redirect URI (see [`redirect_uri`]).
    pub fn redirect_uri(&self) -> String {
        redirect_uri(self.redirect_port)
    }
}

/// The identity resolved by a completed login: the HuggingFace username and
/// the access token to save (and use as a download bearer token).
#[derive(Debug, Clone)]
pub struct HfIdentity {
    /// The signed-in HuggingFace username (`name` from `whoami-v2`).
    pub username: String,
    /// The OAuth access token. Store it (see
    /// `whspr_config::HuggingFaceSettings::token`) and pass it to
    /// [`crate::download`] for gated repos.
    pub token: String,
}

/// An in-progress login: the authorize URL to send the user to, the CSRF
/// `state` to check on return, and the PKCE verifier to redeem the code with.
pub struct AuthSession {
    /// The full authorize URL the user's browser should open.
    pub authorize_url: String,
    /// The random CSRF `state`; must equal the `state` echoed on the redirect.
    pub csrf_state: String,
    /// The redirect port the callback listener should bind.
    pub redirect_port: u16,
    client_id: String,
    redirect_uri: String,
    pkce_verifier: PkceCodeVerifier,
}

/// The fully-configured HuggingFace OAuth client type. Spelled out as an
/// alias (rather than inline in [`hf_client`]'s signature) because oauth2 5.x
/// encodes which endpoints are set in the type itself: `EndpointSet` for the
/// auth URL (6th param) and token URL (10th param), `EndpointNotSet` for the
/// device-auth/introspection/revocation endpoints we don't use.
type HfOauthClient = oauth2::Client<
    oauth2::basic::BasicErrorResponse,
    oauth2::basic::BasicTokenResponse,
    oauth2::basic::BasicTokenIntrospectionResponse,
    oauth2::StandardRevocableToken,
    oauth2::basic::BasicRevocationErrorResponse,
    oauth2::EndpointSet,
    oauth2::EndpointNotSet,
    oauth2::EndpointNotSet,
    oauth2::EndpointNotSet,
    oauth2::EndpointSet,
>;

/// Builds the `oauth2` client with HuggingFace's endpoints and `redirect_uri`
/// wired in. Factored out so [`begin`] and [`AuthSession::exchange`] agree on
/// the exact same client configuration.
fn hf_client(client_id: &str, redirect_uri: &str) -> Result<HfOauthClient> {
    let auth_url = AuthUrl::new(HF_AUTHORIZE_URL.to_string())
        .map_err(|e| WhsprError::Other(format!("invalid HF authorize URL: {e}")))?;
    let token_url = TokenUrl::new(HF_TOKEN_URL.to_string())
        .map_err(|e| WhsprError::Other(format!("invalid HF token URL: {e}")))?;
    let redirect = RedirectUrl::new(redirect_uri.to_string())
        .map_err(|e| WhsprError::Other(format!("invalid redirect URI: {e}")))?;
    Ok(BasicClient::new(ClientId::new(client_id.to_string()))
        .set_auth_uri(auth_url)
        .set_token_uri(token_url)
        .set_redirect_uri(redirect))
}

/// Starts a login: generates a PKCE challenge and CSRF state and builds the
/// authorize URL. Pure aside from the RNG -- no I/O -- so its output is
/// unit-testable (the URL's structure) without touching the network.
pub fn begin(config: &OauthConfig) -> Result<AuthSession> {
    let redirect = config.redirect_uri();
    let client = hf_client(&config.client_id, &redirect)?;

    let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
    let mut request = client.authorize_url(CsrfToken::new_random);
    for scope in DEFAULT_SCOPES {
        request = request.add_scope(Scope::new((*scope).to_string()));
    }
    let (url, csrf) = request.set_pkce_challenge(challenge).url();

    Ok(AuthSession {
        authorize_url: url.to_string(),
        csrf_state: csrf.secret().clone(),
        redirect_port: config.redirect_port,
        client_id: config.client_id.clone(),
        redirect_uri: redirect,
        pkce_verifier: verifier,
    })
}

impl AuthSession {
    /// Trades an authorization `code` for an access token, proving possession
    /// of the PKCE verifier. Uses oauth2's re-exported reqwest client with
    /// redirects disabled (SSRF hardening, per oauth2's guidance).
    async fn exchange(self, code: String) -> Result<String> {
        let client = hf_client(&self.client_id, &self.redirect_uri)?;
        let http = oauth2::reqwest::ClientBuilder::new()
            .redirect(oauth2::reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| WhsprError::Other(format!("failed to build HTTP client: {e}")))?;

        let token = client
            .exchange_code(AuthorizationCode::new(code))
            .set_pkce_verifier(self.pkce_verifier)
            .request_async(&http)
            .await
            .map_err(|e| WhsprError::Other(format!("token exchange failed: {e}")))?;

        Ok(token.access_token().secret().clone())
    }
}

/// The `code` + `state` parsed off an OAuth redirect's query string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallbackParams {
    /// The authorization code to exchange for a token.
    pub code: String,
    /// The CSRF `state` echoed back; compare against [`AuthSession::csrf_state`].
    pub state: String,
}

/// Parses `code` and `state` out of a request target like
/// `/callback?code=abc&state=xyz`. Returns `None` unless both are present.
/// Pure and unit-tested.
pub fn parse_callback_query(target: &str) -> Option<CallbackParams> {
    let (_, query) = target.split_once('?')?;
    let mut code = None;
    let mut state = None;
    for pair in query.split('&') {
        if let Some((key, value)) = pair.split_once('=') {
            match key {
                "code" => code = Some(percent_decode(value)),
                "state" => state = Some(percent_decode(value)),
                _ => {}
            }
        }
    }
    Some(CallbackParams {
        code: code?,
        state: state?,
    })
}

/// Minimal `application/x-www-form-urlencoded` value decoder: turns `+` into a
/// space and `%XX` escapes into their byte, leaving anything malformed as-is.
/// Enough for OAuth `code`/`state` values; unit-tested.
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hi = (bytes[i + 1] as char).to_digit(16);
                let lo = (bytes[i + 2] as char).to_digit(16);
                match (hi, lo) {
                    (Some(hi), Some(lo)) => {
                        out.push((hi * 16 + lo) as u8);
                        i += 3;
                    }
                    _ => {
                        out.push(b'%');
                        i += 1;
                    }
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Blocks on `listener` until an OAuth redirect carrying `code` + `state`
/// arrives, then replies with a small "you can close this tab" page. Bounded
/// by `timeout` so it can never hang forever; stray requests (e.g. a browser
/// `favicon.ico` probe) are answered and ignored rather than aborting the
/// wait. Runs on a blocking thread (see [`run_login`]).
fn accept_callback(listener: TcpListener, timeout: Duration) -> Result<CallbackParams> {
    listener
        .set_nonblocking(true)
        .map_err(|e| WhsprError::Other(format!("callback listener setup failed: {e}")))?;
    let deadline = Instant::now() + timeout;

    loop {
        if Instant::now() >= deadline {
            return Err(WhsprError::Other(
                "timed out waiting for the HuggingFace login redirect".to_string(),
            ));
        }
        match listener.accept() {
            Ok((mut stream, _addr)) => {
                let mut buf = [0u8; 4096];
                stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
                let n = stream.read(&mut buf).unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]);
                let target = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("");
                let params = parse_callback_query(target);

                let body = if params.is_some() {
                    "<html><body style=\"font-family:sans-serif\"><h2>whspr: sign-in complete</h2>\
                     <p>You can close this tab and return to whspr.</p></body></html>"
                } else {
                    "<html><body style=\"font-family:sans-serif\"><h2>whspr</h2>\
                     <p>Waiting for the HuggingFace redirect...</p></body></html>"
                };
                let status = if params.is_some() {
                    "200 OK"
                } else {
                    "404 Not Found"
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();

                if let Some(params) = params {
                    return Ok(params);
                }
                // Not the redirect (favicon, etc.) -- keep waiting.
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => {
                return Err(WhsprError::Other(format!("callback listener error: {e}")));
            }
        }
    }
}

/// Fetches the signed-in username from HuggingFace's `whoami-v2` with the
/// bearer `token`. Untested (live network only).
pub async fn whoami(token: &str) -> Result<String> {
    let http = oauth2::reqwest::Client::new();
    let body = http
        .get(HF_WHOAMI_URL)
        .bearer_auth(token)
        .header("User-Agent", "whspr")
        .send()
        .await
        .map_err(|e| WhsprError::Other(format!("whoami request failed: {e}")))?
        .error_for_status()
        .map_err(|e| WhsprError::Other(format!("whoami returned an error status: {e}")))?
        .text()
        .await
        .map_err(|e| WhsprError::Other(format!("could not read whoami response: {e}")))?;

    let value: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| WhsprError::Other(format!("could not parse whoami response: {e}")))?;
    value
        .get("name")
        .and_then(|n| n.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| WhsprError::Other("whoami response missing a 'name' field".to_string()))
}

/// Runs the whole login end-to-end: build the authorize URL, bind the local
/// redirect listener, open the browser, wait (bounded by `timeout`) for the
/// redirect, verify the CSRF state, exchange the code for a token, and
/// resolve the username. The browser + socket + network steps make this the
/// untested orchestration seam over the tested pure helpers above.
///
/// If the browser can't be opened automatically, the login still proceeds --
/// the user can open [`AuthSession::authorize_url`] by hand (the GUI shows it).
pub async fn run_login(config: OauthConfig, timeout: Duration) -> Result<HfIdentity> {
    let session = begin(&config)?;
    let expected_state = session.csrf_state.clone();
    let authorize_url = session.authorize_url.clone();

    // Bind before opening the browser so there's no window where the redirect
    // could arrive with nothing listening.
    let listener = TcpListener::bind(("127.0.0.1", session.redirect_port)).map_err(|e| {
        WhsprError::Other(format!(
            "could not bind the local OAuth callback listener on port {}: {e}",
            session.redirect_port
        ))
    })?;

    let _ = webbrowser::open(&authorize_url);

    let params = tokio::task::spawn_blocking(move || accept_callback(listener, timeout))
        .await
        .map_err(|e| WhsprError::Other(format!("callback listener task panicked: {e}")))??;

    if params.state != expected_state {
        return Err(WhsprError::Other(
            "OAuth state mismatch (possible CSRF); aborting login".to_string(),
        ));
    }

    let token = session.exchange(params.code).await?;
    let username = whoami(&token).await?;
    Ok(HfIdentity { username, token })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redirect_uri_uses_the_given_port() {
        assert_eq!(redirect_uri(8788), "http://localhost:8788/callback");
        assert_eq!(
            OauthConfig::new("cid").redirect_uri(),
            "http://localhost:8788/callback"
        );
    }

    #[test]
    fn begin_builds_an_authorize_url_with_the_pkce_and_client_params() {
        let session = begin(&OauthConfig::new("my-client-id")).unwrap();
        let url = &session.authorize_url;

        assert!(url.starts_with(HF_AUTHORIZE_URL), "wrong endpoint: {url}");
        assert!(
            url.contains("response_type=code"),
            "missing response_type: {url}"
        );
        assert!(
            url.contains("client_id=my-client-id"),
            "missing client_id: {url}"
        );
        assert!(
            url.contains("code_challenge_method=S256"),
            "missing/incorrect PKCE method: {url}"
        );
        assert!(
            url.contains("code_challenge="),
            "missing PKCE challenge: {url}"
        );
        // The redirect URI is percent-encoded in the query.
        assert!(
            url.contains("redirect_uri=http%3A%2F%2Flocalhost%3A8788%2Fcallback"),
            "missing/incorrect redirect_uri: {url}"
        );
        // Every requested scope shows up (space-joined -> %20 or +).
        assert!(
            url.contains("read-repos"),
            "missing read-repos scope: {url}"
        );
        // The CSRF state in the URL matches the one we kept to verify later.
        assert!(
            url.contains(&format!("state={}", session.csrf_state)),
            "state in URL should match the stored csrf_state: {url}"
        );
    }

    #[test]
    fn begin_generates_a_fresh_state_each_time() {
        let a = begin(&OauthConfig::new("cid")).unwrap();
        let b = begin(&OauthConfig::new("cid")).unwrap();
        assert_ne!(
            a.csrf_state, b.csrf_state,
            "CSRF state must be random per login"
        );
    }

    #[test]
    fn parse_callback_query_extracts_code_and_state() {
        let params = parse_callback_query("/callback?code=abc123&state=xyz789").unwrap();
        assert_eq!(
            params,
            CallbackParams {
                code: "abc123".to_string(),
                state: "xyz789".to_string(),
            }
        );
    }

    #[test]
    fn parse_callback_query_ignores_extra_params_and_order() {
        let params =
            parse_callback_query("/callback?state=s1&foo=bar&code=c1&iss=huggingface").unwrap();
        assert_eq!(params.code, "c1");
        assert_eq!(params.state, "s1");
    }

    #[test]
    fn parse_callback_query_requires_both_code_and_state() {
        assert!(parse_callback_query("/callback?code=only").is_none());
        assert!(parse_callback_query("/callback?state=only").is_none());
        assert!(parse_callback_query("/callback").is_none());
        assert!(parse_callback_query("/favicon.ico").is_none());
    }

    #[test]
    fn percent_decode_handles_escapes_and_plus() {
        assert_eq!(percent_decode("a%2Bb"), "a+b");
        assert_eq!(percent_decode("hello+world"), "hello world");
        assert_eq!(percent_decode("plain"), "plain");
        // A malformed escape is left untouched rather than dropped.
        assert_eq!(percent_decode("50%"), "50%");
    }

    #[test]
    fn parse_callback_query_percent_decodes_values() {
        let params = parse_callback_query("/callback?code=a%2Bb&state=x%20y").unwrap();
        assert_eq!(params.code, "a+b");
        assert_eq!(params.state, "x y");
    }
}
