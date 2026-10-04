//! Builds the Google OAuth client into the binary, so users just "Sign in with Google".
//!
//! Source, first match wins:
//! 1. env PLANNER_GOOGLE_CLIENT_ID + PLANNER_GOOGLE_CLIENT_SECRET (e.g. CI secrets)
//! 2. `google-client.json` next to Cargo.toml: the file Google Cloud Console
//!    downloads for a "Desktop app" client ({"installed": {"client_id", "client_secret", …}}).
//!    It is git-ignored; never commit it.
//!
//! Without either, the app still builds and asks for the client in its sign-in window.

fn main() {
    println!("cargo:rerun-if-changed=google-client.json");
    println!("cargo:rerun-if-env-changed=PLANNER_GOOGLE_CLIENT_ID");
    println!("cargo:rerun-if-env-changed=PLANNER_GOOGLE_CLIENT_SECRET");

    let from_env = std::env::var("PLANNER_GOOGLE_CLIENT_ID").ok().zip(std::env::var("PLANNER_GOOGLE_CLIENT_SECRET").ok());
    let from_file = || -> Option<(String, String)> {
        let text = std::fs::read_to_string("google-client.json").ok()?;
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
