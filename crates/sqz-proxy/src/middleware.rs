use axum::extract::Request;
use axum::http::HeaderValue;
use axum::middleware::Next;
use axum::response::Response;

/// Middleware that ensures every request has an `X-Request-ID` header.
///
/// If the incoming request already carries the header it is left untouched;
/// otherwise a new UUID v4 is generated and attached.
pub async fn request_id_layer(mut req: Request, next: Next) -> Response {
    if !req.headers().contains_key("x-request-id") {
        let id = uuid::Uuid::new_v4().to_string();
        if let Ok(val) = HeaderValue::from_str(&id) {
            req.headers_mut().insert("x-request-id", val);
        }
    }

    let request_id = req
        .headers()
        .get("x-request-id")
        .cloned();

    let mut response = next.run(req).await;

    // Echo the request ID back on the response
    if let Some(id) = request_id {
        response.headers_mut().insert("x-request-id", id);
    }

    response
}

/// Middleware that measures wall-clock time for each request and attaches an
/// `X-Response-Time` header (value in milliseconds) to the response.
pub async fn timing_layer(req: Request, next: Next) -> Response {
    let start = std::time::Instant::now();

    let mut response = next.run(req).await;

    let elapsed = start.elapsed();
    let ms = elapsed.as_secs_f64() * 1000.0;
    if let Ok(val) = HeaderValue::from_str(&format!("{ms:.2}ms")) {
        response.headers_mut().insert("x-response-time", val);
    }

    response
}
