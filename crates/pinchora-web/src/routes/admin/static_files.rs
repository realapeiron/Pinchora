//! The SPA (built into the binary, or from `PINCHORA_WEB__STATIC_DIR`), the card fonts, and the setup
//! page shown while Discord login is not configured.

use std::{
    collections::HashMap,
    path::{Component, Path},
    sync::LazyLock,
};

use axum::{
    Router,
    body::Bytes,
    extract::{Path as UrlPath, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, Uri, header},
    response::{IntoResponse, Response},
    routing::get,
};
use pinchora_render::FONT_FILES;
use sha2::{Digest, Sha256};

use crate::state::WebState;

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/spa.rs"));
}

/// Vite's hashed output never changes under one name.
const IMMUTABLE: &str = "public, max-age=31536000, immutable";

pub fn router() -> Router<WebState> {
    Router::new()
        .route("/assets/fonts/{name}", get(font))
        .route("/setup.css", get(setup_css))
        .fallback(spa)
}

/// Fonts by name, with a strong ETag from their bytes.
static FONTS: LazyLock<HashMap<&'static str, (&'static [u8], String)>> = LazyLock::new(|| {
    FONT_FILES
        .iter()
        .map(|&(name, bytes)| (name, (bytes, etag(bytes))))
        .collect()
});

fn etag(bytes: &[u8]) -> String {
    format!("\"{}\"", &hex::encode(Sha256::digest(bytes))[..16])
}

/// `GET /assets/fonts/{name}`: the TTFs the cards are drawn in.
async fn font(UrlPath(name): UrlPath<String>, headers: HeaderMap) -> Response {
    let Some((bytes, tag)) = FONTS.get(name.as_str()) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let cached = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == tag);
    let mut response = if cached {
        StatusCode::NOT_MODIFIED.into_response()
    } else {
        ([(header::CONTENT_TYPE, "font/ttf")], *bytes).into_response()
    };
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=86400"));
    if let Ok(tag) = HeaderValue::from_str(tag) {
        headers.insert(header::ETAG, tag);
    }
    response
}

/// Text files of the build that name the site's own address (canonical links, the sitemap): this
/// word in them becomes the public URL, so one build serves any domain.
const ORIGIN_PLACEHOLDER: &str = "__ORIGIN__";

/// Every other GET: a built file when one matches, a page of the public site (`/` is `index.html`, `/commands`
/// is `commands/index.html`, written out at build time so crawlers and link previews see each page's own
/// title and tags), else `index.html` for the dashboard's router to handle.
async fn spa(State(state): State<WebState>, method: Method, uri: Uri) -> Response {
    if method != Method::GET && method != Method::HEAD {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    let path = uri.path().trim_matches('/');
    if !path.is_empty()
        && let Some(bytes) = file(&state, path).await
    {
        let cache = if path.starts_with("assets/") {
            IMMUTABLE
        } else {
            "public, max-age=3600"
        };
        let bytes = with_origin(&state, path, bytes);
        return (
            [(header::CONTENT_TYPE, mime(path)), (header::CACHE_CONTROL, cache)],
            bytes,
        )
            .into_response();
    }
    // A missing script or image is a 404, not the app page.
    if path.starts_with("assets/") || path.rsplit('/').next().is_some_and(|last| last.contains('.')) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let page = if path.is_empty() {
        "index.html".to_owned()
    } else {
        format!("{path}/index.html")
    };
    if let Some(bytes) = file(&state, &page).await {
        return html(with_origin(&state, &page, bytes), false);
    }
    if !state.settings.login_ready() && !state.settings.dev_login {
        return setup_page(&state);
    }
    match file(&state, "index.html").await {
        // The shell for the dashboard's own pages: nothing there is for a search engine.
        Some(bytes) => html(with_origin(&state, "index.html", bytes), true),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

fn html(body: Bytes, noindex: bool) -> Response {
    let mut response = (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        body,
    )
        .into_response();
    if noindex {
        response
            .headers_mut()
            .insert("x-robots-tag", HeaderValue::from_static("noindex, nofollow"));
    }
    response
}

/// `bytes` with [`ORIGIN_PLACEHOLDER`] replaced, for the text files that carry it.
fn with_origin(state: &WebState, path: &str, bytes: Bytes) -> Bytes {
    let text = matches!(path.rsplit('.').next(), Some("html" | "xml" | "txt" | "webmanifest"));
    if !text {
        return bytes;
    }
    match std::str::from_utf8(&bytes) {
        Ok(page) if page.contains(ORIGIN_PLACEHOLDER) => {
            Bytes::from(page.replace(ORIGIN_PLACEHOLDER, &state.settings.origin))
        }
        _ => bytes,
    }
}

/// A file of the SPA: from disk when `static_dir` is set, else from the binary.
async fn file(state: &WebState, path: &str) -> Option<Bytes> {
    match &state.settings.static_dir {
        Some(dir) => {
            let relative = Path::new(path);
            if !relative.components().all(|c| matches!(c, Component::Normal(_))) {
                return None;
            }
            tokio::fs::read(dir.join(relative)).await.ok().map(Bytes::from)
        }
        None => crate::routes::admin::static_files::embedded::SPA
            .iter()
            .find(|(name, _)| *name == path)
            .map(|(_, bytes)| Bytes::from_static(bytes)),
    }
}

fn mime(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or_default() {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "webp" => "image/webp",
        "xml" => "application/xml; charset=utf-8",
        "webmanifest" => "application/manifest+json",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// What the dashboard says while it cannot log anyone in: which variables are missing and which
/// redirect to register. `docker compose up` never fails because of the dashboard.
fn setup_page(state: &WebState) -> Response {
    let settings = &state.settings;
    let missing: String = settings
        .missing
        .iter()
        .map(|name| format!("<li><code>{}</code></li>", escape(name)))
        .collect();
    let html = format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Pinchora dashboard: setup</title>
<link rel="stylesheet" href="/setup.css">
</head>
<body>
<main class="panel">
<h1>PINCHORA</h1>
<p class="lead">The dashboard is running, but Discord login is not set up yet.</p>
<h2>Set these in .env</h2>
<ul>{missing}</ul>
<h2>Add this redirect</h2>
<p>Developer Portal &rarr; your application &rarr; OAuth2 &rarr; Redirects:</p>
<p><code>{redirect}</code></p>
<p class="muted">Then restart the web service (<code>docker compose up -d web</code>).</p>
</main>
</body>
</html>
"#,
        redirect = escape(&settings.redirect_uri),
    );
    (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        html,
    )
        .into_response()
}

async fn setup_css() -> Response {
    const CSS: &str = r#"@font-face { font-family: "Silkscreen"; src: url("/assets/fonts/Silkscreen-Regular.ttf"); }
@font-face { font-family: "Noto Sans"; src: url("/assets/fonts/NotoSans-Regular.ttf"); }
body { margin: 0; min-height: 100vh; display: grid; place-items: center; padding: 16px; box-sizing: border-box;
  background: linear-gradient(#1d2b53, #0f1631); color: #fff1e8; font: 16px/1.5 "Noto Sans", sans-serif; }
.panel { max-width: 560px; background: #141b3d; border: 4px solid #2e3d75; padding: 24px 28px; }
h1 { font: 32px "Silkscreen", monospace; color: #ff6b4a; margin: 0 0 8px; letter-spacing: 2px; }
h2 { font: 14px "Silkscreen", monospace; color: #ffd166; margin: 20px 0 6px; }
code { background: #0f1631; border: 2px solid #2e3d75; padding: 1px 6px; color: #06d6a0; word-break: break-all; }
ul { padding-left: 20px; } .muted { color: #8fa0d8; } .lead { margin: 0; }
"#;
    (
        [
            (header::CONTENT_TYPE, "text/css; charset=utf-8"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        CSS,
    )
        .into_response()
}
