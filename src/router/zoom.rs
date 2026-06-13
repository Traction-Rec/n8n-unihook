use crate::config::Config;
use crate::db::{Database, ZoomTriggerRow};
use crate::n8n::N8nClient;
use axum::http::HeaderMap;
use std::collections::HashSet;
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

    pub async fn route_event(
        &self,
        event: &str,
        host_email: Option<&str>,
        raw_body: String,
        headers: HeaderMap,
    ) {
        debug!(event = %event, host_email = ?host_email, "Routing Zoom event");

        if host_email.is_none() {
            warn!(
                event = %event,
                "Zoom event has no host email field; only privileged triggers may receive it"
            );
        }

        let all_rows = match self.db.query_zoom_triggers() {
            Ok(rows) => rows,
            Err(e) => {
                error!(error = %e, "Failed to query Zoom triggers from database");
                return;
            }
        };

        let privileged_users = self.config.zoom_privileged_user_emails();
        let privileged_workflow_ids = self.config.zoom_privileged_workflow_ids();

        let matching_triggers: Vec<_> = all_rows
            .iter()
            .filter(|t| zoom_trigger_matches_event(&t.events, event))
            .filter(|t| {
                trigger_should_receive(t, host_email, &privileged_users, &privileged_workflow_ids)
            })
            .collect();

        if matching_triggers.is_empty() {
            debug!(event = %event, "No matching Zoom triggers found for event after host filter");
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

/// Returns true if the trigger should receive the event after host/privileged filtering.
pub(crate) fn trigger_should_receive(
    trigger: &ZoomTriggerRow,
    host_email: Option<&str>,
    privileged_users: &HashSet<String>,
    privileged_workflow_ids: &HashSet<String>,
) -> bool {
    if privileged_workflow_ids.contains(&trigger.workflow_id) {
        return true;
    }

    if let Some(owner) = &trigger.owner_email
        && privileged_users.contains(&owner.to_lowercase())
    {
        return true;
    }

    let Some(host) = host_email else {
        debug!(
            workflow_name = %trigger.workflow_name,
            "Skipping Zoom trigger: event has no host email and trigger is not privileged"
        );
        return false;
    };

    match &trigger.owner_email {
        Some(owner) if host.eq_ignore_ascii_case(owner) => true,
        Some(owner) => {
            debug!(
                workflow_name = %trigger.workflow_name,
                host_email = %host,
                owner_email = %owner,
                "Skipping Zoom trigger: host email does not match workflow owner"
            );
            false
        }
        None => {
            debug!(
                workflow_name = %trigger.workflow_name,
                project_type = %trigger.project_type,
                "Skipping Zoom trigger: team project without workflow-ID allowlist"
            );
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_trigger(
        workflow_id: &str,
        owner_email: Option<&str>,
        project_type: &str,
    ) -> ZoomTriggerRow {
        ZoomTriggerRow {
            webhook_id: "wh1".to_string(),
            workflow_id: workflow_id.to_string(),
            workflow_name: "Test".to_string(),
            workflow_active: true,
            events: vec!["meeting.started".to_string()],
            owner_email: owner_email.map(str::to_string),
            project_id: "proj1".to_string(),
            project_type: project_type.to_string(),
        }
    }

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

    #[test]
    fn test_trigger_should_receive_host_match() {
        let trigger = sample_trigger("wf1", Some("host@example.com"), "personal");
        let users = HashSet::new();
        let workflows = HashSet::new();
        assert!(trigger_should_receive(
            &trigger,
            Some("host@example.com"),
            &users,
            &workflows
        ));
    }

    #[test]
    fn test_trigger_should_receive_host_mismatch() {
        let trigger = sample_trigger("wf1", Some("host@example.com"), "personal");
        let users = HashSet::new();
        let workflows = HashSet::new();
        assert!(!trigger_should_receive(
            &trigger,
            Some("other@example.com"),
            &users,
            &workflows
        ));
    }

    #[test]
    fn test_trigger_should_receive_privileged_user_with_wrong_host() {
        let trigger = sample_trigger("wf1", Some("host@example.com"), "personal");
        let users = HashSet::from(["host@example.com".to_string()]);
        let workflows = HashSet::new();
        assert!(trigger_should_receive(
            &trigger,
            Some("other@example.com"),
            &users,
            &workflows
        ));
    }

    #[test]
    fn test_trigger_should_receive_privileged_user_without_host() {
        let trigger = sample_trigger("wf1", Some("host@example.com"), "personal");
        let users = HashSet::from(["host@example.com".to_string()]);
        let workflows = HashSet::new();
        assert!(trigger_should_receive(&trigger, None, &users, &workflows));
    }

    #[test]
    fn test_trigger_should_receive_missing_host_non_privileged() {
        let trigger = sample_trigger("wf1", Some("host@example.com"), "personal");
        let users = HashSet::new();
        let workflows = HashSet::new();
        assert!(!trigger_should_receive(&trigger, None, &users, &workflows));
    }

    #[test]
    fn test_trigger_should_receive_privileged_workflow_id() {
        let trigger = sample_trigger("wf-admin", None, "team");
        let users = HashSet::new();
        let workflows = HashSet::from(["wf-admin".to_string()]);
        assert!(trigger_should_receive(
            &trigger,
            Some("other@example.com"),
            &users,
            &workflows
        ));
    }

    #[test]
    fn test_trigger_should_receive_team_without_bypass() {
        let trigger = sample_trigger("wf-team", None, "team");
        let users = HashSet::new();
        let workflows = HashSet::new();
        assert!(!trigger_should_receive(
            &trigger,
            Some("host@example.com"),
            &users,
            &workflows
        ));
    }
}
