//! Integration tests for slack-unihook
//!
//! These tests require a running Docker environment with n8n and slack-unihook.
//! Run with: cargo test --test integration
//!
//! Or use the test script: ./scripts/run-integration-tests.sh

mod common;
mod test_event_routing;
