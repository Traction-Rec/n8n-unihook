# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
