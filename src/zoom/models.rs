use serde::{Deserialize, Serialize};

/// Top-level Zoom webhook payload (subset of fields needed for routing).
#[derive(Debug, Clone, Deserialize)]
pub struct ZoomWebhookPayload {
    pub event: String,
    #[serde(default)]
    pub payload: serde_json::Value,
}

/// Response body for Zoom `endpoint.url_validation` challenges.
#[derive(Debug, Serialize)]
pub struct UrlValidationResponse {
    #[serde(rename = "plainToken")]
    pub plain_token: String,
    #[serde(rename = "encryptedToken")]
    pub encrypted_token: String,
}

/// Extract `plainToken` from a URL validation payload.
pub fn extract_plain_token(payload: &serde_json::Value) -> Option<String> {
    payload
        .get("plainToken")
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_extract_plain_token() {
        let payload = json!({ "plainToken": "abc123" });
        assert_eq!(extract_plain_token(&payload).as_deref(), Some("abc123"));
    }

    #[test]
    fn test_extract_plain_token_missing() {
        assert!(extract_plain_token(&json!({})).is_none());
    }
}
