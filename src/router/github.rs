use crate::config::Config;
use crate::crypto::compute_hmac_sha256;
use crate::github::GitHubTriggerConfig;
use crate::n8n::N8nClient;
use axum::http::{HeaderMap, HeaderName, HeaderValue};
use parking_lot::RwLock;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

use super::forward_to_webhook;

/// The GitHub routing engine that manages trigger configurations and forwards events
pub struct GitHubRouter {
    /// Cached GitHub trigger configurations
    triggers: Arc<RwLock<Vec<GitHubTriggerConfig>>>,

    /// n8n API client (shared with other routers)
    n8n_client: Arc<N8nClient>,

    /// Configuration
    config: Arc<Config>,
}

impl GitHubRouter {
    /// Create a new GitHub router instance
    pub fn new(config: Arc<Config>, n8n_client: Arc<N8nClient>) -> Self {
        Self {
            triggers: Arc::new(RwLock::new(Vec::new())),
            n8n_client,
            config,
        }
    }

    /// Start the background task that periodically refreshes GitHub trigger configurations
    pub fn start_refresh_task(self: Arc<Self>) {
        let router = self.clone();
        let refresh_interval = self.config.refresh_interval_secs;

        tokio::spawn(async move {
            // Initial load
            if let Err(e) = router.refresh_triggers().await {
                error!(error = %e, "Failed initial GitHub trigger load");
            }

            // Periodic refresh
            let mut ticker = interval(Duration::from_secs(refresh_interval));
            loop {
                ticker.tick().await;
                if let Err(e) = router.refresh_triggers().await {
                    warn!(error = %e, "Failed to refresh GitHub triggers");
                }
            }
        });
    }

    /// Refresh the GitHub trigger configurations from n8n
    async fn refresh_triggers(&self) -> Result<(), crate::n8n::N8nClientError> {
        info!("Refreshing GitHub trigger configurations from n8n");
        let new_triggers = self.n8n_client.fetch_github_triggers().await?;

        let mut triggers = self.triggers.write();
        *triggers = new_triggers;

        Ok(())
    }

    /// Route a GitHub event to all matching triggers
    ///
    /// The `raw_body` parameter is the exact raw request body from GitHub.
    /// This must be forwarded as-is (not re-serialized) to preserve the
    /// payload integrity.
    ///
    /// For each matching trigger, the middleware re-signs the body with n8n's
    /// webhook secret (from the workflow's `staticData`) and sets the
    /// `X-Hub-Signature-256` header. This is necessary because n8n's GitHub
    /// Trigger node verifies the HMAC-SHA256 signature on every incoming
    /// webhook delivery, and the original signature from GitHub (if any)
    /// was computed with a different secret than what n8n expects.
    ///
    /// If any forward returns a 401 or if a trigger's webhook secret was
    /// missing, the router will immediately refresh its trigger cache from
    /// the n8n API and retry those specific deliveries. This handles the
    /// common case where a workflow was just activated and the periodic
    /// refresh hasn't picked up the new `staticData` yet.
    pub async fn route_event(
        &self,
        event_type: &str,
        owner: Option<&str>,
        repository: Option<&str>,
        raw_body: String,
        headers: HeaderMap,
    ) {
        debug!(
            event_type = %event_type,
            owner = ?owner,
            repository = ?repository,
            "Routing GitHub event"
        );

        // Get matching triggers
        let matching_triggers: Vec<GitHubTriggerConfig> = {
            let triggers = self.triggers.read();
            triggers
                .iter()
                .filter(|t| t.matches_event(event_type, owner, repository))
                .cloned()
                .collect()
        };

        if matching_triggers.is_empty() {
            debug!(
                event_type = %event_type,
                owner = ?owner,
                repository = ?repository,
                "No matching GitHub triggers found for event"
            );
            return;
        }

        info!(
            event_type = %event_type,
            owner = ?owner,
            repository = ?repository,
            matching_count = matching_triggers.len(),
            "Forwarding GitHub event to matching triggers"
        );

        let raw_body = Arc::new(raw_body);

        // Phase 1: Forward to all matching triggers concurrently and collect results.
        // Each result is paired with metadata so we can decide which need a retry.
        let results = self
            .forward_all(&matching_triggers, &raw_body, &headers)
            .await;

        // Phase 2: Identify forwards that failed with 401 or had no webhook secret.
        let retry_urls: HashSet<String> = results
            .into_iter()
            .filter(|r| r.status == Some(401) || !r.had_secret)
            .map(|r| r.webhook_url)
            .collect();

        if retry_urls.is_empty() {
            return;
        }

        warn!(
            retry_count = retry_urls.len(),
            "Got 401 or missing webhook secret; refreshing triggers from n8n API and retrying"
        );

        // Refresh the trigger cache from the n8n API
        if let Err(e) = self.refresh_triggers().await {
            warn!(error = %e, "Failed to refresh triggers for retry — giving up");
            return;
        }

        // Get the refreshed matching triggers
        let fresh_triggers: Vec<GitHubTriggerConfig> = {
            let triggers = self.triggers.read();
            triggers
                .iter()
                .filter(|t| t.matches_event(event_type, owner, repository))
                .cloned()
                .collect()
        };

        // Phase 3: Retry only the specific webhook URLs that failed, using the
        // refreshed trigger configs (which should now have the webhook secret).
        let mut retry_forwards = Vec::new();
        for trigger in &fresh_triggers {
            let signed_headers = Arc::new(Self::build_signed_headers(
                &headers,
                &raw_body,
                trigger.webhook_secret.as_deref(),
            ));

            if trigger.workflow_active && retry_urls.contains(&trigger.webhook_url) {
                let client = self.n8n_client.clone();
                let url = trigger.webhook_url.clone();
                let name = trigger.workflow_name.clone();
                let body = raw_body.clone();
                let hdrs = signed_headers.clone();
                retry_forwards.push(tokio::spawn(async move {
                    forward_to_webhook(&client, &url, &name, "production (retry)", &body, &hdrs)
                        .await
                }));
            }

            if retry_urls.contains(&trigger.test_webhook_url) {
                let client = self.n8n_client.clone();
                let url = trigger.test_webhook_url.clone();
                let name = trigger.workflow_name.clone();
                let body = raw_body.clone();
                let hdrs = signed_headers.clone();
                retry_forwards.push(tokio::spawn(async move {
                    forward_to_webhook(&client, &url, &name, "test (retry)", &body, &hdrs).await
                }));
            }
        }

        for handle in retry_forwards {
            let _ = handle.await;
        }
    }

    /// Forward a GitHub event to all matching triggers concurrently and return
    /// the result of each forward attempt.
    async fn forward_all(
        &self,
        triggers: &[GitHubTriggerConfig],
        raw_body: &Arc<String>,
        headers: &HeaderMap,
    ) -> Vec<ForwardResult> {
        let mut jobs: Vec<(tokio::task::JoinHandle<Option<u16>>, ForwardResult)> = Vec::new();

        for trigger in triggers {
            let had_secret = trigger.webhook_secret.is_some();
            let signed_headers = Arc::new(Self::build_signed_headers(
                headers,
                raw_body,
                trigger.webhook_secret.as_deref(),
            ));

            // Production webhook — only for active workflows
            if trigger.workflow_active {
                let client = self.n8n_client.clone();
                let url = trigger.webhook_url.clone();
                let name = trigger.workflow_name.clone();
                let body = raw_body.clone();
                let hdrs = signed_headers.clone();

                let meta = ForwardResult {
                    webhook_url: trigger.webhook_url.clone(),
                    had_secret,
                    status: None,
                };
                let handle = tokio::spawn(async move {
                    forward_to_webhook(&client, &url, &name, "production", &body, &hdrs).await
                });
                jobs.push((handle, meta));
            } else {
                debug!(
                    workflow_name = %trigger.workflow_name,
                    "Skipping production webhook for inactive GitHub workflow"
                );
            }

            // Test webhook — always forward (for development and testing)
            {
                let client = self.n8n_client.clone();
                let url = trigger.test_webhook_url.clone();
                let name = trigger.workflow_name.clone();
                let body = raw_body.clone();
                let hdrs = signed_headers.clone();

                let meta = ForwardResult {
                    webhook_url: trigger.test_webhook_url.clone(),
                    had_secret,
                    status: None,
                };
                let handle = tokio::spawn(async move {
                    forward_to_webhook(&client, &url, &name, "test", &body, &hdrs).await
                });
                jobs.push((handle, meta));
            }
        }

        // Collect results
        let mut results = Vec::with_capacity(jobs.len());
        for (handle, mut meta) in jobs {
            meta.status = handle.await.ok().flatten();
            results.push(meta);
        }
        results
    }

    /// Build forwarded headers with a re-computed `X-Hub-Signature-256`.
    ///
    /// n8n's GitHub Trigger node verifies the HMAC-SHA256 signature on every
    /// incoming webhook delivery using the secret it generated during workflow
    /// activation. Since the original `X-Hub-Signature-256` from GitHub was
    /// computed with that same secret (sent to GitHub's API), it would normally
    /// be valid. However, in our middleware architecture the event arrives from
    /// GitHub signed with a *different* secret (the one the user configured on
    /// the GitHub → middleware webhook), so we must re-sign with n8n's secret.
    ///
    /// If no secret is available (e.g., the workflow hasn't been activated yet
    /// or staticData wasn't populated), we forward the original headers as-is
    /// and let n8n decide whether to accept or reject.
    fn build_signed_headers(
        original_headers: &HeaderMap,
        body: &str,
        webhook_secret: Option<&str>,
    ) -> HeaderMap {
        let mut headers = original_headers.clone();

        if let Some(secret) = webhook_secret {
            let signature = compute_hmac_sha256(secret, body.as_bytes());

            // Replace or insert the signature header
            headers.insert(
                HeaderName::from_static("x-hub-signature-256"),
                HeaderValue::from_str(&signature).expect("signature is valid ASCII"),
            );

            debug!(
                has_secret = true,
                "Re-signed GitHub webhook payload with n8n's webhook secret"
            );
        } else {
            warn!("No webhook secret available for GitHub trigger; forwarding without re-signing");
        }

        headers
    }

    /// Get the current number of loaded GitHub triggers (for health checks)
    pub fn trigger_count(&self) -> usize {
        self.triggers.read().len()
    }
}

/// Result of a single webhook forward attempt, used to decide whether a retry
/// is needed after refreshing the trigger cache.
struct ForwardResult {
    /// The webhook URL that was called
    webhook_url: String,
    /// Whether the trigger had a webhook secret at the time of the attempt
    had_secret: bool,
    /// HTTP status returned by n8n, or `None` if the connection failed entirely
    status: Option<u16>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::github::GitHubTriggerConfig;
    use wiremock::matchers::{method, path, path_regex};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Build a minimal `Config` that points at the given mock server URL.
    fn test_config(base_url: &str) -> Arc<Config> {
        Arc::new(Config {
            n8n_api_url: base_url.to_string(),
            n8n_api_key: "test-api-key".to_string(),
            listen_addr: "0.0.0.0:3000".to_string(),
            refresh_interval_secs: 600, // long — we don't want the background task interfering
            n8n_endpoint_webhook: "webhook".to_string(),
            n8n_endpoint_webhook_test: "webhook-test".to_string(),
            github_webhook_secret: None,
        })
    }

    /// Build a trigger config that points its webhook URLs at the mock server.
    fn seed_trigger(base_url: &str, secret: Option<&str>) -> GitHubTriggerConfig {
        GitHubTriggerConfig {
            workflow_id: "wf1".to_string(),
            workflow_name: "Test Workflow".to_string(),
            workflow_active: true,
            webhook_url: format!("{}/webhook/wh1/webhook", base_url),
            test_webhook_url: format!("{}/webhook-test/wh1/webhook", base_url),
            events: vec!["push".to_string()],
            owner: "test-owner".to_string(),
            repository: "test-repo".to_string(),
            webhook_secret: secret.map(|s| s.to_string()),
        }
    }

    /// Build a mock n8n API response for `GET /api/v1/workflows` that returns
    /// a single GitHub Trigger workflow with the given webhook secret.
    fn workflows_api_response(secret: &str) -> serde_json::Value {
        serde_json::json!({
            "data": [{
                "id": "wf1",
                "name": "Test Workflow",
                "active": true,
                "nodes": [{
                    "type": "n8n-nodes-base.githubTrigger",
                    "name": "GitHub Trigger",
                    "webhookId": "wh1",
                    "parameters": {
                        "events": ["push"],
                        "owner": "test-owner",
                        "repository": "test-repo"
                    }
                }],
                "staticData": {
                    "node:GitHub Trigger": {
                        "webhookId": 1,
                        "webhookSecret": secret
                    }
                }
            }],
            "nextCursor": null
        })
    }

    // ==================== Happy-path: no retry needed ====================

    /// When all forwards succeed (200) and the trigger has a secret, no
    /// refresh or retry should occur.
    #[tokio::test]
    async fn test_no_retry_when_all_forwards_succeed() {
        let mock_server = MockServer::start().await;
        let base_url = mock_server.uri();

        let config = test_config(&base_url);
        let n8n_client = Arc::new(N8nClient::new(config.clone()));
        let router = GitHubRouter::new(config, n8n_client);

        // Pre-seed with a trigger that already has a valid secret
        {
            let mut triggers = router.triggers.write();
            triggers.push(seed_trigger(&base_url, Some("good-secret")));
        }

        // Webhook forwards should succeed — expect exactly 2 (production + test)
        Mock::given(method("POST"))
            .and(path_regex("/wh1/webhook"))
            .respond_with(ResponseTemplate::new(200))
            .expect(2)
            .mount(&mock_server)
            .await;

        // API should NOT be called — no refresh needed
        Mock::given(method("GET"))
            .and(path("/api/v1/workflows"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&mock_server)
            .await;

        router
            .route_event(
                "push",
                Some("test-owner"),
                Some("test-repo"),
                r#"{"ref":"refs/heads/main"}"#.to_string(),
                HeaderMap::new(),
            )
            .await;

        // wiremock verifies on drop: 2 POSTs, 0 GETs
    }

    // ==================== Retry on 401 ====================

    /// When the cached trigger has a stale secret, the initial forward gets
    /// 401. The router should refresh triggers from the API and retry.
    /// Expects:
    ///   4× POST (2 initial + 2 retry — all return 401 from the mock, but
    ///            the retry results are discarded so no infinite loop)
    ///   1× GET  /api/v1/workflows (refresh)
    #[tokio::test]
    async fn test_retry_on_401_refreshes_triggers_and_retries() {
        let mock_server = MockServer::start().await;
        let base_url = mock_server.uri();

        let config = test_config(&base_url);
        let n8n_client = Arc::new(N8nClient::new(config.clone()));
        let router = GitHubRouter::new(config, n8n_client);

        // Pre-seed with a trigger whose secret is stale
        {
            let mut triggers = router.triggers.write();
            triggers.push(seed_trigger(&base_url, Some("old-secret")));
        }

        // All webhook forwards return 401 (both initial and retry).
        // The retry results are not inspected, so no infinite loop occurs.
        Mock::given(method("POST"))
            .and(path_regex("/wh1/webhook"))
            .respond_with(ResponseTemplate::new(401))
            .expect(4) // 2 initial + 2 retry
            .mount(&mock_server)
            .await;

        // API refresh returns workflow with the new secret
        Mock::given(method("GET"))
            .and(path("/api/v1/workflows"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(workflows_api_response("new-secret")),
            )
            .expect(1)
            .mount(&mock_server)
            .await;

        router
            .route_event(
                "push",
                Some("test-owner"),
                Some("test-repo"),
                r#"{"ref":"refs/heads/main"}"#.to_string(),
                HeaderMap::new(),
            )
            .await;

        // wiremock verifies on drop:
        //   4× POST 401 (2 initial + 2 retry), 1× GET (refresh)
    }

    // ==================== Retry on missing secret ====================

    /// When the cached trigger has no webhook secret at all (e.g. workflow
    /// just activated, staticData not yet synced), the router should refresh
    /// and retry regardless of the forward's HTTP status.
    #[tokio::test]
    async fn test_retry_on_missing_secret_refreshes_and_retries() {
        let mock_server = MockServer::start().await;
        let base_url = mock_server.uri();

        let config = test_config(&base_url);
        let n8n_client = Arc::new(N8nClient::new(config.clone()));
        let router = GitHubRouter::new(config, n8n_client);

        // Pre-seed with a trigger that has NO secret
        {
            let mut triggers = router.triggers.write();
            triggers.push(seed_trigger(&base_url, None));
        }

        // All forwards return 200 — doesn't matter, retry is triggered by !had_secret
        Mock::given(method("POST"))
            .and(path_regex("/wh1/webhook"))
            .respond_with(ResponseTemplate::new(200))
            .expect(4) // 2 initial + 2 retry
            .mount(&mock_server)
            .await;

        // API refresh returns the trigger with a secret
        Mock::given(method("GET"))
            .and(path("/api/v1/workflows"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(workflows_api_response("the-secret")),
            )
            .expect(1)
            .mount(&mock_server)
            .await;

        router
            .route_event(
                "push",
                Some("test-owner"),
                Some("test-repo"),
                r#"{"ref":"refs/heads/main"}"#.to_string(),
                HeaderMap::new(),
            )
            .await;

        // wiremock verifies on drop:
        //   4× POST (2 initial + 2 retry), 1× GET (refresh)
    }
}
