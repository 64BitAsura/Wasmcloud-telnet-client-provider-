//! Telnet capability provider for wasmCloud
//!
//! This provider connects to remote Telnet servers and forwards received messages
//! to wasmCloud components via wRPC. It implements unidirectional communication
//! (receiving only) with automatic reconnection and message size limits.

mod config;
mod provider;
mod telnet;

use provider::TelnetProvider;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    TelnetProvider::run().await?;
    eprintln!("Telnet provider exiting");
    Ok(())
}
