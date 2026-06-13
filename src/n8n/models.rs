use serde::Deserialize;

/// Response from n8n GET /api/v1/workflows endpoint
#[derive(Debug, Deserialize)]
pub struct WorkflowsResponse {
    pub data: Vec<Workflow>,
    #[serde(rename = "nextCursor")]
    pub next_cursor: Option<String>,
}

/// Project metadata embedded in workflow sharing info.
#[derive(Debug, Deserialize)]
pub struct SharedProject {
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub project_type: Option<String>,
}

/// Workflow sharing entry from the n8n public API.
#[derive(Debug, Deserialize)]
pub struct SharedWorkflow {
    pub role: String,
    #[serde(rename = "projectId")]
    pub project_id: Option<String>,
    pub project: Option<SharedProject>,
}

/// Response from n8n GET /api/v1/users endpoint
#[derive(Debug, Deserialize)]
pub struct UsersResponse {
    pub data: Vec<N8nUser>,
    #[serde(rename = "nextCursor")]
    pub next_cursor: Option<String>,
}

/// An n8n user from the public API
#[derive(Debug, Deserialize)]
pub struct N8nUser {
    pub email: String,
}

/// Response from n8n GET /api/v1/projects/{id}/users
#[derive(Debug, Deserialize)]
pub struct ProjectMembersResponse {
    pub data: Vec<ProjectMember>,
    #[serde(rename = "nextCursor")]
    pub next_cursor: Option<String>,
}

/// A member of an n8n project.
#[derive(Debug, Deserialize)]
pub struct ProjectMember {
    pub email: String,
    pub role: String,
}

/// Resolved ownership metadata for host-based Zoom routing.
#[derive(Debug, Clone)]
pub struct WorkflowOwnerInfo {
    pub project_id: String,
    pub project_type: String,
    pub owner_email: Option<String>,
}

impl WorkflowOwnerInfo {
    pub fn unknown() -> Self {
        Self {
            project_id: String::new(),
            project_type: String::new(),
            owner_email: None,
        }
    }
}

/// A workflow from the n8n API
#[derive(Debug, Deserialize)]
pub struct Workflow {
    pub id: String,
    pub name: String,
    pub active: bool,
    pub nodes: Vec<WorkflowNode>,

    /// Per-node static data stored by n8n during webhook lifecycle.
    ///
    /// Keys are `"node:<NodeName>"` and values are node-specific objects.
    /// For GitHub Trigger nodes, the value contains `webhookSecret` which
    /// is the HMAC secret n8n generated when registering the webhook with
    /// GitHub. We need this secret to re-sign forwarded payloads so that
    /// n8n's signature verification passes.
    #[serde(rename = "staticData", default)]
    pub static_data: Option<serde_json::Value>,

    /// Project ownership/sharing metadata (requires recent n8n versions).
    #[serde(default)]
    pub shared: Vec<SharedWorkflow>,
}

impl Workflow {
    /// Returns the owning project ID and type from the `workflow:owner` shared entry.
    pub fn owner_project(&self) -> Option<(String, String)> {
        let entry = self.shared.iter().find(|s| s.role == "workflow:owner")?;

        let project_id = entry
            .project_id
            .clone()
            .or_else(|| entry.project.as_ref().and_then(|p| p.id.clone()))?;

        let project_type = entry
            .project
            .as_ref()
            .and_then(|p| p.project_type.clone())
            .unwrap_or_else(|| "personal".to_string());

        Some((project_id, project_type))
    }
}

/// A node within a workflow
#[derive(Debug, Clone, Deserialize)]
pub struct WorkflowNode {
    /// The node type (e.g., "n8n-nodes-base.slackTrigger")
    #[serde(rename = "type")]
    pub node_type: String,

    /// Node name
    pub name: String,

    /// Node parameters containing configuration
    #[serde(default)]
    pub parameters: serde_json::Value,

    /// Webhook ID (for trigger nodes)
    #[serde(rename = "webhookId")]
    pub webhook_id: Option<String>,
}
