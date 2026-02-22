pub mod github;
pub mod jira;
pub mod slack;

pub use github::GitHubRouter;
pub use jira::JiraRouter;
pub use slack::SlackRouter;

use crate::n8n::N8nClient;
use axum::http::HeaderMap;
use tracing::{debug, warn};

/// Forward an event to a single webhook URL with proper error handling.
/// Errors are logged but do not propagate - other webhooks should still receive the event.
///
/// The `raw_body` is the exact raw request body from the source (Slack, Jira, etc.),
/// forwarded as-is to preserve signature/authentication verification by n8n.
///
/// Returns the HTTP status code if the request was sent successfully, or `None`
/// if the connection failed entirely. Callers can use this to detect 401s and
/// trigger a retry with refreshed credentials.
pub async fn forward_to_webhook(
    client: &N8nClient,
    webhook_url: &str,
    workflow_name: &str,
    webhook_type: &str,
    raw_body: &str,
    headers: &HeaderMap,
) -> Option<u16> {
    match client.forward_event(webhook_url, raw_body, headers).await {
        Ok(status) => {
            if (200..300).contains(&status) {
                debug!(
                    workflow_name = %workflow_name,
                    webhook_url = %webhook_url,
                    webhook_type = %webhook_type,
                    "Successfully forwarded event"
                );
            }
            Some(status)
        }
        Err(e) => {
            // Log the error but don't propagate - other webhooks should still receive the event
            warn!(
                workflow_name = %workflow_name,
                webhook_url = %webhook_url,
                webhook_type = %webhook_type,
                error = %e,
                "Failed to forward event (continuing to other webhooks)"
            );
            None
        }
    }
}
