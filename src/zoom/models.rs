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

/// Extract meeting host email from a Zoom webhook payload.
///
/// Checks known host fields on `payload.object` only (not generic `email` fields
/// used by account/user events).
pub fn extract_host_email(payload: &serde_json::Value) -> Option<String> {
    let object = payload.get("payload")?.get("object")?;

    object
        .get("host_email")
        .or_else(|| object.get("meeting_host_email"))
        .and_then(|v| v.as_str())
        .map(normalize_email)
}

fn normalize_email(email: &str) -> String {
    email.trim().to_string()
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

    #[test]
    fn test_extract_host_email_from_host_email() {
        let payload = json!({
            "payload": {
                "object": {
                    "host_email": "Host@Example.com"
                }
            }
        });
        assert_eq!(
            extract_host_email(&payload).as_deref(),
            Some("Host@Example.com")
        );
    }

    #[test]
    fn test_extract_host_email_from_meeting_host_email() {
        let payload = json!({
            "payload": {
                "object": {
                    "meeting_host_email": " host@example.com "
                }
            }
        });
        assert_eq!(
            extract_host_email(&payload).as_deref(),
            Some("host@example.com")
        );
    }

    #[test]
    fn test_extract_host_email_missing() {
        let payload = json!({
            "payload": {
                "object": {
                    "host_id": "abc"
                }
            }
        });
        assert!(extract_host_email(&payload).is_none());
    }

    #[test]
    fn test_extract_host_email_ignores_generic_email_on_user_events() {
        let payload = json!({
            "payload": {
                "object": {
                    "email": "user@example.com"
                }
            }
        });
        assert!(extract_host_email(&payload).is_none());
    }
}
