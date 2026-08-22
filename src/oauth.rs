use keyring::Entry;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::env;
use std::fmt::{Display, Formatter};
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener};
use std::thread;
use std::time::{Duration, Instant};
use url::Url;
use uuid::Uuid;

const AUTHORIZE_URL: &str = "https://auth.atlassian.com/authorize";
const TOKEN_URL: &str = "https://auth.atlassian.com/oauth/token";
const RESOURCES_URL: &str = "https://api.atlassian.com/oauth/token/accessible-resources";
const SCOPES: &str = "read:jira-work write:jira-work offline_access";
const KEYRING_SERVICE: &str = "com.tebasaki314.jira-lens";
const KEYRING_ACCOUNT: &str = "atlassian-oauth";
const CALLBACK_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Clone, Debug)]
pub struct OAuthConfig {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
}

impl OAuthConfig {
    pub fn from_env() -> Result<Self, OAuthError> {
        let client_id = required_env("JIRA_OAUTH_CLIENT_ID")?;
        let client_secret = required_env("JIRA_OAUTH_CLIENT_SECRET")?;
        let redirect_uri = env::var("JIRA_OAUTH_REDIRECT_URI")
            .unwrap_or_else(|_| "http://127.0.0.1:53682/callback".to_owned());
        validate_loopback_redirect(&redirect_uri)?;
        Ok(Self {
            client_id,
            client_secret,
            redirect_uri,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TokenSet {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: u64,
    pub scope: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct JiraResource {
    pub id: String,
    pub name: String,
    pub url: String,
    pub scopes: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SavedSession {
    pub tokens: TokenSet,
    pub resources: Vec<JiraResource>,
}

#[derive(Debug)]
pub struct OAuthError(String);

impl Display for OAuthError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for OAuthError {}

pub fn connect() -> Result<SavedSession, OAuthError> {
    let config = OAuthConfig::from_env()?;
    if let Some(saved) = load_session()
        && let Some(refresh_token) = saved.tokens.refresh_token.as_deref()
        && let Ok(tokens) = refresh(&config, refresh_token)
    {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(http_error)?;
        let resources = fetch_resources(&client, &tokens.access_token)?;
        if !resources.is_empty() {
            let session = SavedSession { tokens, resources };
            save_session(&session)?;
            return Ok(session);
        }
    }

    let redirect = Url::parse(&config.redirect_uri)
        .map_err(|error| OAuthError(format!("コールバックURLが不正です: {error}")))?;
    let listener = bind_callback_listener(&redirect)?;
    let state = Uuid::new_v4().to_string();
    let authorization_url = build_authorization_url(&config, &state)?;

    webbrowser::open(authorization_url.as_str())
        .map_err(|error| OAuthError(format!("ブラウザを開けませんでした: {error}")))?;

    let code = wait_for_callback(&listener, &redirect, &state)?;
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(http_error)?;
    let tokens = exchange_code(&client, &config, &code)?;
    let resources = fetch_resources(&client, &tokens.access_token)?;
    if resources.is_empty() {
        return Err(OAuthError(
            "利用可能なJiraサイトがありません。認可したサイトとスコープを確認してください。".into(),
        ));
    }

    let session = SavedSession { tokens, resources };
    save_session(&session)?;
    Ok(session)
}

pub fn refresh(config: &OAuthConfig, refresh_token: &str) -> Result<TokenSet, OAuthError> {
    #[derive(Serialize)]
    struct RefreshRequest<'a> {
        grant_type: &'static str,
        client_id: &'a str,
        client_secret: &'a str,
        refresh_token: &'a str,
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(http_error)?;
    post_token(
        &client,
        &RefreshRequest {
            grant_type: "refresh_token",
            client_id: &config.client_id,
            client_secret: &config.client_secret,
            refresh_token,
        },
    )
}

fn required_env(name: &str) -> Result<String, OAuthError> {
    env::var(name).map_err(|_| {
        OAuthError(format!(
            "{name} が未設定です。READMEのOAuth 2.0設定手順を確認してください。"
        ))
    })
}

fn validate_loopback_redirect(value: &str) -> Result<(), OAuthError> {
    let url = Url::parse(value)
        .map_err(|error| OAuthError(format!("JIRA_OAUTH_REDIRECT_URIが不正です: {error}")))?;
    let is_loopback = url
        .host_str()
        .and_then(|host| host.parse::<IpAddr>().ok())
        .is_some_and(|ip| ip.is_loopback())
        || url.host_str() == Some("localhost");
    if url.scheme() != "http" || !is_loopback || url.port().is_none() {
        return Err(OAuthError(
            "デスクトップ版のコールバックURLは http://127.0.0.1:ポート/パス または http://localhost:ポート/パス にしてください。".into(),
        ));
    }
    Ok(())
}

fn bind_callback_listener(redirect: &Url) -> Result<TcpListener, OAuthError> {
    let host = redirect.host_str().unwrap_or("127.0.0.1");
    let port = redirect
        .port()
        .ok_or_else(|| OAuthError("コールバックURLにポートがありません。".into()))?;
    let address = format!("{host}:{port}")
        .parse::<SocketAddr>()
        .or_else(|_| format!("127.0.0.1:{port}").parse::<SocketAddr>())
        .map_err(|error| OAuthError(format!("コールバック待受先が不正です: {error}")))?;
    let listener = TcpListener::bind(address)
        .map_err(|error| OAuthError(format!("コールバックを待受できません: {error}")))?;
    listener
        .set_nonblocking(true)
        .map_err(|error| OAuthError(format!("コールバック待受の設定に失敗しました: {error}")))?;
    Ok(listener)
}

fn build_authorization_url(config: &OAuthConfig, state: &str) -> Result<Url, OAuthError> {
    let mut url = Url::parse(AUTHORIZE_URL).map_err(|error| OAuthError(error.to_string()))?;
    url.query_pairs_mut()
        .append_pair("audience", "api.atlassian.com")
        .append_pair("client_id", &config.client_id)
        .append_pair("scope", SCOPES)
        .append_pair("redirect_uri", &config.redirect_uri)
        .append_pair("state", state)
        .append_pair("response_type", "code")
        .append_pair("prompt", "consent");
    Ok(url)
}

fn wait_for_callback(
    listener: &TcpListener,
    redirect: &Url,
    expected_state: &str,
) -> Result<String, OAuthError> {
    let started = Instant::now();
    while started.elapsed() < CALLBACK_TIMEOUT {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let mut buffer = [0_u8; 8192];
                let size = stream.read(&mut buffer).map_err(|error| {
                    OAuthError(format!("コールバックの読取に失敗しました: {error}"))
                })?;
                let request = String::from_utf8_lossy(&buffer[..size]);
                let target = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .ok_or_else(|| OAuthError("コールバック要求を解釈できません。".into()))?;
                let callback = Url::parse(&format!("http://localhost{target}"))
                    .map_err(|error| OAuthError(format!("コールバックURLが不正です: {error}")))?;

                if callback.path() != redirect.path() {
                    respond(&mut stream, 404, "Jira Lens: callback path not found");
                    continue;
                }
                let params: std::collections::HashMap<_, _> = callback.query_pairs().collect();
                if params.get("state").map(|value| value.as_ref()) != Some(expected_state) {
                    respond(&mut stream, 400, "Jira Lens: invalid OAuth state");
                    return Err(OAuthError(
                        "OAuth stateが一致しません。接続を中止しました。".into(),
                    ));
                }
                if let Some(error) = params.get("error") {
                    respond(&mut stream, 400, "Jira Lens: authorization canceled");
                    return Err(OAuthError(format!("Atlassian認可エラー: {error}")));
                }
                let code = params
                    .get("code")
                    .map(ToString::to_string)
                    .ok_or_else(|| OAuthError("認可コードがありません。".into()))?;
                respond(
                    &mut stream,
                    200,
                    "Jira Lens connected. You can close this browser tab.",
                );
                return Ok(code);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(100));
            }
            Err(error) => {
                return Err(OAuthError(format!(
                    "コールバックの受付に失敗しました: {error}"
                )));
            }
        }
    }
    Err(OAuthError(
        "OAuth認可が3分以内に完了しませんでした。".into(),
    ))
}

fn respond(stream: &mut impl Write, status: u16, message: &str) {
    let status_text = if status == 200 { "OK" } else { "Error" };
    let body =
        format!("<!doctype html><meta charset=utf-8><title>Jira Lens</title><h1>{message}</h1>");
    let response = format!(
        "HTTP/1.1 {status} {status_text}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
}

fn exchange_code(
    client: &Client,
    config: &OAuthConfig,
    code: &str,
) -> Result<TokenSet, OAuthError> {
    #[derive(Serialize)]
    struct TokenRequest<'a> {
        grant_type: &'static str,
        client_id: &'a str,
        client_secret: &'a str,
        code: &'a str,
        redirect_uri: &'a str,
    }

    post_token(
        client,
        &TokenRequest {
            grant_type: "authorization_code",
            client_id: &config.client_id,
            client_secret: &config.client_secret,
            code,
            redirect_uri: &config.redirect_uri,
        },
    )
}

fn post_token<T: Serialize>(client: &Client, body: &T) -> Result<TokenSet, OAuthError> {
    let response = client
        .post(TOKEN_URL)
        .json(body)
        .send()
        .map_err(http_error)?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        return Err(OAuthError(format!(
            "トークン交換に失敗しました ({status}): {body}"
        )));
    }
    response.json::<TokenSet>().map_err(http_error)
}

fn fetch_resources(client: &Client, access_token: &str) -> Result<Vec<JiraResource>, OAuthError> {
    let response = client
        .get(RESOURCES_URL)
        .bearer_auth(access_token)
        .header("Accept", "application/json")
        .send()
        .map_err(http_error)?;
    let status = response.status();
    if !status.is_success() {
        return Err(OAuthError(format!(
            "Jiraサイト一覧の取得に失敗しました ({status})"
        )));
    }
    response.json::<Vec<JiraResource>>().map_err(http_error)
}

fn save_session(session: &SavedSession) -> Result<(), OAuthError> {
    let serialized = serde_json::to_string(session)
        .map_err(|error| OAuthError(format!("OAuth情報を保存できません: {error}")))?;
    Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .and_then(|entry| entry.set_password(&serialized))
        .map_err(|error| OAuthError(format!("OS資格情報ストアへ保存できません: {error}")))
}

fn load_session() -> Option<SavedSession> {
    let serialized = Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .ok()?
        .get_password()
        .ok()?;
    serde_json::from_str(&serialized).ok()
}

fn http_error(error: reqwest::Error) -> OAuthError {
    OAuthError(format!("Atlassianとの通信に失敗しました: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> OAuthConfig {
        OAuthConfig {
            client_id: "client-id".into(),
            client_secret: "never-in-url".into(),
            redirect_uri: "http://127.0.0.1:53682/callback".into(),
        }
    }

    #[test]
    fn authorization_url_contains_required_parameters_without_secret() {
        let url = build_authorization_url(&config(), "csrf-state").unwrap();
        let params: std::collections::HashMap<_, _> = url.query_pairs().collect();
        assert_eq!(params.get("audience").unwrap(), "api.atlassian.com");
        assert_eq!(params.get("state").unwrap(), "csrf-state");
        assert!(params.get("scope").unwrap().contains("offline_access"));
        assert!(!url.as_str().contains("never-in-url"));
    }

    #[test]
    fn only_loopback_redirects_are_accepted() {
        assert!(validate_loopback_redirect("http://localhost:53682/callback").is_ok());
        assert!(validate_loopback_redirect("http://127.0.0.1:53682/callback").is_ok());
        assert!(validate_loopback_redirect("https://example.com/callback").is_err());
    }
}
