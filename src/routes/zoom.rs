use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json},
};
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::crypto::{compute_zoom_url_validation_token, verify_zoom_webhook_signature};
use crate::zoom::{UrlValidationResponse, ZoomWebhookPayload, extract_plain_token};

use super::{AppState, extract_forwarded_headers};

/// Headers to forward from Zoom to n8n webhooks
const ZOOM_FORWARDED_HEADER_PREFIXES: &[&str] = &["x-zm-", "content-type"];

pub async fn handle_zoom_event(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: String,
) -> impl IntoResponse {
    let payload: ZoomWebhookPayload = match serde_json::from_str(&body) {
        Ok(p) => p,
        Err(e) => {
            warn!(error = %e, "Failed to parse Zoom webhook payload");
            debug!(body = %body, "Raw payload that failed to parse");
            return (StatusCode::BAD_REQUEST, "Invalid Zoom webhook payload").into_response();
        }
    };

    let signature = header_value(&headers, "x-zm-signature");
    let timestamp = header_value(&headers, "x-zm-request-timestamp");

    let (Some(signature), Some(timestamp)) = (signature, timestamp) else {
        warn!("Missing Zoom webhook signature headers");
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let secret = &state.config.zoom_webhook_secret;
    if !verify_zoom_webhook_signature(secret, body.as_bytes(), &timestamp, &signature) {
        warn!("Invalid Zoom webhook signature");
        return StatusCode::UNAUTHORIZED.into_response();
    }

    if payload.event == "endpoint.url_validation" {
        let plain_token = match extract_plain_token(&payload.payload) {
            Some(token) => token,
            None => {
                warn!("Missing plainToken in Zoom URL validation payload");
                return (StatusCode::BAD_REQUEST, "Missing plainToken").into_response();
            }
        };

        info!("Received Zoom URL validation challenge");
        let encrypted_token = compute_zoom_url_validation_token(secret, &plain_token);
        return Json(UrlValidationResponse {
            plain_token,
            encrypted_token,
        })
        .into_response();
    }

    if !state.config.is_zoom_event_allowed(&payload.event) {
        info!(
            event = %payload.event,
            "Zoom event not on platform allowlist; acknowledging without routing"
        );
        return StatusCode::OK.into_response();
    }

    info!(event = %payload.event, "Received Zoom event");

    let forwarded_headers = extract_forwarded_headers(&headers, ZOOM_FORWARDED_HEADER_PREFIXES);
    let router = state.zoom_router.clone();
    let event = payload.event.clone();
    tokio::spawn(async move {
        router.route_event(&event, body, forwarded_headers).await;
    });

    StatusCode::OK.into_response()
}

fn header_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderName, HeaderValue};

    #[test]
    fn test_forwards_zoom_signature_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("x-zm-signature"),
            HeaderValue::from_static("v0=abc123"),
        );

        let forwarded = extract_forwarded_headers(&headers, ZOOM_FORWARDED_HEADER_PREFIXES);

        assert_eq!(forwarded.len(), 1);
        assert_eq!(forwarded.get("x-zm-signature").unwrap(), "v0=abc123");
    }

    #[test]
    fn test_forwards_zoom_request_timestamp_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("x-zm-request-timestamp"),
            HeaderValue::from_static("1739923528"),
        );

        let forwarded = extract_forwarded_headers(&headers, ZOOM_FORWARDED_HEADER_PREFIXES);

        assert_eq!(forwarded.len(), 1);
        assert_eq!(
            forwarded.get("x-zm-request-timestamp").unwrap(),
            "1739923528"
        );
    }

    #[test]
    fn test_forwards_content_type_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("content-type"),
            HeaderValue::from_static("application/json"),
        );

        let forwarded = extract_forwarded_headers(&headers, ZOOM_FORWARDED_HEADER_PREFIXES);

        assert_eq!(forwarded.len(), 1);
        assert_eq!(forwarded.get("content-type").unwrap(), "application/json");
    }

    #[test]
    fn test_does_not_forward_arbitrary_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("authorization"),
            HeaderValue::from_static("Bearer token123"),
        );

        let forwarded = extract_forwarded_headers(&headers, ZOOM_FORWARDED_HEADER_PREFIXES);

        assert_eq!(forwarded.len(), 0);
    }
}
