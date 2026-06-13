# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.5.2] - 2026-06-12

### Added

- Zoom host-based routing: personal-project workflows receive events only when `host_email` matches the workflow owner's n8n email
- `ZOOM_PRIVILEGED_USERS` — comma-separated emails bypassing host routing for personal-project workflows
- `ZOOM_PRIVILEGED_WORKFLOW_IDS` — comma-separated workflow IDs bypassing host routing (team-project admin catch-alls)
- Workflow owner resolution via n8n `shared` metadata and `GET /api/v1/projects/{id}/users`

### Security

- Prevents cross-user leakage of sensitive Zoom payloads (e.g. `recording.completed` share passwords) to unrelated employee workflows

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
