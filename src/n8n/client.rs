use crate::config::Config;
use crate::github::triggers::{GitHubTriggerConfig, parse_github_trigger};
use crate::jira::triggers::{JiraTriggerConfig, parse_jira_trigger};
use crate::n8n::models::{
    ProjectMember, ProjectMembersResponse, UsersResponse, WorkflowOwnerInfo, WorkflowsResponse,
};
use crate::slack::triggers::{SlackTriggerConfig, parse_slack_trigger};
use crate::zoom::triggers::{ZoomTriggerConfig, parse_zoom_trigger};
use axum::http::HeaderMap;
use reqwest::Client;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

/// Client for interacting with the n8n API
pub struct N8nClient {
    client: Client,
    config: Arc<Config>,
}

impl N8nClient {
    pub fn new(config: Arc<Config>) -> Self {
        let client = Client::new();
        Self { client, config }
    }

    /// Fetch all active workflows and extract Slack trigger configurations
    pub async fn fetch_slack_triggers(&self) -> Result<Vec<SlackTriggerConfig>, N8nClientError> {
        let mut triggers = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let response = self.fetch_workflows_page(cursor.as_deref()).await?;

            for workflow in response.data {
                // Look for Slack Trigger nodes in the workflow
                // We include both active and inactive workflows:
                // - Active workflows: forward to both production and test webhooks
                // - Inactive workflows: forward only to test webhooks (for development)
                for node in &workflow.nodes {
                    if let Some(trigger) = parse_slack_trigger(&workflow, node) {
                        info!(
                            workflow_id = %trigger.workflow_id,
                            workflow_name = %trigger.workflow_name,
                            workflow_active = trigger.workflow_active,
                            event_type = %trigger.event_type,
                            watch_whole_workspace = trigger.watch_whole_workspace,
                            channels = ?trigger.channels,
                            "Found Slack trigger"
                        );
                        triggers.push(trigger);
                    }
                }
            }

            // Check if there are more pages
            match response.next_cursor {
                Some(next) if !next.is_empty() => cursor = Some(next),
                _ => break,
            }
        }

        info!(
            count = triggers.len(),
            "Loaded Slack trigger configurations"
        );
        Ok(triggers)
    }

    /// Fetch all workflows and extract Jira trigger configurations
    pub async fn fetch_jira_triggers(&self) -> Result<Vec<JiraTriggerConfig>, N8nClientError> {
        let mut triggers = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let response = self.fetch_workflows_page(cursor.as_deref()).await?;

            for workflow in response.data {
                // Look for Jira Trigger nodes in the workflow
                // We include both active and inactive workflows:
                // - Active workflows: forward to both production and test webhooks
                // - Inactive workflows: forward only to test webhooks (for development)
                for node in &workflow.nodes {
                    if let Some(trigger) = parse_jira_trigger(&workflow, node) {
                        info!(
                            workflow_id = %trigger.workflow_id,
                            workflow_name = %trigger.workflow_name,
                            workflow_active = trigger.workflow_active,
                            events = ?trigger.events,
                            "Found Jira trigger"
                        );
                        triggers.push(trigger);
                    }
                }
            }

            // Check if there are more pages
            match response.next_cursor {
                Some(next) if !next.is_empty() => cursor = Some(next),
                _ => break,
            }
        }

        info!(count = triggers.len(), "Loaded Jira trigger configurations");
        Ok(triggers)
    }

    /// Fetch all workflows and extract Zoom trigger configurations
    pub async fn fetch_zoom_triggers(&self) -> Result<Vec<ZoomTriggerConfig>, N8nClientError> {
        let mut triggers = Vec::new();
        let mut cursor: Option<String> = None;
        let mut project_owner_cache: HashMap<String, Option<String>> = HashMap::new();

        loop {
            let response = self.fetch_workflows_page(cursor.as_deref()).await?;

            for workflow in response.data {
                let owner = self
                    .resolve_workflow_owner(&workflow, &mut project_owner_cache)
                    .await;

                for node in &workflow.nodes {
                    if let Some(trigger) = parse_zoom_trigger(&workflow, node, &owner) {
                        info!(
                            workflow_id = %trigger.workflow_id,
                            workflow_name = %trigger.workflow_name,
                            workflow_active = trigger.workflow_active,
                            events = ?trigger.events,
                            owner_email = ?trigger.owner_email,
                            project_type = %trigger.project_type,
                            "Found Zoom trigger"
                        );
                        triggers.push(trigger);
                    }
                }
            }

            match response.next_cursor {
                Some(next) if !next.is_empty() => cursor = Some(next),
                _ => break,
            }
        }

        info!(count = triggers.len(), "Loaded Zoom trigger configurations");
        Ok(triggers)
    }

    async fn resolve_workflow_owner(
        &self,
        workflow: &crate::n8n::models::Workflow,
        project_owner_cache: &mut HashMap<String, Option<String>>,
    ) -> WorkflowOwnerInfo {
        if let Some((project_id, project_type)) = workflow.owner_project() {
            return self
                .build_workflow_owner_info(project_id, project_type, project_owner_cache)
                .await;
        }

        warn!(
            workflow_id = %workflow.id,
            workflow_name = %workflow.name,
            "Workflow list response missing shared metadata; refetching workflow"
        );

        if let Ok(full) = self.fetch_workflow_by_id(&workflow.id).await
            && let Some((project_id, project_type)) = full.owner_project()
        {
            return self
                .build_workflow_owner_info(project_id, project_type, project_owner_cache)
                .await;
        }

        warn!(
            workflow_id = %workflow.id,
            workflow_name = %workflow.name,
            "Could not resolve workflow owner project; host routing disabled for this trigger"
        );
        WorkflowOwnerInfo::unknown()
    }

    async fn build_workflow_owner_info(
        &self,
        project_id: String,
        project_type: String,
        project_owner_cache: &mut HashMap<String, Option<String>>,
    ) -> WorkflowOwnerInfo {
        let owner_email = if project_type.eq_ignore_ascii_case("team") {
            None
        } else {
            self.resolve_personal_project_owner_email(&project_id, project_owner_cache)
                .await
        };

        WorkflowOwnerInfo {
            project_id,
            project_type,
            owner_email,
        }
    }

    async fn resolve_personal_project_owner_email(
        &self,
        project_id: &str,
        cache: &mut HashMap<String, Option<String>>,
    ) -> Option<String> {
        if let Some(cached) = cache.get(project_id) {
            return cached.clone();
        }

        let email = self.fetch_personal_project_owner_email(project_id).await;
        cache.insert(project_id.to_string(), email.clone());
        email
    }

    async fn fetch_personal_project_owner_email(&self, project_id: &str) -> Option<String> {
        match self.fetch_project_members(project_id).await {
            Ok(members) if !members.is_empty() => {
                if let Some(email) = pick_personal_owner_from_members(&members) {
                    return Some(email);
                }
            }
            Ok(_) => {}
            Err(e) => {
                warn!(
                    project_id = %project_id,
                    error = %e,
                    "Project members API unavailable; falling back to users list"
                );
            }
        }

        match self.fetch_users_for_project(project_id).await {
            Ok(users) if users.len() == 1 => Some(users[0].email.clone()),
            Ok(users) if !users.is_empty() => {
                warn!(
                    project_id = %project_id,
                    user_count = users.len(),
                    "Multiple users on personal project; using first user email"
                );
                Some(users[0].email.clone())
            }
            Ok(_) => None,
            Err(e) => {
                warn!(
                    project_id = %project_id,
                    error = %e,
                    "Failed to resolve personal project owner email"
                );
                None
            }
        }
    }

    async fn fetch_users_for_project(
        &self,
        project_id: &str,
    ) -> Result<Vec<crate::n8n::models::N8nUser>, N8nClientError> {
        let mut users = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let mut url = format!(
                "{}/api/v1/users?projectId={}",
                self.config.n8n_api_url.trim_end_matches('/'),
                project_id
            );
            if let Some(c) = &cursor {
                url.push_str(&format!("&cursor={}", c));
            }

            debug!(url = %url, "Fetching project users via users API");

            let response = self
                .client
                .get(&url)
                .header("X-N8N-API-KEY", &self.config.n8n_api_key)
                .send()
                .await
                .map_err(|e| N8nClientError::RequestFailed(e.to_string()))?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                return Err(N8nClientError::ApiError {
                    status: status.as_u16(),
                    body,
                });
            }

            let page: UsersResponse = response
                .json()
                .await
                .map_err(|e| N8nClientError::ParseError(e.to_string()))?;

            users.extend(page.data);

            match page.next_cursor {
                Some(next) if !next.is_empty() => cursor = Some(next),
                _ => break,
            }
        }

        Ok(users)
    }

    async fn fetch_project_members(
        &self,
        project_id: &str,
    ) -> Result<Vec<crate::n8n::models::ProjectMember>, N8nClientError> {
        let mut members = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let mut url = format!(
                "{}/api/v1/projects/{}/users",
                self.config.n8n_api_url.trim_end_matches('/'),
                project_id
            );
            if let Some(c) = &cursor {
                url.push_str(&format!("?cursor={}", c));
            }

            debug!(url = %url, "Fetching project members from n8n");

            let response = self
                .client
                .get(&url)
                .header("X-N8N-API-KEY", &self.config.n8n_api_key)
                .send()
                .await
                .map_err(|e| N8nClientError::RequestFailed(e.to_string()))?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                return Err(N8nClientError::ApiError {
                    status: status.as_u16(),
                    body,
                });
            }

            let page: ProjectMembersResponse = response
                .json()
                .await
                .map_err(|e| N8nClientError::ParseError(e.to_string()))?;

            members.extend(page.data);

            match page.next_cursor {
                Some(next) if !next.is_empty() => cursor = Some(next),
                _ => break,
            }
        }

        Ok(members)
    }

    /// Fetch all workflows and extract GitHub trigger configurations
    pub async fn fetch_github_triggers(&self) -> Result<Vec<GitHubTriggerConfig>, N8nClientError> {
        let mut triggers = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let response = self.fetch_workflows_page(cursor.as_deref()).await?;

            for workflow in response.data {
                // Look for GitHub Trigger nodes in the workflow
                // We include both active and inactive workflows:
                // - Active workflows: forward to both production and test webhooks
                // - Inactive workflows: forward only to test webhooks (for development)
                for node in &workflow.nodes {
                    if let Some(trigger) = parse_github_trigger(&workflow, node) {
                        info!(
                            workflow_id = %trigger.workflow_id,
                            workflow_name = %trigger.workflow_name,
                            workflow_active = trigger.workflow_active,
                            events = ?trigger.events,
                            owner = %trigger.owner,
                            repository = %trigger.repository,
                            has_webhook_secret = trigger.webhook_secret.is_some(),
                            "Found GitHub trigger"
                        );
                        triggers.push(trigger);
                    }
                }
            }

            // Check if there are more pages
            match response.next_cursor {
                Some(next) if !next.is_empty() => cursor = Some(next),
                _ => break,
            }
        }

        info!(
            count = triggers.len(),
            "Loaded GitHub trigger configurations"
        );
        Ok(triggers)
    }

    /// Fetch a single page of workflows from the n8n API
    async fn fetch_workflows_page(
        &self,
        cursor: Option<&str>,
    ) -> Result<WorkflowsResponse, N8nClientError> {
        let mut url = format!("{}/api/v1/workflows", self.config.n8n_api_url);

        if let Some(c) = cursor {
            url.push_str(&format!("?cursor={}", c));
        }

        debug!(url = %url, "Fetching workflows from n8n");

        let response = self
            .client
            .get(&url)
            .header("X-N8N-API-KEY", &self.config.n8n_api_key)
            .send()
            .await
            .map_err(|e| {
                error!(error = %e, "Failed to connect to n8n API");
                N8nClientError::RequestFailed(e.to_string())
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            error!(status = %status, body = %body, "n8n API returned error");
            return Err(N8nClientError::ApiError {
                status: status.as_u16(),
                body,
            });
        }

        let workflows: WorkflowsResponse = response.json().await.map_err(|e| {
            error!(error = %e, "Failed to parse n8n API response");
            N8nClientError::ParseError(e.to_string())
        })?;

        Ok(workflows)
    }

    /// Forward an event to a specific webhook URL
    ///
    /// The `raw_body` parameter is the exact raw request body from the source
    /// (Slack, Jira, etc.). This is forwarded as-is (not re-serialized) to
    /// preserve the exact bytes for signature/authentication verification by n8n.
    ///
    /// Returns the HTTP status code from n8n's response on success, or an error
    /// if the request couldn't be sent at all (connection failure, DNS, etc.).
    pub async fn forward_event(
        &self,
        webhook_url: &str,
        raw_body: &str,
        headers: &HeaderMap,
    ) -> Result<u16, N8nClientError> {
        debug!(
            webhook_url = %webhook_url,
            forwarded_headers = headers.len(),
            body_len = raw_body.len(),
            "Forwarding event to n8n webhook"
        );

        // Build the request with the raw body (not re-serialized JSON)
        // This preserves the exact bytes for signature verification
        let mut request = self.client.post(webhook_url).body(raw_body.to_string());

        // Forward relevant headers from the original request
        // (e.g. Content-Type, X-Slack-Signature, X-Atlassian-* headers, etc.)
        for (name, value) in headers.iter() {
            let header_name =
                reqwest::header::HeaderName::from_bytes(name.as_str().as_bytes()).ok();
            let header_value = reqwest::header::HeaderValue::from_bytes(value.as_bytes()).ok();

            if let (Some(name), Some(value)) = (header_name, header_value) {
                request = request.header(name, value);
            }
        }

        let response = request.send().await.map_err(|e| {
            warn!(error = %e, webhook_url = %webhook_url, "Failed to forward event");
            N8nClientError::RequestFailed(e.to_string())
        })?;

        let status = response.status();
        let status_code = status.as_u16();

        if !status.is_success() {
            warn!(
                status = %status,
                webhook_url = %webhook_url,
                "Webhook returned non-success status"
            );
        }

        Ok(status_code)
    }

    async fn fetch_workflow_by_id(
        &self,
        workflow_id: &str,
    ) -> Result<crate::n8n::models::Workflow, N8nClientError> {
        let url = format!(
            "{}/api/v1/workflows/{}",
            self.config.n8n_api_url.trim_end_matches('/'),
            workflow_id
        );

        let response = self
            .client
            .get(&url)
            .header("X-N8N-API-KEY", &self.config.n8n_api_key)
            .send()
            .await
            .map_err(|e| N8nClientError::RequestFailed(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(N8nClientError::ApiError {
                status: status.as_u16(),
                body,
            });
        }

        response
            .json()
            .await
            .map_err(|e| N8nClientError::ParseError(e.to_string()))
    }
}

fn pick_personal_owner_from_members(members: &[ProjectMember]) -> Option<String> {
    members
        .iter()
        .find(|m| m.role == "project:personalOwner" || m.role == "project:owner")
        .map(|m| m.email.clone())
        .or_else(|| {
            if members.len() == 1 {
                Some(members[0].email.clone())
            } else {
                None
            }
        })
}

#[derive(Debug, thiserror::Error)]
pub enum N8nClientError {
    #[error("Request failed: {0}")]
    RequestFailed(String),

    #[error("API error (status {status}): {body}")]
    ApiError { status: u16, body: String },

    #[error("Failed to parse response: {0}")]
    ParseError(String),
}
