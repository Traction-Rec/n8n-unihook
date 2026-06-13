//! Integration tests for Zoom event routing functionality

use crate::common::{
    TEST_ZOOM_WEBHOOK_SECRET, TestEnvironment, UNIHOOK_URL, compute_zoom_signature,
    compute_zoom_url_validation_token, create_zoom_meeting_started_payload,
    create_zoom_meeting_started_payload_no_host, create_zoom_meeting_started_payload_wrong_host,
    create_zoom_recording_completed_payload, create_zoom_url_validation_payload,
    create_zoom_user_updated_payload, get_execution_count, load_workflow, wait_for_execution,
};
use std::time::Duration;

#[tokio::test]
async fn test_zoom_url_validation_returns_encrypted_token() {
    let env = TestEnvironment::new(false)
        .await
        .expect("Failed to create test environment");

    let plain_token = "test-plain-token-12345";
    let payload = create_zoom_url_validation_payload(plain_token);
    let body = serde_json::to_string(&payload).unwrap();
    let timestamp = "1739923528";
    let signature = compute_zoom_signature(TEST_ZOOM_WEBHOOK_SECRET, timestamp, &body);

    let response = env
        .http_client
        .post(format!("{}/zoom/events", UNIHOOK_URL))
        .header("content-type", "application/json")
        .header("x-zm-signature", signature)
        .header("x-zm-request-timestamp", timestamp)
        .body(body)
        .send()
        .await
        .expect("Failed to send URL validation request");

    assert!(response.status().is_success());

    let json: serde_json::Value = response.json().await.expect("Failed to parse response");
    assert_eq!(json["plainToken"], plain_token);
    assert_eq!(
        json["encryptedToken"],
        compute_zoom_url_validation_token(TEST_ZOOM_WEBHOOK_SECRET, plain_token)
    );
}

#[tokio::test]
async fn test_zoom_invalid_signature_rejected() {
    let env = TestEnvironment::new(false)
        .await
        .expect("Failed to create test environment");

    let workflow = load_workflow("zoom_meeting_started_trigger");
    let created = env
        .setup_workflow(&workflow)
        .await
        .expect("Failed to setup workflow");

    let initial_count = get_execution_count(&env, &created.id).await;
    let payload = create_zoom_meeting_started_payload();
    let body = serde_json::to_string(&payload).unwrap();

    let response = env
        .http_client
        .post(format!("{}/zoom/events", UNIHOOK_URL))
        .header("content-type", "application/json")
        .header("x-zm-signature", "v0=invalid")
        .header("x-zm-request-timestamp", "1739923528")
        .body(body)
        .send()
        .await
        .expect("Failed to send event");

    assert_eq!(response.status(), 401);

    tokio::time::sleep(Duration::from_secs(2)).await;
    let final_count = get_execution_count(&env, &created.id).await;
    assert_eq!(final_count, initial_count);

    env.cleanup_workflow(&created.id)
        .await
        .expect("Failed to cleanup workflow");
}

#[tokio::test]
async fn test_zoom_meeting_started_triggers_workflow_execution() {
    let env = TestEnvironment::new(false)
        .await
        .expect("Failed to create test environment");

    let workflow = load_workflow("zoom_meeting_started_trigger");
    let created = env
        .setup_workflow(&workflow)
        .await
        .expect("Failed to setup workflow");

    let initial_count = get_execution_count(&env, &created.id).await;
    let payload = create_zoom_meeting_started_payload();
    let response = env
        .send_zoom_event(&payload)
        .await
        .expect("Failed to send event");

    assert!(response.status().is_success());

    let execution_occurred = wait_for_execution(&env, &created.id, initial_count + 1).await;
    assert!(
        execution_occurred,
        "Expected Zoom meeting.started workflow execution to be triggered"
    );

    env.cleanup_workflow(&created.id)
        .await
        .expect("Failed to cleanup workflow");
}

#[tokio::test]
async fn test_zoom_non_matching_event_does_not_trigger() {
    let env = TestEnvironment::new(false)
        .await
        .expect("Failed to create test environment");

    let workflow = load_workflow("zoom_meeting_started_trigger");
    let created = env
        .setup_workflow(&workflow)
        .await
        .expect("Failed to setup workflow");

    let initial_count = get_execution_count(&env, &created.id).await;
    let payload = create_zoom_recording_completed_payload();
    let response = env
        .send_zoom_event(&payload)
        .await
        .expect("Failed to send event");

    assert!(response.status().is_success());

    tokio::time::sleep(Duration::from_secs(2)).await;
    let final_count = get_execution_count(&env, &created.id).await;
    assert_eq!(
        final_count, initial_count,
        "Expected no execution for non-matching event type"
    );

    env.cleanup_workflow(&created.id)
        .await
        .expect("Failed to cleanup workflow");
}

#[tokio::test]
async fn test_zoom_wildcard_trigger_receives_event() {
    let env = TestEnvironment::new(false)
        .await
        .expect("Failed to create test environment");

    let workflow = load_workflow("zoom_wildcard_trigger");
    let created = env
        .setup_workflow(&workflow)
        .await
        .expect("Failed to setup workflow");

    let initial_count = get_execution_count(&env, &created.id).await;
    let payload = create_zoom_meeting_started_payload();
    let response = env
        .send_zoom_event(&payload)
        .await
        .expect("Failed to send event");

    assert!(response.status().is_success());

    let execution_occurred = wait_for_execution(&env, &created.id, initial_count + 1).await;
    assert!(
        execution_occurred,
        "Expected wildcard Zoom workflow to execute on allowlisted event"
    );

    env.cleanup_workflow(&created.id)
        .await
        .expect("Failed to cleanup workflow");
}

#[tokio::test]
async fn test_zoom_disallowed_event_not_forwarded() {
    let env = TestEnvironment::new(false)
        .await
        .expect("Failed to create test environment");

    let workflow = load_workflow("zoom_wildcard_trigger");
    let created = env
        .setup_workflow(&workflow)
        .await
        .expect("Failed to setup workflow");

    let initial_count = get_execution_count(&env, &created.id).await;
    let payload = create_zoom_user_updated_payload();
    let response = env
        .send_zoom_event(&payload)
        .await
        .expect("Failed to send event");

    assert!(response.status().is_success());

    tokio::time::sleep(Duration::from_secs(2)).await;
    let final_count = get_execution_count(&env, &created.id).await;
    assert_eq!(
        final_count, initial_count,
        "Expected disallowed event not to trigger workflow even with wildcard"
    );

    env.cleanup_workflow(&created.id)
        .await
        .expect("Failed to cleanup workflow");
}

#[tokio::test]
async fn test_zoom_host_mismatch_does_not_trigger() {
    let env = TestEnvironment::new(false)
        .await
        .expect("Failed to create test environment");

    let workflow = load_workflow("zoom_meeting_started_trigger");
    let created = env
        .setup_workflow(&workflow)
        .await
        .expect("Failed to setup workflow");

    let initial_count = get_execution_count(&env, &created.id).await;
    let payload = create_zoom_meeting_started_payload_wrong_host();
    let response = env
        .send_zoom_event(&payload)
        .await
        .expect("Failed to send event");

    assert!(response.status().is_success());

    tokio::time::sleep(Duration::from_secs(2)).await;
    let final_count = get_execution_count(&env, &created.id).await;
    assert_eq!(
        final_count, initial_count,
        "Expected no execution when host email does not match workflow owner"
    );

    env.cleanup_workflow(&created.id)
        .await
        .expect("Failed to cleanup workflow");
}

#[tokio::test]
async fn test_zoom_missing_host_does_not_trigger() {
    let env = TestEnvironment::new(false)
        .await
        .expect("Failed to create test environment");

    let workflow = load_workflow("zoom_meeting_started_trigger");
    let created = env
        .setup_workflow(&workflow)
        .await
        .expect("Failed to setup workflow");

    let initial_count = get_execution_count(&env, &created.id).await;
    let payload = create_zoom_meeting_started_payload_no_host();
    let response = env
        .send_zoom_event(&payload)
        .await
        .expect("Failed to send event");

    assert!(response.status().is_success());

    tokio::time::sleep(Duration::from_secs(2)).await;
    let final_count = get_execution_count(&env, &created.id).await;
    assert_eq!(
        final_count, initial_count,
        "Expected no execution when event has no host email and owner is not privileged"
    );

    env.cleanup_workflow(&created.id)
        .await
        .expect("Failed to cleanup workflow");
}
