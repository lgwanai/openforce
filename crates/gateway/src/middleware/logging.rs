use axum::extract::Request;
use axum::response::Response;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;
use tower::{Layer, Service};
use tracing::info;

#[derive(Clone)]
pub struct LoggingLayer;

impl<S> Layer<S> for LoggingLayer {
    type Service = LoggingService<S>;
    fn layer(&self, inner: S) -> Self::Service { LoggingService { inner } }
}

#[derive(Clone)]
pub struct LoggingService<S> { inner: S }

impl<S, B> Service<Request<B>> for LoggingService<S>
where
    S: Service<Request<B>, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
    B: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<B>) -> Self::Future {
        let start = Instant::now();
        let method = req.method().to_string();
        let path = req.uri().path().to_string();
        let fut = self.inner.call(req);
        Box::pin(async move {
            let resp = fut.await;
            let elapsed = start.elapsed();
            let status = resp.as_ref().map(|r| r.status().as_u16()).unwrap_or(500);
            info!("{method} {path} -> {status} ({elapsed:?})");
            resp
        })
    }
}
