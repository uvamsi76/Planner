//! Google Drive storage. No GTK widgets here: everything is blocking and meant
//! to run on a worker thread (see `sync.rs`).
//!
//! - Sign-in: OAuth 2.0 for installed apps, using a loopback redirect
//!   (http://127.0.0.1:<random port>) and PKCE. The OAuth client (id/secret)
//!   is built in from `google-client.json` at compile time (see build.rs), or
//!   entered once in the sign-in window if the build has none.
//! - Scope `drive.file`: Planner only sees files its own Google Cloud project
//!   created. The Android app uses the same project, so both see one file.
//! - Data: `My Drive/<folder>/planner-data.json` (folder defaults to "Planner").
//! - The account (refresh token, file id, sync bookkeeping) lives in
//!   `~/.config/planner/google-drive.json`, readable only by you.

use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Read, Write},
    net::TcpListener,
    path::PathBuf,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const REVOKE_URL: &str = "https://oauth2.googleapis.com/revoke";
const SCOPE: &str = "https://www.googleapis.com/auth/drive.file";
const API: &str = "https://www.googleapis.com/drive/v3";
const UPLOAD_API: &str = "https://www.googleapis.com/upload/drive/v3";
pub const DEFAULT_FOLDER: &str = "Planner";
pub const FILE_NAME: &str = "planner-data.json";
const FOLDER_MIME: &str = "application/vnd.google-apps.folder";

pub type Result<T> = std::result::Result<T, String>;

/// OAuth client compiled into this build (from google-client.json), if any.
fn builtin_client() -> Option<(&'static str, &'static str)> {
    match (option_env!("PLANNER_GOOGLE_CLIENT_ID"), option_env!("PLANNER_GOOGLE_CLIENT_SECRET")) {
        (Some(id), Some(secret)) if !id.is_empty() && !secret.is_empty() => Some((id, secret)),
        _ => None,
    }
}

pub fn has_builtin_client() -> bool {
    builtin_client().is_some()
}

/// Google account, folder and sync bookkeeping, persisted between runs.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Account {
    /// Only used when the build has no built-in client.
    #[serde(default)]
    pub client_id: String,
    #[serde(default)]
    pub client_secret: String,
    pub refresh_token: Option<String>,
    pub email: Option<String>,
    #[serde(default = "default_folder")]
    pub folder: String,
    pub file_id: Option<String>,
    /// Drive's `version` of the file as of our last upload or download.
    pub version: Option<String>,
    /// Local changes not uploaded yet.
    #[serde(default)]
    pub dirty: bool,
    /// When the last successful sync happened (RFC 3339, local time).
    pub last_sync: Option<String>,
}

fn default_folder() -> String {
    DEFAULT_FOLDER.into()
}

impl Default for Account {
    fn default() -> Self {
        Account {
            client_id: String::new(),
            client_secret: String::new(),
            refresh_token: None,
            email: None,
            folder: default_folder(),
            file_id: None,
            version: None,
            dirty: false,
            last_sync: None,
        }
    }
}

impl Account {
    pub fn path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("planner")
            .join("google-drive.json")
    }

    pub fn load() -> Account {
        fs::read_to_string(Self::path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Written with mode 0600: it holds the refresh token.
    pub fn save(&self) -> io::Result<()> {
        use std::os::unix::fs::OpenOptionsExt;
        let path = Self::path();
        fs::create_dir_all(path.parent().unwrap())?;
        let tmp = path.with_extension("json.tmp");
        let mut f = fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&tmp)?;
        f.write_all(serde_json::to_string_pretty(self).expect("account serializes").as_bytes())?;
        fs::rename(tmp, path)
    }

    pub fn signed_in(&self) -> bool {
        self.refresh_token.is_some()
    }

    /// The OAuth client to use: built-in, else the one entered by the user.
    fn client(&self) -> Result<(String, String)> {
        if let Some((id, secret)) = builtin_client() {
            return Ok((id.into(), secret.into()));
        }
        if self.client_id.trim().is_empty() || self.client_secret.trim().is_empty() {
            return Err("No Google OAuth client configured".into());
        }
        Ok((self.client_id.trim().into(), self.client_secret.trim().into()))
    }

    /// Sign out, keeping the folder choice and any entered client.
    pub fn sign_out(&mut self) {
        *self = Account {
            client_id: self.client_id.clone(),
            client_secret: self.client_secret.clone(),
            folder: self.folder.clone(),
            ..Default::default()
        };
    }

    /// Point at another folder: the file there (if any) is found on next sync.
    pub fn set_folder(&mut self, folder: &str) {
        self.folder = folder.trim().to_string();
        self.file_id = None;
        self.version = None;
    }
}

// ---------------------------------------------------------------- HTTP

fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .http_status_as_error(false)
            .build()
            .into()
    })
}

/// Turn a response into its body text, or a readable error.
fn body(resp: std::result::Result<ureq::http::Response<ureq::Body>, ureq::Error>) -> Result<String> {
    let mut resp = resp.map_err(|e| format!("Network error: {e}"))?;
    let status = resp.status();
    let text = resp.body_mut().read_to_string().unwrap_or_default();
    if status.is_success() {
        return Ok(text);
    }
    // Google errors: {"error": "invalid_grant", ...} or {"error": {"message": ...}}
    let detail = serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|v| v["error"]["message"].as_str().or(v["error"].as_str()).map(String::from))
        .unwrap_or(text);
    Err(format!("Google returned {}: {detail}", status.as_u16()))
}

fn json<T: for<'de> Deserialize<'de>>(text: &str) -> Result<T> {
    serde_json::from_str(text).map_err(|e| format!("Unexpected reply from Google: {e}"))
}

/// The sign-in was revoked or expired: the user has to sign in again.
pub fn is_auth_error(err: &str) -> bool {
    err.contains("invalid_grant") || err.contains("returned 401")
}

// ---------------------------------------------------------------- OAuth

#[derive(Deserialize)]
struct TokenReply {
    access_token: String,
    expires_in: u64,
    refresh_token: Option<String>,
}

/// Access tokens last an hour; keep the current one in memory only.
fn token_cache() -> &'static Mutex<Option<(String, String, Instant)>> {
    static CACHE: OnceLock<Mutex<Option<(String, String, Instant)>>> = OnceLock::new();
    CACHE.get_or_init(Mutex::default)
}

fn access_token(acct: &Account) -> Result<String> {
    let refresh = acct.refresh_token.as_deref().ok_or("Not signed in")?;
    if let Some((key, token, until)) = &*token_cache().lock().unwrap()
        && key == refresh
        && Instant::now() < *until
    {
        return Ok(token.clone());
    }
    let (id, secret) = acct.client()?;
    let reply: TokenReply = json(&body(agent().post(TOKEN_URL).send_form([
        ("client_id", id.as_str()),
        ("client_secret", secret.as_str()),
        ("refresh_token", refresh),
        ("grant_type", "refresh_token"),
    ]))?)?;
    let until = Instant::now() + Duration::from_secs(reply.expires_in.saturating_sub(60));
    *token_cache().lock().unwrap() = Some((refresh.to_string(), reply.access_token.clone(), until));
    Ok(reply.access_token)
}

/// A started sign-in: open `url` in the browser, then call `finish`.
pub struct PendingAuth {
    pub url: String,
    listener: TcpListener,
    redirect: String,
    verifier: String,
    state: String,
}

pub fn begin_auth(acct: &Account) -> Result<PendingAuth> {
    let (id, _) = acct.client()?;
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| format!("Couldn't open a local port: {e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let redirect = format!("http://127.0.0.1:{port}");
    let verifier = random_token(48);
    let state = random_token(16);
    let url = format!(
        "{AUTH_URL}?client_id={}&redirect_uri={}&response_type=code&scope={}&code_challenge={}\
         &code_challenge_method=S256&state={}&access_type=offline&prompt=consent",
        encode(&id),
        encode(&redirect),
        encode(SCOPE),
        pkce_challenge(&verifier),
        state,
    );
    Ok(PendingAuth { url, listener, redirect, verifier, state })
}

impl PendingAuth {
    /// Wait (up to 5 minutes, or until `cancel`) for Google to redirect back,
    /// then trade the code for tokens. Returns the account, signed in.
    pub fn finish(self, mut acct: Account, cancel: Arc<AtomicBool>) -> Result<Account> {
        self.listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(300);
        let mut stream = loop {
            if cancel.load(Ordering::Relaxed) {
                return Err("Sign-in cancelled".into());
            }
            if Instant::now() > deadline {
                return Err("Sign-in timed out".into());
            }
            match self.listener.accept() {
                Ok((s, _)) => break s,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(150)),
                Err(e) => return Err(e.to_string()),
            }
        };
        stream.set_nonblocking(false).ok();
        stream.set_read_timeout(Some(Duration::from_secs(10))).ok();
        let mut buf = [0u8; 8192];
        let n = stream.read(&mut buf).unwrap_or(0);
        let request = String::from_utf8_lossy(&buf[..n]);
        let query = request
            .lines()
            .next()
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|p| p.split_once('?'))
            .map(|(_, q)| q)
            .unwrap_or("");
        let param = |k: &str| query.split('&').find_map(|kv| kv.strip_prefix(&format!("{k}="))).map(decode);

        let outcome = match (param("code"), param("state"), param("error")) {
            (_, _, Some(err)) => Err(format!("Google sign-in was not completed ({err})")),
            (Some(code), Some(state), None) if state == self.state => Ok(code),
            _ => Err("Unexpected reply from the browser".to_string()),
        };
        let page = match &outcome {
            Ok(_) => "Planner is connected to Google Drive. You can close this tab.",
            Err(_) => "Sign-in didn't complete. You can close this tab and try again from Planner.",
        };
        let html = format!(
            "<!doctype html><meta charset=utf-8><title>Planner</title>\
             <body style=\"font:16px system-ui;display:grid;place-items:center;height:90vh\"><p>{page}</p>"
        );
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{html}",
            html.len()
        );
        let code = outcome?;

        let (id, secret) = acct.client()?;
        let reply: TokenReply = json(&body(agent().post(TOKEN_URL).send_form([
            ("code", code.as_str()),
            ("client_id", id.as_str()),
            ("client_secret", secret.as_str()),
            ("redirect_uri", self.redirect.as_str()),
            ("grant_type", "authorization_code"),
            ("code_verifier", self.verifier.as_str()),
        ]))?)?;
        let refresh = reply.refresh_token.ok_or("Google didn't return a refresh token; try signing in again")?;
        *token_cache().lock().unwrap() = Some((
            refresh.clone(),
            reply.access_token,
            Instant::now() + Duration::from_secs(reply.expires_in.saturating_sub(60)),
        ));
        acct.refresh_token = Some(refresh);
        acct.email = user_email(&acct).ok();
        Ok(acct)
    }
}

/// Best effort: tell Google to drop the token.
pub fn revoke(acct: &Account) {
    if let Some(t) = &acct.refresh_token {
        let _ = agent().post(REVOKE_URL).send_form([("token", t.as_str())]);
    }
    *token_cache().lock().unwrap() = None;
}

fn user_email(acct: &Account) -> Result<String> {
    let v: serde_json::Value = json(&get(acct, &format!("{API}/about?fields=user(emailAddress)"))?)?;
    v["user"]["emailAddress"].as_str().map(String::from).ok_or_else(|| "No email in reply".into())
}

fn get(acct: &Account, url: &str) -> Result<String> {
    let token = access_token(acct)?;
    body(agent().get(url).header("Authorization", format!("Bearer {token}")).call())
}

// ---------------------------------------------------------------- Drive

#[derive(Deserialize, Debug, Clone)]
pub struct Meta {
    pub id: String,
    pub version: Option<String>,
    #[serde(default)]
    pub trashed: bool,
    #[serde(default)]
    pub parents: Vec<String>,
}

const META_FIELDS: &str = "id,version,trashed,parents";

fn search(acct: &Account, query: &str) -> Result<Vec<Meta>> {
    #[derive(Deserialize)]
    struct List {
        files: Vec<Meta>,
    }
    let url = format!(
        "{API}/files?q={}&spaces=drive&fields=files({META_FIELDS})&orderBy=modifiedTime%20desc&pageSize=10",
        encode(query)
    );
    Ok(json::<List>(&get(acct, &url)?)?.files)
}

/// Quote a value as a Drive query string literal.
fn quoted(s: &str) -> String {
    format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
}

/// The chosen folder at the top of My Drive, if it exists.
fn find_folder(acct: &Account) -> Result<Option<String>> {
    let q = format!("name = {} and mimeType = '{FOLDER_MIME}' and 'root' in parents and trashed = false", quoted(&acct.folder));
    Ok(search(acct, &q)?.into_iter().next().map(|m| m.id))
}

/// The data file inside the chosen folder, if there is one.
pub fn find_file(acct: &Account) -> Result<Option<Meta>> {
    let Some(folder) = find_folder(acct)? else { return Ok(None) };
    if let Some(id) = &acct.file_id
        && let Ok(text) = get(acct, &format!("{API}/files/{id}?fields={META_FIELDS}"))
    {
        let meta: Meta = json(&text)?;
        if !meta.trashed && meta.parents.contains(&folder) {
            return Ok(Some(meta));
        }
    }
    let q = format!("name = '{FILE_NAME}' and {} in parents and trashed = false", quoted(&folder));
    Ok(search(acct, &q)?.into_iter().next())
}

pub fn download(acct: &Account, id: &str) -> Result<String> {
    get(acct, &format!("{API}/files/{id}?alt=media"))
}

/// Upload `data`, creating the folder and file the first time.
pub fn upload(acct: &Account, data: &str) -> Result<Meta> {
    let id = match find_file(acct)? {
        Some(meta) => meta.id,
        None => {
            let folder = match find_folder(acct)? {
                Some(f) => f,
                None => create(acct, serde_json::json!({ "name": acct.folder, "mimeType": FOLDER_MIME, "parents": ["root"] }))?,
            };
            create(acct, serde_json::json!({ "name": FILE_NAME, "parents": [folder], "mimeType": "application/json" }))?
        }
    };
    let token = access_token(acct)?;
    json(&body(
        agent()
            .patch(format!("{UPLOAD_API}/files/{id}?uploadType=media&fields={META_FIELDS}"))
            .header("Authorization", format!("Bearer {token}"))
            .header("Content-Type", "application/json")
            .send(data.as_bytes()),
    )?)
}

fn create(acct: &Account, metadata: serde_json::Value) -> Result<String> {
    let token = access_token(acct)?;
    let text = body(
        agent()
            .post(format!("{API}/files?fields=id"))
            .header("Authorization", format!("Bearer {token}"))
            .send_json(metadata),
    )?;
    let v: serde_json::Value = json(&text)?;
    v["id"].as_str().map(String::from).ok_or_else(|| "Drive didn't return a file id".into())
}

// ---------------------------------------------------------------- helpers

fn random_token(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut buf)).expect("random bytes");
    base64url(&buf)
}

fn pkce_challenge(verifier: &str) -> String {
    let mut sum = gtk::glib::Checksum::new(gtk::glib::ChecksumType::Sha256).expect("sha256");
    sum.update(verifier.as_bytes());
    base64url(&sum.digest())
}

fn base64url(data: &[u8]) -> String {
    gtk::glib::base64_encode(data).replace('+', "-").replace('/', "_").trim_end_matches('=').to_string()
}

/// Percent-encode for a URL query value.
fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 3 <= bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok());
            if let Some(v) = hex {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_client() -> Account {
        Account { client_id: "id".into(), client_secret: "secret".into(), ..Default::default() }
    }

    #[test]
    fn url_encoding_round_trips() {
        let s = "4/0Ab_x+y z/é?&=";
        assert_eq!(decode(&encode(s)), s);
        assert_eq!(encode("http://127.0.0.1:8080"), "http%3A%2F%2F127.0.0.1%3A8080");
        assert_eq!(decode("a%2"), "a%2");
    }

    #[test]
    fn pkce_challenge_is_base64url_sha256() {
        // Expected value computed independently:
        // base64.urlsafe_b64encode(hashlib.sha256(verifier).digest()).rstrip("=")
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWnOEjXk"),
            "JeGxM-AFIzw6jttzaxRHhAH-ZgSJs3LN_xzZXDwgwho"
        );
        assert!(!random_token(32).contains(['+', '/', '=']));
    }

    /// The browser redirect is read off the loopback port; a reply whose
    /// `state` doesn't match is rejected before anything is sent to Google.
    #[test]
    fn loopback_rejects_wrong_state_and_answers_browser() {
        let acct = with_client();
        let pending = begin_auth(&acct).unwrap();
        assert!(pending.url.contains("code_challenge_method=S256"));
        assert!(pending.url.contains("scope=https%3A%2F%2Fwww.googleapis.com%2Fauth%2Fdrive.file"));
        let redirect = pending.redirect.clone();
        let browser = std::thread::spawn(move || {
            let addr = redirect.trim_start_matches("http://");
            let mut s = std::net::TcpStream::connect(addr).unwrap();
            write!(s, "GET /?state=forged&code=abc HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
            let mut reply = String::new();
            s.read_to_string(&mut reply).unwrap();
            reply
        });
        let err = pending.finish(acct, Arc::new(AtomicBool::new(false))).unwrap_err();
        assert_eq!(err, "Unexpected reply from the browser");
        let reply = browser.join().unwrap();
        assert!(reply.starts_with("HTTP/1.1 200 OK") && reply.contains("didn't complete"));
    }

    #[test]
    fn sign_in_can_be_cancelled() {
        let acct = with_client();
        let pending = begin_auth(&acct).unwrap();
        assert_eq!(pending.finish(acct, Arc::new(AtomicBool::new(true))).unwrap_err(), "Sign-in cancelled");
    }

    #[test]
    fn query_values_are_escaped() {
        assert_eq!(quoted("My 'Plans'"), r"'My \'Plans\''");
    }

    #[test]
    fn older_settings_files_still_load() {
        // The Online Accounts version stored a GOA id; it's ignored now.
        let old = r#"{"goa_id":"account_1","email":"a@b.c","folder":"Plans","dirty":true}"#;
        let acct: Account = serde_json::from_str(old).unwrap();
        assert!(!acct.signed_in());
        assert_eq!(acct.folder, "Plans");
    }

    #[test]
    fn changing_folder_forgets_the_old_file() {
        let mut acct = Account { file_id: Some("f".into()), version: Some("3".into()), ..Default::default() };
        acct.set_folder("  Plans 2026 ");
        assert_eq!((acct.folder.as_str(), acct.file_id, acct.version), ("Plans 2026", None, None));
    }
}
