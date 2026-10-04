//! Builds the Google OAuth client into the binary, so users just "Sign in with Google".
//!
//! Source, first match wins:
//! 1. env PLANNER_GOOGLE_CLIENT_ID + PLANNER_GOOGLE_CLIENT_SECRET (e.g. CI secrets)
//! 2. `google-client.json`, or `client_secret_<id>.json` as Google names the
//!    download, next to Cargo.toml: the "Desktop app" client file
//!    ({"installed": {"client_id", "client_secret", …}}). Both are git-ignored; never commit them.
//!
//! Without either, the app still builds and asks for the client in its sign-in window.

fn main() {
    println!("cargo:rerun-if-changed=google-client.json");
    println!("cargo:rerun-if-changed=.");
    println!("cargo:rerun-if-env-changed=PLANNER_GOOGLE_CLIENT_ID");
    println!("cargo:rerun-if-env-changed=PLANNER_GOOGLE_CLIENT_SECRET");

    let from_env = std::env::var("PLANNER_GOOGLE_CLIENT_ID").ok().zip(std::env::var("PLANNER_GOOGLE_CLIENT_SECRET").ok());
    let from_file = || -> Option<(String, String)> {
        // google-client.json, or the file as Google names it (client_secret_<id>.json).
        let path = std::iter::once(std::path::PathBuf::from("google-client.json"))
            .chain(std::fs::read_dir(".").ok()?.flatten().map(|e| e.path()).filter(|p| {
                p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("client_secret_") && n.ends_with(".json"))
            }))
            .find(|p| p.exists())?;
        println!("cargo:rerun-if-changed={}", path.display());
        let text = std::fs::read_to_string(path).ok()?;
        let v: serde_json::Value = serde_json::from_str(&text).ok()?;
        let c = v.get("installed").unwrap_or(&v);
        Some((c["client_id"].as_str()?.to_string(), c["client_secret"].as_str()?.to_string()))
    };
    if let Some((id, secret)) = from_env.or_else(from_file) {
        println!("cargo:rustc-env=PLANNER_GOOGLE_CLIENT_ID={id}");
        println!("cargo:rustc-env=PLANNER_GOOGLE_CLIENT_SECRET={secret}");
    } else {
        println!("cargo:warning=No Google OAuth client (google-client.json); Drive sign-in will ask for one at runtime.");
    }
}
