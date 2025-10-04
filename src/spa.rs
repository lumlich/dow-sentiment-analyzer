// src/spa.rs
use axum::Router;
use tower_http::services::{ServeDir, ServeFile};

/// Statické routy (assets + config) a SPA fallback.
/// DŮLEŽITÉ: /config montujeme dřív, aby ho nepřebilo SPA.
pub fn routes() -> Router {
    // SPA fallback na index.html pro cesty, které neodpovídají žádnému souboru v /assets
    let spa = ServeDir::new("assets")
        .not_found_service(ServeFile::new("assets/index.html"));

    Router::new()
        // statický config (JSON/TOML) – musí být před SPA!
        .nest_service("/config", ServeDir::new("config"))
        // bundlované assety z Vite
        .nest_service("/assets", ServeDir::new("assets"))
        // kořen aplikace (a vše ostatní) → SPA fallback
        .nest_service("/", spa)
}
