use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Configuration for the Telnet provider
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderConfig {
    values: HashMap<String, String>,
}

impl From<&HashMap<String, String>> for ProviderConfig {
    /// Construct configuration struct from the passed config values.
    fn from(values: &HashMap<String, String>) -> ProviderConfig {
        ProviderConfig {
            values: values.clone(),
        }
    }
}

/// Link-specific configuration for Telnet connections
#[derive(Debug, Clone)]
pub struct LinkConfig {
    /// Telnet server host to connect to
    pub telnet_host: String,

    /// Telnet server port to connect to
    pub telnet_port: u16,

    /// Maximum reconnection attempts (0 for infinite)
    pub max_reconnect_attempts: u32,

    /// Initial reconnection delay in milliseconds
    pub initial_reconnect_delay_ms: u64,

    /// Maximum reconnection delay in milliseconds
    pub max_reconnect_delay_ms: u64,

    /// Maximum message size in bytes
    pub max_message_size: usize,
}

impl LinkConfig {
    /// Create from link configuration values
    pub fn from_values(config: &HashMap<String, String>) -> anyhow::Result<Self> {
        let telnet_host = config
            .get("telnet_host")
            .ok_or_else(|| anyhow::anyhow!("Missing required config: telnet_host"))?
            .clone();

        let telnet_port = config
            .get("telnet_port")
            .and_then(|v| v.parse().ok())
            .unwrap_or(23);

        let max_reconnect_attempts = config
            .get("max_reconnect_attempts")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);

        let initial_reconnect_delay_ms = config
            .get("initial_reconnect_delay_ms")
            .and_then(|v| v.parse().ok())
            .unwrap_or(1000);

        let max_reconnect_delay_ms = config
            .get("max_reconnect_delay_ms")
            .and_then(|v| v.parse().ok())
            .unwrap_or(60000);

        let max_message_size = config
            .get("max_message_size")
            .and_then(|v| v.parse().ok())
            .unwrap_or(1024 * 1024);

        Ok(Self {
            telnet_host,
            telnet_port,
            max_reconnect_attempts,
            initial_reconnect_delay_ms,
            max_reconnect_delay_ms,
            max_message_size,
        })
    }

    /// Get the initial reconnection delay as Duration
    pub fn initial_reconnect_delay(&self) -> Duration {
        Duration::from_millis(self.initial_reconnect_delay_ms)
    }

    /// Get the maximum reconnection delay as Duration
    pub fn max_reconnect_delay(&self) -> Duration {
        Duration::from_millis(self.max_reconnect_delay_ms)
    }

    /// Get the full address string
    pub fn address(&self) -> String {
        format!("{}:{}", self.telnet_host, self.telnet_port)
    }
}
