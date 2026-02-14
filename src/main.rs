mod config;
mod n8n;
mod router;
mod routes;
mod slack;

use axum::{Router as AxumRouter, routing::get, routing::post};
use std::sync::Arc;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::config::Config;
use crate::router::Router;
use crate::routes::{AppState, handle_slack_event, health_check};

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "n8n_slack_unihook=info,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Load configuration
    let config = match Config::from_env() {
        Ok(c) => Arc::new(c),
        Err(e) => {
            error!(error = %e, "Failed to load configuration");
            eprintln!("Error: Failed to load configuration: {}", e);
            eprintln!("\nRequired environment variables:");
            eprintln!("  N8N_API_KEY     - Your n8n API key");
            eprintln!("\nOptional environment variables:");
            eprintln!(
                "  N8N_API_URL              - n8n instance URL (default: http://localhost:5678)"
            );
            eprintln!("  LISTEN_ADDR              - Address to bind (default: 0.0.0.0:3000)");
            eprintln!("  REFRESH_INTERVAL_SECS    - Trigger refresh interval (default: 60)");
            eprintln!("  N8N_ENDPOINT_WEBHOOK     - Production webhook path (default: webhook)");
            eprintln!("  N8N_ENDPOINT_WEBHOOK_TEST - Test webhook path (default: webhook-test)");
            std::process::exit(1);
        }
    };

    info!(
        n8n_api_url = %config.n8n_api_url,
        listen_addr = %config.listen_addr,
        refresh_interval_secs = config.refresh_interval_secs,
        "Starting Slack Unihook router"
    );

    // Create the router (event routing engine)
    let event_router = Arc::new(Router::new(config.clone()));

    // Start the background task that refreshes trigger configurations
    event_router.clone().start_refresh_task();

    // Create application state
    let app_state = Arc::new(AppState {
        router: event_router,
    });

    // Build the HTTP router
    let app = AxumRouter::new()
        .route("/slack/events", post(handle_slack_event))
        .route("/health", get(health_check))
        .with_state(app_state);

    // Start the server
    let listener = tokio::net::TcpListener::bind(&config.listen_addr)
        .await
        .expect("Failed to bind to address");

    info!(address = %config.listen_addr, "Server listening");
    info!("Slack webhook URL: http://<your-host>/slack/events");

    axum::serve(listener, app)
        .await
        .expect("Server failed to start");
}
