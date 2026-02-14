use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Context as _;
use tokio::sync::RwLock;
use tracing::{error, info, warn};
use wasmcloud_provider_sdk::initialize_observability;
use wasmcloud_provider_sdk::{
    run_provider, LinkConfig as SdkLinkConfig, LinkDeleteInfo, Provider, ProviderInitConfig,
};

use crate::config::{LinkConfig, ProviderConfig};
use crate::telnet::TelnetClient;

pub(crate) mod bindings {
    wit_bindgen_wrpc::generate!({
        with: {
            "wasmcloud:messaging/types@0.2.0": generate,
            "wasmcloud:messaging/handler@0.2.0": generate,
        }
    });
}

// Import the standard messaging interfaces from WIT
use bindings::wasmcloud::messaging::handler;
use bindings::wasmcloud::messaging::types;

/// State for a single Telnet connection
struct ConnectionState {
    /// Configuration for this connection
    _config: LinkConfig,
    /// Handle to the Telnet task
    _task_handle: tokio::task::JoinHandle<()>,
}

/// Telnet provider implementation
#[derive(Default, Clone)]
pub struct TelnetProvider {
    config: Arc<RwLock<ProviderConfig>>,
    /// All components linked to this provider (target) and their connections
    connections: Arc<RwLock<HashMap<String, ConnectionState>>>,
}

impl TelnetProvider {
    fn name() -> &'static str {
        "telnet-provider"
    }

    /// Execute the provider
    pub async fn run() -> anyhow::Result<()> {
        initialize_observability!(
            Self::name(),
            std::env::var_os("PROVIDER_TELNET_FLAMEGRAPH_PATH")
        );

        let provider = Self::default();
        let shutdown = run_provider(provider.clone(), Self::name())
            .await
            .context("failed to run provider")?;

        // For this unidirectional provider, we don't export any functions
        // Just await shutdown
        shutdown.await;
        Ok(())
    }
}

/// Implement the Provider trait for wasmCloud integration
impl Provider for TelnetProvider {
    /// Initialize the provider
    async fn init(&self, config: impl ProviderInitConfig) -> anyhow::Result<()> {
        let provider_id = config.get_provider_id();
        let initial_config = config.get_config();
        info!(
            provider_id,
            ?initial_config,
            "initializing Telnet provider"
        );

        // Save configuration to provider state
        *self.config.write().await = ProviderConfig::from(initial_config);

        Ok(())
    }

    /// Handle incoming link from a component (component links TO this provider)
    /// This is where we start the Telnet client
    async fn receive_link_config_as_target(
        &self,
        SdkLinkConfig {
            source_id, config, ..
        }: SdkLinkConfig<'_>,
    ) -> anyhow::Result<()> {
        info!("Received link configuration from component: {}", source_id);

        // Parse link configuration
        let link_config = LinkConfig::from_values(config)?;

        info!(
            "Starting Telnet client for {}:{}",
            link_config.telnet_host, link_config.telnet_port
        );

        // Clone what we need for the task
        let config_clone = link_config.clone();
        let source_id_clone = source_id.to_string();

        // Spawn Telnet client task
        let task_handle = tokio::spawn(async move {
            let telnet_client = TelnetClient::new(config_clone.clone());

            // Create message handler that forwards to the component via wRPC
            // using the standard wasmcloud:messaging interface
            let address = config_clone.address();
            let result = telnet_client
                .run(move |data| {
                    // Convert Telnet message to a standard broker-message
                    let message = create_broker_message(data, &address);

                    // Spawn a task to send message to component
                    let source = source_id_clone.clone();
                    tokio::spawn(async move {
                        if let Err(e) = send_message_to_component(&source, message).await {
                            error!("Failed to send message to component {}: {}", source, e);
                        }
                    });

                    Ok(())
                })
                .await;

            if let Err(e) = result {
                error!("Telnet client error: {}", e);
            }
        });

        // Store connection state
        self.connections.write().await.insert(
            source_id.to_string(),
            ConnectionState {
                _config: link_config,
                _task_handle: task_handle,
            },
        );

        info!(
            "Telnet connection established for component: {}",
            source_id
        );
        Ok(())
    }

    /// Handle link deletion
    async fn delete_link_as_target(&self, link: impl LinkDeleteInfo) -> anyhow::Result<()> {
        let source_id = link.get_source_id();
        info!("Deleting link with component: {}", source_id);

        // Remove connection state (task will be cancelled)
        if let Some(state) = self.connections.write().await.remove(source_id) {
            info!("Telnet connection closed for component: {}", source_id);
            state._task_handle.abort();
        } else {
            warn!("No connection found for component: {}", source_id);
        }

        Ok(())
    }

    /// Handle provider shutdown
    async fn shutdown(&self) -> anyhow::Result<()> {
        info!("Shutting down Telnet provider");

        // Clean up all connections
        let mut connections = self.connections.write().await;
        for (source_id, state) in connections.drain() {
            info!("Closing Telnet connection for component: {}", source_id);
            state._task_handle.abort();
        }

        info!("Telnet provider shutdown complete");
        Ok(())
    }
}

/// Create a broker-message from raw Telnet data
///
/// The subject is set to "telnet.<host>:<port>" so the component knows
/// which Telnet connection the message originated from.
/// The body contains the raw bytes of the Telnet message.
fn create_broker_message(data: Vec<u8>, telnet_address: &str) -> types::BrokerMessage {
    types::BrokerMessage {
        subject: format!("telnet.{}", telnet_address),
        body: data.into(),
        reply_to: None,
    }
}

/// Send message to component via wRPC using the standard messaging handler
async fn send_message_to_component(
    component_id: &str,
    message: types::BrokerMessage,
) -> anyhow::Result<()> {
    let client = wasmcloud_provider_sdk::get_connection()
        .get_wrpc_client(component_id)
        .await
        .context("failed to get wrpc client")?;

    match handler::handle_message(&client, None, &message).await {
        Ok(Ok(_)) => {
            info!("Message successfully sent to component {}", component_id);
            Ok(())
        }
        Ok(Err(e)) => {
            error!("Component {} returned error: {}", component_id, e);
            anyhow::bail!("Component error: {}", e)
        }
        Err(e) => {
            error!("Failed to call component {}: {}", component_id, e);
            Err(e)
        }
    }
}
