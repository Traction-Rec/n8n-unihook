use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
};
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::router::Router;
use crate::slack::{SlackPayload, UrlVerificationResponse};

/// Application state shared across handlers
pub struct AppState {
    pub router: Arc<Router>,
}

/// Handle incoming Slack events
///
/// This endpoint handles:
/// 1. URL verification challenges from Slack
/// 2. Event callbacks that get routed to matching n8n workflows
pub async fn handle_slack_event(
    State(state): State<Arc<AppState>>,
    body: String,
) -> impl IntoResponse {
    // Parse the raw JSON first to keep the original payload for forwarding
    let raw_payload: serde_json::Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(e) => {
            warn!(error = %e, "Failed to parse Slack payload as JSON");
            return (StatusCode::BAD_REQUEST, "Invalid JSON").into_response();
        }
    };

    // Parse into our typed structure
    let payload: SlackPayload = match serde_json::from_value(raw_payload.clone()) {
        Ok(p) => p,
        Err(e) => {
            warn!(error = %e, "Failed to parse Slack payload structure");
            debug!(body = %body, "Raw payload that failed to parse");
            return (StatusCode::BAD_REQUEST, "Invalid Slack payload").into_response();
        }
    };

    match payload {
        SlackPayload::UrlVerification { challenge } => {
            info!("Received URL verification challenge from Slack");
            Json(UrlVerificationResponse { challenge }).into_response()
        }
        SlackPayload::EventCallback(callback) => {
            info!(
                event_type = %callback.event.event_type,
                event_id = %callback.event_id,
                team_id = %callback.team_id,
                "Received Slack event"
            );

            // Route the event asynchronously but respond immediately to Slack
            // Slack requires a response within 3 seconds
            let router = state.router.clone();
            tokio::spawn(async move {
                router.route_event(&callback, &raw_payload).await;
            });

            // Return 200 OK immediately to acknowledge receipt
            StatusCode::OK.into_response()
        }
    }
}

/// Health check endpoint
pub async fn health_check(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let trigger_count = state.router.trigger_count();
    Json(serde_json::json!({
        "status": "healthy",
        "triggers_loaded": trigger_count
    }))
}
