//! Google Drive storage. No GTK widgets here: everything is blocking and meant
//! to run on a worker thread (see `sync.rs`).
//!
//! - Sign-in: none of our own. The Google account added in Ubuntu Settings →
//!   Online Accounts (GNOME Online Accounts, "GOA") hands out access tokens
//!   over D-Bus, so Planner needs no OAuth client id or secret.
//! - Data: one JSON file, `My Drive/<folder>/planner-data.json`. The folder
//!   (default "Planner") is chosen by the user. GOA's token can reach the whole
//!   Drive, so staying inside that folder is enforced here: every lookup is
//!   scoped to the folder, and Planner never lists or touches anything else.
//! - Settings live in `~/.config/planner/google-drive.json` (no secrets in it).

use gtk::{gio, glib};
use serde::{Deserialize, Serialize};
use std::{fs, io, path::PathBuf, sync::OnceLock, time::Duration};

const GOA: &str = "org.gnome.OnlineAccounts";
const GOA_ROOT: &str = "/org/gnome/OnlineAccounts";
const GOA_ACCOUNT: &str = "org.gnome.OnlineAccounts.Account";
const GOA_OAUTH2: &str = "org.gnome.OnlineAccounts.OAuth2Based";
const API: &str = "https://www.googleapis.com/drive/v3";
const UPLOAD_API: &str = "https://www.googleapis.com/upload/drive/v3";
pub const DEFAULT_FOLDER: &str = "Planner";
pub const FILE_NAME: &str = "planner-data.json";
const FOLDER_MIME: &str = "application/vnd.google-apps.folder";

pub type Result<T> = std::result::Result<T, String>;

/// Which Google account and folder Planner syncs with, plus sync bookkeeping.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Account {
    /// GNOME Online Accounts id of the Google account (None = not connected).
    pub goa_id: Option<String>,
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
            goa_id: None,
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

    pub fn save(&self) -> io::Result<()> {
        let path = Self::path();
        fs::create_dir_all(path.parent().unwrap())?;
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_string_pretty(self).expect("account serializes"))?;
        fs::rename(tmp, path)
    }

    pub fn signed_in(&self) -> bool {
        self.goa_id.is_some()
    }

    /// Disconnect, remembering the folder name for next time.
    pub fn sign_out(&mut self) {
        *self = Account { folder: self.folder.clone(), ..Default::default() };
    }

    /// Point at another folder: the file there (if any) is found on next sync.
    pub fn set_folder(&mut self, folder: &str) {
        self.folder = folder.trim().to_string();
        self.file_id = None;
        self.version = None;
    }
}

// ---------------------------------------------------------------- GNOME Online Accounts

/// A Google account from Settings → Online Accounts.
#[derive(Clone, Debug)]
pub struct GoaAccount {
    pub id: String,
    path: String,
    pub email: String,
    /// GNOME needs you to sign in again (e.g. the password changed or access expired).
    pub attention_needed: bool,
}

fn session_bus() -> Result<gio::DBusConnection> {
    gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).map_err(|e| e.to_string())
}

/// Google accounts set up in Ubuntu Settings (empty if Online Accounts isn't running).
pub fn google_accounts() -> Vec<GoaAccount> {
    let Ok(bus) = session_bus() else { return Vec::new() };
    let Ok(reply) = bus.call_sync(
        Some(GOA),
        GOA_ROOT,
        "org.freedesktop.DBus.ObjectManager",
        "GetManagedObjects",
        None,
        glib::VariantTy::new("(a{oa{sa{sv}}})").ok(),
        gio::DBusCallFlags::NONE,
        5000,
        gio::Cancellable::NONE,
    ) else {
        return Vec::new();
    };
    let objects = reply.child_value(0);
    let mut out = Vec::new();
    for i in 0..objects.n_children() {
        let entry = objects.child_value(i);
        let path = entry.child_value(0).str().unwrap_or_default().to_string();
        let ifaces = entry.child_value(1);
        let Some(props) = dict_get(&ifaces, GOA_ACCOUNT) else { continue };
        if dict_get(&ifaces, GOA_OAUTH2).is_none() {
            continue;
        }
        let text = |k: &str| dict_get(&props, k).and_then(|v| v.str().map(String::from)).unwrap_or_default();
        if text("ProviderType") != "google" {
            continue;
        }
        out.push(GoaAccount {
            id: text("Id"),
            path,
            email: text("Identity"),
            attention_needed: dict_get(&props, "AttentionNeeded").and_then(|v| v.get::<bool>()).unwrap_or(false),
        });
    }
    out
}

/// Value for `key` in a D-Bus dictionary (`a{s…}`), unboxing `v` values.
fn dict_get(dict: &glib::Variant, key: &str) -> Option<glib::Variant> {
    (0..dict.n_children()).map(|i| dict.child_value(i)).find(|e| e.child_value(0).str() == Some(key)).map(|e| {
        let v = e.child_value(1);
        if v.is_type(glib::VariantTy::VARIANT) { v.as_variant().unwrap_or(v) } else { v }
    })
}

pub const NEEDS_SIGN_IN: &str = "Your Google account needs you to sign in again in Settings → Online Accounts";

fn access_token(acct: &Account) -> Result<String> {
    let id = acct.goa_id.as_deref().ok_or("Not connected")?;
    let goa = google_accounts()
        .into_iter()
        .find(|a| a.id == id)
        .ok_or("The Google account was removed from Settings → Online Accounts")?;
    let reply = session_bus()?
        .call_sync(
            Some(GOA),
            &goa.path,
            GOA_OAUTH2,
            "GetAccessToken",
            None,
            glib::VariantTy::new("(si)").ok(),
            gio::DBusCallFlags::NONE,
            20000,
            gio::Cancellable::NONE,
        )
        .map_err(|e| if e.message().contains("NotAuthorized") { NEEDS_SIGN_IN.to_string() } else { e.to_string() })?;
    reply.child_value(0).str().map(String::from).ok_or_else(|| "No access token".into())
}

/// The account was removed from Settings: sync can't continue until reconnected.
pub fn is_account_gone(err: &str) -> bool {
    err.contains("removed from Settings")
}

pub fn needs_sign_in(err: &str) -> bool {
    err == NEEDS_SIGN_IN
}

/// Open Settings → Online Accounts.
pub fn open_online_accounts() {
    let _ = std::process::Command::new("gnome-control-center").arg("online-accounts").spawn();
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
    let detail = serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|v| v["error"]["message"].as_str().or(v["error"].as_str()).map(String::from))
        .unwrap_or(text);
    if status.as_u16() == 403 && detail.to_lowercase().contains("scope") {
        return Err("This Google account doesn't give apps Drive access. In Settings → Online Accounts → Google, turn on Files.".into());
    }
    Err(format!("Google returned {}: {detail}", status.as_u16()))
}

fn json<T: for<'de> Deserialize<'de>>(text: &str) -> Result<T> {
    serde_json::from_str(text).map_err(|e| format!("Unexpected reply from Google: {e}"))
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
    #[serde(rename = "modifiedTime")]
    pub modified: Option<String>,
    #[serde(default)]
    pub trashed: bool,
    #[serde(default)]
    pub parents: Vec<String>,
}

const META_FIELDS: &str = "id,version,modifiedTime,trashed,parents";

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

/// Percent-encode for a URL query value.
fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_values_are_escaped() {
        assert_eq!(quoted("My 'Plans'"), r"'My \'Plans\''");
        assert_eq!(encode("name = 'a b'"), "name%20%3D%20%27a%20b%27");
    }

    #[test]
    fn old_settings_files_still_load() {
        // The previous version stored an OAuth client; those fields are ignored.
        let old = r#"{"client_id":"x","client_secret":"y","refresh_token":null,"email":null,"dirty":false}"#;
        let acct: Account = serde_json::from_str(old).unwrap();
        assert!(!acct.signed_in());
        assert_eq!(acct.folder, "Planner");
    }

    #[test]
    fn changing_folder_forgets_the_old_file() {
        let mut acct = Account { file_id: Some("f".into()), version: Some("3".into()), ..Default::default() };
        acct.set_folder("  Plans 2026 ");
        assert_eq!((acct.folder.as_str(), acct.file_id, acct.version), ("Plans 2026", None, None));
    }

    /// Reads (never writes) the real Online Accounts service when available.
    #[test]
    fn lists_google_accounts_without_panicking() {
        for a in google_accounts() {
            assert!(!a.id.is_empty());
        }
    }
}
