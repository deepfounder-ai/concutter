use axum::body::Body;
use axum::response::Response;
use futures::StreamExt;

/// Convert a streaming upstream Anthropic response into an axum SSE response.
///
/// The byte stream from reqwest is forwarded as-is to the client with the
/// appropriate SSE headers set.
pub fn forward_stream(upstream_response: reqwest::Response) -> Response {
    let stream = upstream_response.bytes_stream().map(|result| {
        result.map_err(|e| {
            tracing::error!("stream error: {e}");
            std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
        })
    });

    let body = Body::from_stream(stream);

    Response::builder()
        .header("content-type", "text/event-stream")
        .header("cache-control", "no-cache")
        .header("transfer-encoding", "chunked")
        .body(body)
        .unwrap_or_else(|_| {
            Response::builder()
                .status(500)
                .body(Body::from("internal error building stream response"))
                .unwrap()
        })
}
