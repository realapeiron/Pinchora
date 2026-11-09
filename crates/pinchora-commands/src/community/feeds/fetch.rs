//! Fetching a feed from the open internet without letting a server point the bot at a private network.
//!
//! Every address a host name resolves to must be public ([`is_public`]) or the request is refused before
//! a connection is made, and the connection goes to exactly the addresses that were checked, so a name
//! that resolves somewhere else a moment later (DNS rebinding) gains nothing. Hosts written as an IP
//! address are checked the same way. Redirects are followed by hand, at most [`MAX_REDIRECTS`], each one
//! checked again (and never from https down to http); a body is read up to [`MAX_BYTES`] and the whole
//! fetch, redirects included, gets [`TIMEOUT`]. No proxy from the environment is used: a proxy would
//! resolve the names itself.

use std::{
    error::Error as StdError,
    fmt,
    net::{IpAddr, SocketAddr},
    sync::{Arc, LazyLock, RwLock},
    time::Duration,
};

use pinchora_core::community::feeds::is_public;
use reqwest::StatusCode;
use reqwest::Url;
use reqwest::dns::Addrs;
use reqwest::dns::Name;
use reqwest::dns::Resolve;
use reqwest::dns::Resolving;
use reqwest::header;
use reqwest::header::HeaderMap;
use reqwest::header::HeaderValue;
use reqwest::redirect::Policy;

/// The most redirects one fetch follows.
pub const MAX_REDIRECTS: usize = 3;
/// The largest body read.
pub const MAX_BYTES: usize = 1024 * 1024;
/// How long one fetch may take, redirects included.
pub const TIMEOUT: Duration = Duration::from_secs(10);

/// Reddit asks for a descriptive agent with a way to reach the operator.
const USER_AGENT: &str = "Pinchora/0.1 (Discord bot; +https://github.com/realapeiron/Pinchora)";

/// Why a fetch failed. [`FetchError::code`] is what the dashboard stores and the catalogs word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchError {
    /// The address (or a redirect) points inside a private, local or reserved network.
    Blocked,
    /// Not an http(s) address, or a redirect to one, or from https down to http.
    BadUrl,
    TooManyRedirects,
    TooLarge,
    Timeout,
    NotFound,
    /// Asked to slow down; seconds to wait, when it said.
    RateLimited(Option<u64>),
    Status(u16),
    Network,
}

impl FetchError {
    /// The stored form, `feeds.error.<what>` with `|<status>` for an HTTP status.
    pub fn code(&self) -> String {
        match self {
            Self::Blocked => "feeds.error.blocked".to_owned(),
            Self::BadUrl => "feeds.error.bad-url".to_owned(),
            Self::TooManyRedirects => "feeds.error.redirects".to_owned(),
            Self::TooLarge => "feeds.error.too-large".to_owned(),
            Self::Timeout => "feeds.error.timeout".to_owned(),
            Self::NotFound => "feeds.error.not-found".to_owned(),
            Self::RateLimited(_) => "feeds.error.rate-limited".to_owned(),
            Self::Status(status) => format!("feeds.error.status|{status}"),
            Self::Network => "feeds.error.network".to_owned(),
        }
    }
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.code())
    }
}

impl StdError for FetchError {}

/// What to fetch.
#[derive(Debug, Clone, Default)]
pub struct Request<'a> {
    pub url: &'a str,
    pub etag: Option<&'a str>,
    pub last_modified: Option<&'a str>,
    /// JSON rather than a feed.
    pub json: bool,
    /// A web page whose start is enough: past [`MAX_BYTES`] the rest is dropped instead of failing.
    pub page: bool,
    /// A host the operator configured (the bridge): its own address is not checked, but where it
    /// redirects to is.
    pub trusted_host: Option<&'a str>,
}

/// What a fetch brought back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fetched {
    /// The validators matched: nothing changed since the last fetch.
    NotModified,
    Body {
        bytes: Vec<u8>,
        etag: Option<String>,
        last_modified: Option<String>,
    },
}

/// Fetches feeds. The process uses one ([`fetcher`]); tests and the e2e dashboard point it at a local
/// server instead ([`install_fetcher`]).
pub struct Fetcher {
    /// Resolves through [`PublicOnly`].
    guarded: reqwest::Client,
    /// For the operator's own bridge host only.
    open: reqwest::Client,
    /// Debug builds only: every request goes to this local server, path `/<host><path>`.
    local: Option<SocketAddr>,
}

static FETCHER: LazyLock<RwLock<Arc<Fetcher>>> = LazyLock::new(|| RwLock::new(Arc::new(Fetcher::from_env())));

/// The fetcher this process uses.
pub fn fetcher() -> Arc<Fetcher> {
    FETCHER.read().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Replaces the fetcher this process uses.
pub fn install_fetcher(fetcher: Fetcher) {
    *FETCHER.write().unwrap_or_else(|e| e.into_inner()) = Arc::new(fetcher);
}

impl Fetcher {
    /// The fetcher for the open internet.
    pub fn guarded() -> Self {
        let builder = || {
            reqwest::Client::builder()
                .user_agent(USER_AGENT)
                .redirect(Policy::none())
                .no_proxy()
                .connect_timeout(Duration::from_secs(5))
                .timeout(TIMEOUT)
        };
        Self {
            guarded: builder().dns_resolver(PublicOnly).build().expect("feed client"),
            open: builder().build().expect("feed client"),
            local: None,
        }
    }

    /// Debug builds only: a fetcher that sends every request to the server at `addr` (which may be on
    /// this machine), as `http://addr/<host><path>?<query>`, so tests never reach the internet. The
    /// address checks still apply to the addresses as written.
    #[cfg(debug_assertions)]
    pub fn local(addr: SocketAddr) -> Self {
        Self {
            local: Some(addr),
            ..Self::guarded()
        }
    }

    /// [`Fetcher::guarded`], or in a debug build [`Fetcher::local`] when `PINCHORA_FEEDS__DEV_ENDPOINT`
    /// names a loopback `host:port` (the dashboard's e2e tests).
    fn from_env() -> Self {
        #[cfg(debug_assertions)]
        if let Some(addr) = std::env::var("PINCHORA_FEEDS__DEV_ENDPOINT")
            .ok()
            .and_then(|raw| {
                raw.trim_start_matches("http://")
                    .trim_end_matches('/')
                    .parse::<SocketAddr>()
                    .ok()
            })
            .filter(|addr| addr.ip().is_loopback())
        {
            return Self::local(addr);
        }
        Self::guarded()
    }

    /// Fetches `request.url`, following redirects; see the module docs for the limits.
    pub async fn get(&self, request: &Request<'_>) -> Result<Fetched, FetchError> {
        match tokio::time::timeout(TIMEOUT, self.follow(request)).await {
            Ok(result) => result,
            Err(_) => Err(FetchError::Timeout),
        }
    }

    async fn follow(&self, request: &Request<'_>) -> Result<Fetched, FetchError> {
        let mut url = Url::parse(request.url).map_err(|_| FetchError::BadUrl)?;
        for hop in 0..=MAX_REDIRECTS {
            let trusted = request.trusted_host.is_some_and(|host| url.host_str() == Some(host));
            check(&url, trusted)?;
            let response = self.send(&url, request, hop == 0, trusted).await?;
            let status = response.status();
            if status.is_redirection() && status != StatusCode::NOT_MODIFIED {
                let next = response
                    .headers()
                    .get(header::LOCATION)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|location| url.join(location).ok())
                    .ok_or(FetchError::BadUrl)?;
                // Where it points is checked first, so a redirect into a private network says so.
                let trusted_next = request.trusted_host.is_some_and(|host| next.host_str() == Some(host));
                check(&next, trusted_next)?;
                if url.scheme() == "https" && next.scheme() != "https" {
                    return Err(FetchError::BadUrl);
                }
                url = next;
                continue;
            }
            return read(response, request.page).await;
        }
        Err(FetchError::TooManyRedirects)
    }

    async fn send(
        &self,
        url: &Url,
        request: &Request<'_>,
        first: bool,
        trusted: bool,
    ) -> Result<reqwest::Response, FetchError> {
        let wire = match self.local {
            Some(addr) => local_url(addr, url)?,
            None => url.clone(),
        };
        let client = if trusted || self.local.is_some() {
            &self.open
        } else {
            &self.guarded
        };
        let accept = if request.json {
            "application/json"
        } else if request.page {
            "text/html"
        } else {
            "application/atom+xml, application/rss+xml, application/xml;q=0.9, text/xml;q=0.9, */*;q=0.5"
        };
        let mut headers = HeaderMap::new();
        headers.insert(header::ACCEPT, HeaderValue::from_static(accept));
        headers.insert(header::ACCEPT_LANGUAGE, HeaderValue::from_static("en"));
        if request.page {
            // YouTube shows a consent page instead of the channel to visitors from some countries.
            headers.insert(header::COOKIE, HeaderValue::from_static("CONSENT=YES+1"));
        }
        // The validators belong to the address that was asked for, not to where it redirects.
        if first {
            if let Some(etag) = request.etag.and_then(|v| HeaderValue::from_str(v).ok()) {
                headers.insert(header::IF_NONE_MATCH, etag);
            }
            if let Some(modified) = request.last_modified.and_then(|v| HeaderValue::from_str(v).ok()) {
                headers.insert(header::IF_MODIFIED_SINCE, modified);
            }
        }
        if self.local.is_some()
            && let Some(host) = url.host_str().and_then(|h| HeaderValue::from_str(h).ok())
        {
            headers.insert(header::HOST, host);
        }
        client.get(wire).headers(headers).send().await.map_err(|error| {
            if blocked(&error) {
                FetchError::Blocked
            } else if error.is_timeout() {
                FetchError::Timeout
            } else {
                tracing::debug!(%error, url = %url, "feed fetch failed");
                FetchError::Network
            }
        })
    }
}

/// Whether `url` may be fetched: http or https, no password, and a host that is not an address inside
/// a private network (a name is checked when it resolves).
fn check(url: &Url, trusted: bool) -> Result<(), FetchError> {
    if !matches!(url.scheme(), "http" | "https") || !url.username().is_empty() || url.password().is_some() {
        return Err(FetchError::BadUrl);
    }
    let ip = match url.host() {
        None => return Err(FetchError::BadUrl),
        Some(url::Host::Ipv4(ip)) => IpAddr::V4(ip),
        Some(url::Host::Ipv6(ip)) => IpAddr::V6(ip),
        Some(url::Host::Domain(_)) => return Ok(()),
    };
    if trusted || is_public(ip) {
        Ok(())
    } else {
        Err(FetchError::Blocked)
    }
}

/// `url` as the local test server serves it: `http://addr/<host>[:port]<path>?<query>`.
fn local_url(addr: SocketAddr, url: &Url) -> Result<Url, FetchError> {
    let host = url.host_str().ok_or(FetchError::BadUrl)?;
    let port = url.port().map(|p| format!(":{p}")).unwrap_or_default();
    let query = url.query().map(|q| format!("?{q}")).unwrap_or_default();
    Url::parse(&format!("http://{addr}/{host}{port}{}{query}", url.path())).map_err(|_| FetchError::BadUrl)
}

/// The body of a final answer, or why there is none.
async fn read(mut response: reqwest::Response, page: bool) -> Result<Fetched, FetchError> {
    let status = response.status();
    if status == StatusCode::NOT_MODIFIED {
        return Ok(Fetched::NotModified);
    }
    if status == StatusCode::NOT_FOUND || status == StatusCode::GONE {
        return Err(FetchError::NotFound);
    }
    if status == StatusCode::TOO_MANY_REQUESTS {
        let wait = response
            .headers()
            .get(header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse().ok());
        return Err(FetchError::RateLimited(wait));
    }
    if !status.is_success() {
        return Err(FetchError::Status(status.as_u16()));
    }
    let header = |name| {
        response
            .headers()
            .get(name)
            .and_then(|v: &HeaderValue| v.to_str().ok())
            .map(str::to_owned)
    };
    let (etag, last_modified) = (header(header::ETAG), header(header::LAST_MODIFIED));
    if !page && response.content_length().is_some_and(|len| len > MAX_BYTES as u64) {
        return Err(FetchError::TooLarge);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| FetchError::Network)? {
        bytes.extend_from_slice(&chunk);
        if bytes.len() > MAX_BYTES {
            if page {
                bytes.truncate(MAX_BYTES);
                break;
            }
            return Err(FetchError::TooLarge);
        }
    }
    Ok(Fetched::Body {
        bytes,
        etag,
        last_modified,
    })
}

/// Whether the request failed because a name resolved into a private network.
fn blocked(error: &reqwest::Error) -> bool {
    let mut source: Option<&(dyn StdError + 'static)> = error.source();
    while let Some(inner) = source {
        if inner.downcast_ref::<PrivateAddress>().is_some() {
            return true;
        }
        source = inner.source();
    }
    false
}

/// A name that resolved (also) into a private network.
#[derive(Debug)]
struct PrivateAddress;

impl fmt::Display for PrivateAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the name resolves to a private address")
    }
}

impl StdError for PrivateAddress {}

/// Resolves names with the system resolver and refuses any whose addresses are not all public.
pub(crate) struct PublicOnly;

impl Resolve for PublicOnly {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_owned();
        Box::pin(async move {
            let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), 0)).await?.collect();
            public_only(addrs)
        })
    }
}

fn public_only(addrs: Vec<SocketAddr>) -> Result<Addrs, Box<dyn StdError + Send + Sync>> {
    if addrs.is_empty() || addrs.iter().any(|a| !is_public(a.ip())) {
        return Err(Box::new(PrivateAddress));
    }
    Ok(Box::new(addrs.into_iter()))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use axum::{
        Router,
        http::{HeaderMap as Headers, StatusCode as Status, Uri},
        response::IntoResponse,
    };
    use tokio::net::TcpListener;

    use crate::community::feeds::fetch::*;

    #[tokio::test]
    async fn private_addresses_are_refused_before_connecting() {
        let fetcher = Fetcher::guarded();
        for url in [
            "http://127.0.0.1:9/feed",
            "http://10.0.0.1/feed",
            "http://169.254.169.254/latest/meta-data/",
            "http://[::1]:9/feed",
            "http://[::ffff:127.0.0.1]/feed",
            "http://0.0.0.0/",
            // Names are resolved first and refused for where they point.
            "http://localhost:9/feed",
        ] {
            let got = fetcher
                .get(&Request {
                    url,
                    ..Request::default()
                })
                .await;
            assert_eq!(got, Err(FetchError::Blocked), "{url}");
        }
        for url in [
            "ftp://example.com/feed",
            "file:///etc/passwd",
            "https://user:pw@example.com/",
        ] {
            let got = fetcher
                .get(&Request {
                    url,
                    ..Request::default()
                })
                .await;
            assert_eq!(got, Err(FetchError::BadUrl), "{url}");
        }
    }

    #[tokio::test]
    async fn a_name_with_any_private_address_is_refused() {
        let public: SocketAddr = "93.184.215.14:0".parse().unwrap();
        let private: SocketAddr = "192.168.1.5:0".parse().unwrap();
        assert!(public_only(vec![public]).is_ok());
        assert!(
            public_only(vec![public, private]).is_err(),
            "a mixed answer is refused whole"
        );
        assert!(public_only(vec![]).is_err());
        let client = reqwest::Client::builder().dns_resolver(PublicOnly).build().unwrap();
        let error = client.get("http://localhost:9/").send().await.unwrap_err();
        assert!(blocked(&error), "{error:?}");
    }

    /// A local server: `/big` is 2 MB, `/hop/<n>` redirects n more times, `/private` redirects to a
    /// private address, `/etag` answers 304 to its own validator.
    async fn server() -> SocketAddr {
        static HITS: AtomicUsize = AtomicUsize::new(0);
        async fn handle(uri: Uri, headers: Headers) -> axum::response::Response {
            HITS.fetch_add(1, Ordering::Relaxed);
            let path = uri.path();
            let rest = path.split_once('/').map_or(path, |(_, r)| r);
            let rest = rest.split_once('/').map_or("", |(_, r)| r);
            match rest {
                "big" => "x".repeat(2 * MAX_BYTES).into_response(),
                "page" => format!("<head>{}</head>", "y".repeat(2 * MAX_BYTES)).into_response(),
                "private" => (Status::FOUND, [("location", "http://192.168.0.1/admin")]).into_response(),
                "downgrade" => (Status::FOUND, [("location", "http://feeds.example/plain")]).into_response(),
                "etag" if headers.get("if-none-match").is_some_and(|v| v == "\"v1\"") => {
                    Status::NOT_MODIFIED.into_response()
                }
                "etag" => (Status::OK, [("etag", "\"v1\"")], "<rss/>").into_response(),
                "gone" => Status::GONE.into_response(),
                "slow-down" => (Status::TOO_MANY_REQUESTS, [("retry-after", "120")]).into_response(),
                hop if hop.starts_with("hop/") => {
                    let n: usize = hop[4..].parse().unwrap();
                    if n == 0 {
                        "arrived".into_response()
                    } else {
                        let next = format!("/hop/{}", n - 1);
                        (Status::MOVED_PERMANENTLY, [("location", next)]).into_response()
                    }
                }
                _ => Status::IM_A_TEAPOT.into_response(),
            }
        }
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(axum::serve(listener, Router::new().fallback(handle)).into_future());
        addr
    }

    fn body(fetched: Result<Fetched, FetchError>) -> Vec<u8> {
        match fetched {
            Ok(Fetched::Body { bytes, .. }) => bytes,
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn redirects_size_and_validators() {
        let fetcher = Fetcher::local(server().await);
        let get = |url: &'static str| {
            let fetcher = &fetcher;
            async move {
                fetcher
                    .get(&Request {
                        url,
                        ..Request::default()
                    })
                    .await
            }
        };
        assert_eq!(body(get("https://feeds.example/hop/3").await), b"arrived");
        assert_eq!(
            get("https://feeds.example/hop/4").await,
            Err(FetchError::TooManyRedirects)
        );
        assert_eq!(get("https://feeds.example/private").await, Err(FetchError::Blocked));
        assert_eq!(get("https://feeds.example/downgrade").await, Err(FetchError::BadUrl));
        assert_eq!(get("https://feeds.example/big").await, Err(FetchError::TooLarge));
        assert_eq!(get("https://feeds.example/gone").await, Err(FetchError::NotFound));
        assert_eq!(
            get("https://feeds.example/slow-down").await,
            Err(FetchError::RateLimited(Some(120)))
        );
        assert_eq!(get("https://feeds.example/teapot").await, Err(FetchError::Status(418)));
        assert_eq!(get("http://10.1.1.1/feed").await, Err(FetchError::Blocked));
        let page = fetcher
            .get(&Request {
                url: "https://feeds.example/page",
                page: true,
                ..Request::default()
            })
            .await;
        assert_eq!(
            body(page).len(),
            MAX_BYTES,
            "a page is cut at the limit instead of refused"
        );

        let first = fetcher
            .get(&Request {
                url: "https://feeds.example/etag",
                ..Request::default()
            })
            .await;
        let Ok(Fetched::Body { etag, .. }) = first else {
            panic!("{first:?}")
        };
        assert_eq!(etag.as_deref(), Some("\"v1\""));
        let again = fetcher
            .get(&Request {
                url: "https://feeds.example/etag",
                etag: etag.as_deref(),
                ..Request::default()
            })
            .await;
        assert_eq!(again, Ok(Fetched::NotModified));
    }
}
