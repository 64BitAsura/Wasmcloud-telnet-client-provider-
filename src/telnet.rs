use crate::config::LinkConfig;
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

/// Telnet client handler
pub struct TelnetClient {
    config: LinkConfig,
}

impl TelnetClient {
    /// Create a new Telnet client
    pub fn new(config: LinkConfig) -> Self {
        Self { config }
    }

    /// Connect to the Telnet server and start receiving messages
    pub async fn run<F>(&self, mut message_handler: F) -> anyhow::Result<()>
    where
        F: FnMut(Vec<u8>) -> anyhow::Result<()> + Send,
    {
        let mut reconnect_attempts = 0u32;
        let mut current_delay = self.config.initial_reconnect_delay();

        loop {
            match self.connect_and_receive(&mut message_handler).await {
                Ok(_) => {
                    info!("Telnet connection closed normally");
                    break Ok(());
                }
                Err(e) => {
                    error!("Telnet connection error: {}", e);

                    // Check if we should retry
                    if self.config.max_reconnect_attempts > 0
                        && reconnect_attempts >= self.config.max_reconnect_attempts
                    {
                        error!(
                            "Maximum reconnection attempts ({}) reached",
                            self.config.max_reconnect_attempts
                        );
                        return Err(e);
                    }

                    reconnect_attempts += 1;
                    warn!(
                        "Attempting reconnection #{} after {:?}",
                        reconnect_attempts, current_delay
                    );

                    sleep(current_delay).await;

                    // Exponential backoff with max limit
                    current_delay =
                        std::cmp::min(current_delay * 2, self.config.max_reconnect_delay());
                }
            }
        }
    }

    /// Connect to Telnet server and receive messages
    async fn connect_and_receive<F>(&self, message_handler: &mut F) -> anyhow::Result<()>
    where
        F: FnMut(Vec<u8>) -> anyhow::Result<()>,
    {
        let address = self.config.address();
        info!("Connecting to Telnet server: {}", address);

        let mut stream = TcpStream::connect(&address).await?;

        info!("Telnet connection established to {}", address);

        let mut buf = vec![0u8; 4096];

        // Receive data
        loop {
            match stream.read(&mut buf).await {
                Ok(0) => {
                    info!("Telnet connection closed by server");
                    return Err(anyhow::anyhow!("Connection closed"));
                }
                Ok(n) => {
                    let data = buf[..n].to_vec();

                    // Filter out Telnet negotiation bytes (IAC sequences)
                    let filtered = filter_telnet_commands(&data);

                    if filtered.is_empty() {
                        debug!("Received Telnet negotiation only, skipping");
                        continue;
                    }

                    debug!("Received data: {} bytes", filtered.len());

                    if filtered.len() > self.config.max_message_size {
                        warn!(
                            "Message size {} exceeds limit {}, skipping",
                            filtered.len(),
                            self.config.max_message_size
                        );
                        continue;
                    }

                    message_handler(filtered)?;
                }
                Err(e) => {
                    error!("Error receiving data: {}", e);
                    return Err(e.into());
                }
            }
        }
    }
}

/// Filter out Telnet IAC (Interpret As Command) sequences from raw data.
///
/// Telnet protocol uses IAC (0xFF) as an escape byte. Common sequences:
/// - IAC WILL/WONT/DO/DONT <option>: 3 bytes (0xFF, 0xFB-0xFE, <option>)
/// - IAC SB ... IAC SE: Sub-negotiation (variable length)
/// - IAC <command>: 2 bytes for other commands
fn filter_telnet_commands(data: &[u8]) -> Vec<u8> {
    let mut result = Vec::new();
    let mut i = 0;

    while i < data.len() {
        if data[i] == 0xFF {
            // IAC byte
            if i + 1 >= data.len() {
                break;
            }
            match data[i + 1] {
                0xFB | 0xFC | 0xFD | 0xFE => {
                    // WILL, WONT, DO, DONT - skip 3 bytes
                    i += 3;
                }
                0xFA => {
                    // SB (sub-negotiation) - skip until IAC SE
                    i += 2;
                    while i + 1 < data.len() {
                        if data[i] == 0xFF && data[i + 1] == 0xF0 {
                            i += 2;
                            break;
                        }
                        i += 1;
                    }
                }
                0xFF => {
                    // Escaped 0xFF - output single 0xFF
                    result.push(0xFF);
                    i += 2;
                }
                _ => {
                    // Other IAC command - skip 2 bytes
                    i += 2;
                }
            }
        } else {
            result.push(data[i]);
            i += 1;
        }
    }

    result
}
