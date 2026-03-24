use rmcp::{ServiceExt, transport::stdio};
use tracing_subscriber::EnvFilter;

use crate::server::ApeServer;

mod server;

#[tokio::main]
async fn main() {
    // Initialize the tracing subscriber with file and stdout logging
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(tracing::Level::DEBUG.into()))
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    tracing::info!("Starting APE MCP server");

    let ape_server = ApeServer::new().unwrap();
    let service = ape_server
        .serve(stdio())
        .await
        .inspect_err(|e| {
            tracing::error!("serving error: {:?}", e);
        })
        .expect("Failed to start MCP server");

    service.waiting().await.expect("MCP server failed");
}
