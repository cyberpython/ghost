//! A tiny upstream server for manual end-to-end checks.
//!
//! It echoes, as a JSON body, exactly what it received (method, target,
//! headers in order, raw body) so the proxy's transparency can be eyeballed:
//! point `ghost --upstream http://127.0.0.1:9000/app` at it, then
//! `curl -H 'X-Test: abc' -d hello-body http://127.0.0.1:8080/endpoint/path`.

use bytes::Bytes;
use http_body_util::{combinators::BoxBody, BodyExt, Full, Limited};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use hyper::{Request, Response, StatusCode};

type Res = Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error>;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let listen = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:9000".to_string());
    let listener = tokio::net::TcpListener::bind(listen.as_str()).await?;
    tracing::info!(%listen, "origin listening");

    loop {
        let (stream, _peer) = listener.accept().await?;
        let io = TokioIo::new(stream);
        tokio::task::spawn(async move {
            let svc = service_fn(|req| Box::pin(handle(req)));
            let _ = http1::Builder::new()
                .preserve_header_case(true)
                .title_case_headers(true)
                .serve_connection(io, svc)
                .with_upgrades()
                .await;
        });
    }
}

async fn handle(req: Request<Incoming>) -> Res {
    let method = req.method().clone();
    let target = req
        .uri()
        .path_and_query()
        .map(|s| s.to_string())
        .unwrap_or_default();
    let headers: Vec<String> = req
        .headers()
        .iter()
        .map(|(n, v)| format!("{}: {}", n.as_str(), v.to_str().unwrap_or("")))
        .collect();
    let body: Bytes =
        match Limited::new(req.into_body(), 10 * 1024 * 1024).collect().await {
            Ok(c) => c.to_bytes(),
            Err(_) => Bytes::new(),
        };

    let json = serde_json::json!({
        "method": method.to_string(),
        "target": target,
        "headers": headers,
        "body": String::from_utf8_lossy(&body).to_string(),
    });
    let mut resp = Response::new(
        Full::new(Bytes::from(json.to_string().into_bytes()))
            .map_err(|_n| unreachable!())
            .boxed(),
    );
    *resp.status_mut() = StatusCode::OK;
    resp.headers_mut().insert(
        http::HeaderName::from_static("x-origin"),
        http::HeaderValue::from_static("yes"),
    );
    Ok(resp)
}
