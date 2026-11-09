//! Observe real HTTP bodies while preserving frame order, errors, trailers and size hints.
use axum::body::{Body, Bytes, HttpBody};
use http_body::{Frame, SizeHint};
use std::{
    pin::Pin,
    task::{Context, Poll},
};

type Recorder = fn(&str, &str, u16, &[u8]);

pub async fn observe(request: axum::extract::Request, next: axum::middleware::Next) -> axum::response::Response {
    let method = request.method().to_string();
    let path = request.uri().path().to_owned();
    let response = next.run(request).await;
    if !path.starts_with("/api/") || !super::contracts::recording() {
        return response;
    }
    let (parts, body) = response.into_parts();
    let observed = ObservedBody::new(body, method, path, parts.status.as_u16(), super::contracts::record);
    axum::response::Response::from_parts(parts, Body::new(observed))
}

struct ObservedBody {
    inner: Body,
    method: String,
    path: String,
    status: u16,
    bytes: Vec<u8>,
    recorded: bool,
    record: Recorder,
}

impl ObservedBody {
    fn new(inner: Body, method: String, path: String, status: u16, record: Recorder) -> Self {
        let mut body = Self {
            inner,
            method,
            path,
            status,
            bytes: Vec::new(),
            recorded: false,
            record,
        };
        if body.inner.is_end_stream() {
            body.finish();
        }
        body
    }

    fn finish(&mut self) {
        if !self.recorded {
            (self.record)(&self.method, &self.path, self.status, &self.bytes);
            self.recorded = true;
            self.bytes.clear();
        }
    }
}

impl HttpBody for ObservedBody {
    type Data = Bytes;
    type Error = axum::Error;

    fn poll_frame(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Frame<Bytes>, Self::Error>>> {
        let this = self.get_mut();
        let frame = Pin::new(&mut this.inner).poll_frame(cx);
        match &frame {
            Poll::Ready(Some(Ok(frame))) => {
                if let Some(data) = frame.data_ref() {
                    this.bytes.extend_from_slice(data);
                }
                if this.inner.is_end_stream() {
                    this.finish();
                }
            }
            Poll::Ready(None) => this.finish(),
            Poll::Ready(Some(Err(_))) => this.recorded = true,
            _ => {}
        }
        frame
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }
    fn size_hint(&self) -> SizeHint {
        self.inner.size_hint()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::VecDeque,
        sync::atomic::{AtomicUsize, Ordering},
    };
    static RECORDED: AtomicUsize = AtomicUsize::new(0);

    struct Frames(VecDeque<Result<Frame<Bytes>, axum::Error>>);
    impl HttpBody for Frames {
        type Data = Bytes;
        type Error = axum::Error;
        fn poll_frame(
            mut self: Pin<&mut Self>,
            _: &mut Context<'_>,
        ) -> Poll<Option<Result<Frame<Bytes>, Self::Error>>> {
            Poll::Ready(self.0.pop_front())
        }
    }

    #[tokio::test]
    async fn observer_preserves_data_frames_trailers_errors_and_unknown_length() {
        let mut trailers = axum::http::HeaderMap::new();
        trailers.insert("x-checksum", "kept".parse().unwrap());
        let frames = Frames(VecDeque::from([
            Ok(Frame::data(Bytes::from_static(b"hello "))),
            Ok(Frame::data(Bytes::from_static(b"world"))),
            Ok(Frame::trailers(trailers.clone())),
        ]));
        let record: Recorder = |method, path, status, body| {
            assert_eq!(
                (method, path, status, body),
                ("GET", "/test", 201, b"hello world".as_slice())
            );
            RECORDED.fetch_add(1, Ordering::SeqCst);
        };
        let mut body = ObservedBody::new(Body::new(frames), "GET".into(), "/test".into(), 201, record);
        assert_eq!(body.size_hint().exact(), None);
        assert_eq!(RECORDED.load(Ordering::SeqCst), 0);
        for expected in [b"hello ".as_slice(), b"world".as_slice()] {
            let frame = futures_util::future::poll_fn(|cx| Pin::new(&mut body).poll_frame(cx))
                .await
                .unwrap()
                .unwrap();
            assert_eq!(frame.into_data().unwrap().as_ref(), expected);
        }
        let frame = futures_util::future::poll_fn(|cx| Pin::new(&mut body).poll_frame(cx))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(frame.into_trailers().unwrap(), trailers);
        assert!(
            futures_util::future::poll_fn(|cx| Pin::new(&mut body).poll_frame(cx))
                .await
                .is_none()
        );
        assert_eq!(RECORDED.load(Ordering::SeqCst), 1);
        let frames = Frames(VecDeque::from([Err(axum::Error::new(std::io::Error::other(
            "original error",
        )))]));
        let mut body = ObservedBody::new(Body::new(frames), "GET".into(), "/test".into(), 200, |_, _, _, _| {
            panic!("partial body recorded")
        });
        let frame = futures_util::future::poll_fn(|cx| Pin::new(&mut body).poll_frame(cx))
            .await
            .unwrap();
        assert_eq!(frame.unwrap_err().to_string(), "original error");
        assert!(
            futures_util::future::poll_fn(|cx| Pin::new(&mut body).poll_frame(cx))
                .await
                .is_none()
        );
    }
}
