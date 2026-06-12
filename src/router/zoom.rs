use crate::config::Config;
use crate::db::Database;
use crate::n8n::N8nClient;
use axum::http::HeaderMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

use super::forward_to_webhook;

/// The Zoom routing engine that manages trigger configurations and forwards events.
pub struct ZoomRouter {
    db: Arc<Database>,
    n8n_client: Arc<N8nClient>,
    config: Arc<Config>,
}

impl ZoomRouter {
    pub fn new(config: Arc<Config>, n8n_client: Arc<N8nClient>, db: Arc<Database>) -> Self {
        Self {
            db,
            n8n_client,
            config,
        }
    }

    pub fn start_refresh_task(self: Arc<Self>) {
        let router = self.clone();
        let refresh_interval = self.config.refresh_interval_secs;

        tokio::spawn(async move {
            if let Err(e) = router.refresh_triggers().await {
                error!(error = %e, "Failed initial Zoom trigger load");
            }

            let mut ticker = interval(Duration::from_secs(refresh_interval));
            loop {
                ticker.tick().await;
                if let Err(e) = router.refresh_triggers().await {
                    warn!(error = %e, "Failed to refresh Zoom triggers");
                }
            }
        });
    }

    async fn refresh_triggers(&self) -> Result<(), crate::n8n::N8nClientError> {
        info!("Refreshing Zoom trigger configurations from n8n");
        let new_triggers = self.n8n_client.fetch_zoom_triggers().await?;

        if let Err(e) = self.db.sync_zoom_triggers(&new_triggers) {
            warn!(error = %e, "Failed to sync Zoom triggers to database");
        }

        Ok(())
    }

    fn build_webhook_url(&self, webhook_id: &str) -> String {
        let base = self.config.n8n_api_url.trim_end_matches('/');
        format!(
            "{}/{}/{}/webhook",
            base, self.config.n8n_endpoint_webhook, webhook_id
        )
    }

    fn build_test_webhook_url(&self, webhook_id: &str) -> String {
        let base = self.config.n8n_api_url.trim_end_matches('/');
        format!(
            "{}/{}/{}/webhook",
            base, self.config.n8n_endpoint_webhook_test, webhook_id
        )
    }

    pub async fn route_event(&self, event: &str, raw_body: String, headers: HeaderMap) {
        debug!(event = %event, "Routing Zoom event");

        let all_rows = match self.db.query_zoom_triggers() {
            Ok(rows) => rows,
            Err(e) => {
                error!(error = %e, "Failed to query Zoom triggers from database");
                return;
            }
        };

        let matching_triggers: Vec<_> = all_rows
            .iter()
            .filter(|t| zoom_trigger_matches_event(&t.events, event))
            .collect();

        if matching_triggers.is_empty() {
            debug!(event = %event, "No matching Zoom triggers found for event");
            return;
        }

        info!(
            event = %event,
            matching_count = matching_triggers.len(),
            "Forwarding Zoom event to matching triggers"
        );

        let headers = Arc::new(headers);
        let raw_body = Arc::new(raw_body);
        let mut forwards = Vec::new();

        for trigger in &matching_triggers {
            let client = self.n8n_client.clone();
            let workflow_name = trigger.workflow_name.clone();
            let prod_url = self.build_webhook_url(&trigger.webhook_id);
            let test_url = self.build_test_webhook_url(&trigger.webhook_id);

            if trigger.workflow_active {
                let prod_client = client.clone();
                let prod_name = workflow_name.clone();
                let prod_body = raw_body.clone();
                let prod_headers = headers.clone();
                forwards.push(tokio::spawn(async move {
                    forward_to_webhook(
                        &prod_client,
                        &prod_url,
                        &prod_name,
                        "production",
                        &prod_body,
                        &prod_headers,
                    )
                    .await
                }));
            } else {
                debug!(
                    workflow_name = %workflow_name,
                    "Skipping production webhook for inactive Zoom workflow"
                );
            }

            let test_client = client.clone();
            let test_name = workflow_name.clone();
            let test_body = raw_body.clone();
            let test_headers = headers.clone();
            forwards.push(tokio::spawn(async move {
                forward_to_webhook(
                    &test_client,
                    &test_url,
                    &test_name,
                    "test",
                    &test_body,
                    &test_headers,
                )
                .await
            }));
        }

        for handle in forwards {
            let _ = handle.await;
        }
    }

    pub fn trigger_count(&self) -> usize {
        self.db.count_zoom_triggers().unwrap_or(0)
    }
}

/// Returns true if any trigger row matches the given Zoom event (including wildcard).
fn zoom_trigger_matches_event(events: &[String], event: &str) -> bool {
    events.iter().any(|e| e == "*" || e == event)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zoom_trigger_matches_event_exact() {
        assert!(zoom_trigger_matches_event(
            &["meeting.started".to_string()],
            "meeting.started"
        ));
    }

    #[test]
    fn test_zoom_trigger_matches_event_wildcard() {
        assert!(zoom_trigger_matches_event(
            &["*".to_string()],
            "recording.completed"
        ));
    }

    #[test]
    fn test_zoom_trigger_does_not_match() {
        assert!(!zoom_trigger_matches_event(
            &["meeting.started".to_string()],
            "recording.completed"
        ));
    }
}
