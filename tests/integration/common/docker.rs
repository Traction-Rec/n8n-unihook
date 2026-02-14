//! Docker environment management for integration tests

use std::process::Command;
use std::time::Duration;

/// Configuration for the test Docker environment
pub struct DockerConfig {
    pub compose_file: String,
    pub project_name: String,
}

impl Default for DockerConfig {
    fn default() -> Self {
        Self {
            compose_file: "docker-compose.test.yml".to_string(),
            project_name: "slack-unihook-test".to_string(),
        }
    }
}

/// Start the Docker test environment
pub fn start_docker_env(config: &DockerConfig) -> Result<(), DockerError> {
    println!("Starting Docker test environment...");

    let output = Command::new("docker")
        .args([
            "compose",
            "-f",
            &config.compose_file,
            "-p",
            &config.project_name,
            "up",
            "-d",
            "--build",
            "--wait",
        ])
        .output()
        .map_err(|e| DockerError::CommandFailed(format!("Failed to run docker compose: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DockerError::StartFailed(stderr.to_string()));
    }

    println!("Docker environment started successfully");
    Ok(())
}

/// Stop the Docker test environment
pub fn stop_docker_env(config: &DockerConfig) -> Result<(), DockerError> {
    println!("Stopping Docker test environment...");

    let output = Command::new("docker")
        .args([
            "compose",
            "-f",
            &config.compose_file,
            "-p",
            &config.project_name,
            "down",
            "-v", // Remove volumes to ensure clean state
        ])
        .output()
        .map_err(|e| DockerError::CommandFailed(format!("Failed to run docker compose: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DockerError::StopFailed(stderr.to_string()));
    }

    println!("Docker environment stopped successfully");
    Ok(())
}

/// Wait for services to be healthy
pub async fn wait_for_services(
    n8n_url: &str,
    unihook_url: &str,
    timeout: Duration,
) -> Result<(), DockerError> {
    let client = reqwest::Client::new();
    let start = std::time::Instant::now();

    println!("Waiting for services to be healthy...");

    loop {
        if start.elapsed() > timeout {
            return Err(DockerError::Timeout(
                "Services did not become healthy in time".to_string(),
            ));
        }

        // Check n8n health
        let n8n_healthy = client
            .get(format!("{}/healthz", n8n_url))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false);

        // Check slack-unihook health
        let unihook_healthy = client
            .get(format!("{}/health", unihook_url))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false);

        if n8n_healthy && unihook_healthy {
            println!("All services are healthy!");
            return Ok(());
        }

        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

/// Check if services are already running
pub async fn services_running(n8n_url: &str, unihook_url: &str) -> bool {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();

    let n8n_running = client
        .get(format!("{}/healthz", n8n_url))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false);

    let unihook_running = client
        .get(format!("{}/health", unihook_url))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false);

    n8n_running && unihook_running
}

#[derive(Debug)]
pub enum DockerError {
    CommandFailed(String),
    StartFailed(String),
    StopFailed(String),
    Timeout(String),
}

impl std::fmt::Display for DockerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DockerError::CommandFailed(msg) => write!(f, "Docker command failed: {}", msg),
            DockerError::StartFailed(msg) => {
                write!(f, "Failed to start Docker environment: {}", msg)
            }
            DockerError::StopFailed(msg) => write!(f, "Failed to stop Docker environment: {}", msg),
            DockerError::Timeout(msg) => write!(f, "Timeout: {}", msg),
        }
    }
}

impl std::error::Error for DockerError {}
