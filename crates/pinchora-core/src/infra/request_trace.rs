use axum::{extract::Request, middleware::Next, response::Response};
use tracing::Instrument;

pub async fn request(request: Request, next: Next) -> Response {
    let span = request_span(request.method().as_str(), request.uri().path());
    async move {
        let response = next.run(request).await;
        tracing::Span::current().record("status", response.status().as_u16());
        response
    }
    .instrument(span)
    .await
}

/// A fresh identifier per transport request; never trust or log caller-provided credentials.
pub fn request_span(method: &str, path: &str) -> tracing::Span {
    let request_id = format!("{:032x}", rand::random::<u128>());
    tracing::info_span!("http.request", %request_id, method, path, status = tracing::field::Empty)
}
