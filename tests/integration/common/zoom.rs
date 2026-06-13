//! Zoom-specific test payload helpers

use serde_json::{Value, json};

/// Test Zoom webhook secret for signature verification tests.
/// Must match ZOOM_WEBHOOK_SECRET in docker-compose.test.yml and run-integration-tests.sh.
pub const TEST_ZOOM_WEBHOOK_SECRET: &str = "test-zoom-webhook-secret-for-integration-tests";

/// Host email matching the n8n test owner (scripts/run-integration-tests.sh).
pub const TEST_ZOOM_HOST_EMAIL: &str = "test@example.com";

/// Compute a Zoom webhook signature in `v0=<hex>` format.
pub fn compute_zoom_signature(secret: &str, timestamp: &str, body: &str) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let message = format!("v0:{}:{}", timestamp, body);
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key");
    mac.update(message.as_bytes());
    format!("v0={}", hex::encode(mac.finalize().into_bytes()))
}

/// Compute the encrypted token for Zoom URL validation challenges.
pub fn compute_zoom_url_validation_token(secret: &str, plain_token: &str) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key");
    mac.update(plain_token.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// Create a Zoom meeting.started webhook payload
pub fn create_zoom_meeting_started_payload() -> Value {
    json!({
        "event": "meeting.started",
        "event_ts": 1234567890000i64,
        "payload": {
            "account_id": "test-account",
            "object": {
                "id": "123456789",
                "uuid": "test-meeting-uuid",
                "host_id": "host-123",
                "host_email": TEST_ZOOM_HOST_EMAIL,
                "topic": "Test Meeting",
                "type": 2,
                "start_time": "2024-01-01T10:00:00Z",
                "duration": 60,
                "timezone": "UTC"
            }
        }
    })
}

/// Create a Zoom meeting.started payload with a non-matching host email
pub fn create_zoom_meeting_started_payload_wrong_host() -> Value {
    json!({
        "event": "meeting.started",
        "event_ts": 1234567890000i64,
        "payload": {
            "account_id": "test-account",
            "object": {
                "id": "123456789",
                "uuid": "test-meeting-uuid",
                "host_id": "host-456",
                "host_email": "other@example.com",
                "topic": "Test Meeting",
                "type": 2,
                "start_time": "2024-01-01T10:00:00Z",
                "duration": 60,
                "timezone": "UTC"
            }
        }
    })
}

/// Create a Zoom meeting.started payload without a host email field
pub fn create_zoom_meeting_started_payload_no_host() -> Value {
    json!({
        "event": "meeting.started",
        "event_ts": 1234567890000i64,
        "payload": {
            "account_id": "test-account",
            "object": {
                "id": "123456789",
                "uuid": "test-meeting-uuid",
                "host_id": "host-123",
                "topic": "Test Meeting",
                "type": 2,
                "start_time": "2024-01-01T10:00:00Z",
                "duration": 60,
                "timezone": "UTC"
            }
        }
    })
}

/// Create a Zoom recording.completed webhook payload
pub fn create_zoom_recording_completed_payload() -> Value {
    json!({
        "event": "recording.completed",
        "event_ts": 1234567890000i64,
        "payload": {
            "account_id": "test-account",
            "object": {
                "uuid": "test-meeting-uuid",
                "id": 123456789,
                "host_id": "host-123",
                "host_email": TEST_ZOOM_HOST_EMAIL,
                "topic": "Test Meeting"
            }
        }
    })
}

/// Create a Zoom user.updated webhook payload (not on default test allowlist)
pub fn create_zoom_user_updated_payload() -> Value {
    json!({
        "event": "user.updated",
        "event_ts": 1234567890000i64,
        "payload": {
            "account_id": "test-account",
            "object": {
                "id": "user-123",
                "email": "test@example.com"
            }
        }
    })
}

/// Create a Zoom endpoint.url_validation payload
pub fn create_zoom_url_validation_payload(plain_token: &str) -> Value {
    json!({
        "event": "endpoint.url_validation",
        "payload": {
            "plainToken": plain_token
        }
    })
}
