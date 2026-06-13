# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.1] - 2026-06-12

### Fixed

- Accept `CUSTOM.zoomTrigger` node type for custom-extension Zoom trigger installs

## [0.5.0] - 2026-06-12

### Added

- Zoom webhook routing with signature verification, URL validation, and event allowlist gating
- `/zoom/events` endpoint and `ZoomRouter` with wildcard event matching
- `ZOOM_WEBHOOK_SECRET` and `ZOOM_ALLOWED_EVENTS` configuration
- Zoom trigger discovery from n8n workflows
- Integration tests and CI support for the Zoom trigger node

## [0.4.2] - 2026-05-12

### Fixed

- Deduplicate Slack, GitHub, and Jira triggers by `webhook_id` before syncing to SQLite so duplicate n8n webhook IDs no longer abort the transaction and leave routing with an empty or stale trigger cache
- Integration test API key scopes to match n8n's allowed scopes for the instance owner role
- Clippy warnings and incorrect dedupe sort keys in trigger deduplication logic

## [0.4.1] - 2026-02-23

### Fixed

- Trigger immediate trigger sync after webhook registration to avoid a routing gap before the next periodic refresh

## [0.4.0] - 2026-02-22

### Added

- Native provider API interception endpoints for GitHub (`/repos/:owner/:repo/hooks`, `/user`) and Jira (`/rest/webhooks/1.0/webhook`, `/rest/api/2/myself`)
- SQLite-backed trigger storage and webhook secret capture via rusqlite
- 401 retry logic that refreshes triggers from the n8n API and retries delivery with fresh secrets
- Integration test for the full secret-capture-to-resign flow
- ADR 003 documenting provider API interception and DB-backed triggers

### Changed

- Renamed package from `n8n-slack-unihook` to `n8n-unihook`
- Replaced external nginx mock APIs with Unihook-native provider endpoints in integration tests
- Migrated trigger sync results from in-memory storage to SQLite tables (`github_triggers`, `jira_triggers`, `slack_triggers`)

### Removed

- Dead code from the DB migration: unused `webhook_url`/`test_webhook_url` fields, `matches_event()` methods, `WebhookEndpoints` struct, and redundant trigger row fields

## [0.3.1] - 2026-02-21

### Fixed

- Retry GitHub webhook delivery on HTTP 401 or missing webhook secret by refreshing the trigger cache from the n8n API and retrying failed deliveries

## [0.3.0] - 2026-02-21

### Added

- GitHub webhook routing via `/github/events` with event-type matching and wildcard triggers
- Per-workflow GitHub payload re-signing using captured webhook secrets
- Inbound HMAC-SHA256 signature verification for GitHub via optional `GITHUB_WEBHOOK_SECRET`
- Shared HMAC utilities in `src/crypto.rs`
- ADR 001 (GitHub payload re-signing) and ADR 002 (inbound verification)
- Comprehensive GitHub integration tests

### Changed

- Jira inbound handling now forwards query parameters to n8n webhook URLs instead of HMAC verification, enabling n8n `authenticateWebhook` / `httpQueryAuth` credential validation

### Fixed

- Integration test cleanup retries `get_workflows` on transient n8n API failures

## [0.2.0] - 2026-02-17

### Added

- Jira webhook event routing via `/jira/events`
- `JiraRouter` with background trigger refresh and event matching by `webhookEvent` (exact match and wildcard)
- Jira Trigger node configuration parsing (`n8n-nodes-base.jiraTrigger`)
- Mock Jira API server for integration tests

### Changed

- Refactored Slack-specific routing into dedicated modules (`router/slack.rs`, `routes/slack.rs`)

## [0.1.2] - 2026-02-14

### Fixed

- Forward raw request body to n8n webhooks without re-serialization, preserving Slack signature bytes
- Slack credential field name (`signingSecret` → `signatureSecret`)

### Added

- Integration test for Slack signature verification
- Default signing of test Slack events in integration tests

## [0.1.1] - 2026-02-14

### Added

- Forward `X-Slack-*` headers and `Content-Type` from incoming Slack requests to n8n webhook endpoints

## [0.1.0] - 2026-02-14

### Added

- Initial release of Slack Unihook router
- Dynamic discovery of n8n Slack trigger configurations via API
- Smart routing based on event type and channel matching
- Support for workspace-wide triggers
- Health check endpoint
- Docker and Docker Compose support
- Configurable refresh interval for trigger discovery
- Structured logging with tracing

### Event Types Supported

- Messages (`message`)
- App mentions (`app_mention`)
- Reactions (`reaction_added`)
- File sharing (`file_shared`, `file_public`)
- Channel creation (`channel_created`)
- User joining (`team_join`)
- Any event (catch-all)

[Unreleased]: https://github.com/Traction-Rec/n8n-unihook/compare/v0.5.1...HEAD
[0.5.1]: https://github.com/Traction-Rec/n8n-unihook/releases/tag/v0.5.1
[0.5.0]: https://github.com/Traction-Rec/n8n-unihook/releases/tag/v0.5.0
[0.4.2]: https://github.com/Traction-Rec/n8n-unihook/releases/tag/v0.4.2
[0.4.1]: https://github.com/Traction-Rec/n8n-unihook/releases/tag/v0.4.1
[0.4.0]: https://github.com/Traction-Rec/n8n-unihook/releases/tag/v0.4.0
[0.3.1]: https://github.com/Traction-Rec/n8n-unihook/releases/tag/v0.3.1
[0.3.0]: https://github.com/Traction-Rec/n8n-unihook/releases/tag/v0.3.0
[0.2.0]: https://github.com/Traction-Rec/n8n-unihook/releases/tag/v0.2.0
[0.1.2]: https://github.com/Traction-Rec/n8n-unihook/releases/tag/v0.1.2
[0.1.1]: https://github.com/Traction-Rec/n8n-unihook/releases/tag/v0.1.1
[0.1.0]: https://github.com/Traction-Rec/n8n-unihook/releases/tag/v0.1.0
