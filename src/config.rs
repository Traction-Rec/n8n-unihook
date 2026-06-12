use serde::{Deserialize, Deserializer};

/// Configuration for the Slack Unihook router.
/// All values are loaded from environment variables.
#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    /// n8n instance URL (e.g., "http://localhost:5678")
    #[serde(default = "default_n8n_api_url")]
    pub n8n_api_url: String,

    /// n8n API key for authentication
    pub n8n_api_key: String,

    /// Address to bind the HTTP server (default: "0.0.0.0:3000")
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,

    /// How often to refresh the routing table from n8n (in seconds)
    #[serde(default = "default_refresh_interval")]
    pub refresh_interval_secs: u64,

    /// n8n production webhook endpoint path (default: "webhook")
    /// Corresponds to n8n's N8N_ENDPOINT_WEBHOOK env var
    #[serde(default = "default_endpoint_webhook")]
    pub n8n_endpoint_webhook: String,

    /// n8n test webhook endpoint path (default: "webhook-test")
    /// Corresponds to n8n's N8N_ENDPOINT_WEBHOOK_TEST env var
    #[serde(default = "default_endpoint_webhook_test")]
    pub n8n_endpoint_webhook_test: String,

    /// Optional shared secret for verifying inbound GitHub webhook signatures.
    /// When set, the `X-Hub-Signature-256` header on incoming requests to
    /// `/github/events` is verified using HMAC-SHA256 before routing.
    /// When unset, inbound verification is skipped (existing behavior).
    #[serde(default)]
    pub github_webhook_secret: Option<String>,

    /// Path to the SQLite database file used for storing webhook secrets and
    /// trigger metadata. Defaults to `"unihook.db"` in the current working
    /// directory. Set to `":memory:"` for an in-memory database (useful for
    /// tests).
    #[serde(default = "default_database_path")]
    pub database_path: String,

    /// Secret token from the Zoom app Event Subscriptions settings.
    /// Used for URL validation challenges and inbound webhook signature verification.
    pub zoom_webhook_secret: String,

    /// Comma-separated Zoom event types Unihook is allowed to forward.
    /// Events not listed are acknowledged to Zoom but not routed to n8n.
    #[serde(deserialize_with = "deserialize_comma_separated")]
    pub zoom_allowed_events: Vec<String>,
}

fn deserialize_comma_separated<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = String::deserialize(deserializer)?;
    Ok(parse_comma_separated(&raw))
}

fn parse_comma_separated(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

fn default_n8n_api_url() -> String {
    "http://localhost:5678".to_string()
}

fn default_listen_addr() -> String {
    "0.0.0.0:3000".to_string()
}

fn default_refresh_interval() -> u64 {
    60
}

fn default_endpoint_webhook() -> String {
    "webhook".to_string()
}

fn default_endpoint_webhook_test() -> String {
    "webhook-test".to_string()
}

fn default_database_path() -> String {
    "unihook.db".to_string()
}

impl Config {
    /// Load configuration from environment variables.
    /// Environment variables should be prefixed with nothing (e.g., N8N_API_URL).
    pub fn from_env() -> Result<Self, envy::Error> {
        envy::from_env::<Config>()
    }

    /// Returns true if the given Zoom event type is on the platform allowlist.
    pub fn is_zoom_event_allowed(&self, event: &str) -> bool {
        self.zoom_allowed_events.iter().any(|e| e == event)
    }
}

#[cfg(test)]
impl Config {
    pub fn test_default() -> Self {
        Self {
            n8n_api_url: "http://localhost:5678".to_string(),
            n8n_api_key: "test-key".to_string(),
            listen_addr: "0.0.0.0:3000".to_string(),
            refresh_interval_secs: 600,
            n8n_endpoint_webhook: "webhook".to_string(),
            n8n_endpoint_webhook_test: "webhook-test".to_string(),
            github_webhook_secret: None,
            database_path: ":memory:".to_string(),
            zoom_webhook_secret: "test-zoom-secret".to_string(),
            zoom_allowed_events: vec!["meeting.started".to_string()],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_comma_separated_events() {
        assert_eq!(
            parse_comma_separated("meeting.started, meeting.ended,recording.completed"),
            vec![
                "meeting.started".to_string(),
                "meeting.ended".to_string(),
                "recording.completed".to_string(),
            ]
        );
    }

    #[test]
    fn test_parse_comma_separated_empty_items_ignored() {
        assert_eq!(
            parse_comma_separated("a,, b, "),
            vec!["a".to_string(), "b".to_string()]
        );
    }
}
