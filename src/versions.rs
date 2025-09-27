// src/versions.rs
// Simple /_version handler returning package name/version and optional build metadata.

use axum::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct VersionInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub build_ts_utc: Option<&'static str>,
    pub git_sha: Option<&'static str>,
}

pub async fn handler() -> Json<VersionInfo> {
    Json(VersionInfo {
        name: env!("CARGO_PKG_NAME"),
        version: env!("CARGO_PKG_VERSION"),
        // These are optional; you can inject them via build.rs or CI envs later.
        build_ts_utc: option_env!("BUILD_TS_UTC"),
        git_sha: option_env!("GIT_SHA"),
    })
}
