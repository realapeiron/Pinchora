//! Observe HTTP responses from the real loopback test server, including raw reqwest calls.
pub async fn observe(request: axum::extract::Request, next: axum::middleware::Next) -> axum::response::Response {
    let method = request.method().to_string();
    let path = request.uri().path().to_owned();
    let response = next.run(request).await;
    if !path.starts_with("/api/") {
        return response;
    }
    let (parts, body) = response.into_parts();
    let body = axum::body::to_bytes(body, 32 * 1024 * 1024)
        .await
        .expect("test response body");
    super::contracts::record(&method, &path, parts.status.as_u16(), &body);
    axum::response::Response::from_parts(parts, axum::body::Body::from(body))
}
