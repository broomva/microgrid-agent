//! Fleet MQTT sync — store-and-forward telemetry.
//!
//! Publishes microgrid metrics to a central MQTT broker for fleet-wide
//! monitoring. When connectivity is lost, messages are queued to disk
//! and replayed when the connection is restored.

use std::path::{Path, PathBuf};

use tracing::{debug, info};

use crate::config::ConnectivitySection;
use crate::devices::SensorReadings;

// ---------------------------------------------------------------------------
// Fleet sync
// ---------------------------------------------------------------------------

/// MQTT-based fleet synchronization with store-and-forward capability.
///
/// When online, publishes sensor metrics to the MQTT broker in real-time.
/// When offline, writes messages to a queue directory on disk. On reconnection,
/// queued messages are replayed in order.
pub struct FleetSync {
    broker: String,
    port: u16,
    queue_dir: PathBuf,
    // TODO: Add rumqttc::AsyncClient and EventLoop fields
    // client: Option<rumqttc::AsyncClient>,
}

impl FleetSync {
    /// Create a new fleet sync instance.
    pub fn new(config: &ConnectivitySection, queue_dir: &Path) -> Self {
        info!(
            broker = %config.mqtt_broker,
            port = config.mqtt_port,
            queue_dir = %queue_dir.display(),
            "Fleet sync initialized"
        );

        Self {
            broker: config.mqtt_broker.clone(),
            port: config.mqtt_port,
            queue_dir: queue_dir.to_path_buf(),
        }
    }

    /// Publish sensor metrics to the fleet MQTT broker.
    ///
    /// If the broker is unreachable, the message is queued to disk
    /// for later replay.
    pub async fn publish_metrics(&self, readings: &SensorReadings) -> anyhow::Result<()> {
        let payload = serde_json::to_vec(readings)?;

        // TODO: Implement actual MQTT publishing using rumqttc.
        //       For now, write to the store-and-forward queue.
        //
        //   let topic = format!("microgrid/{}/metrics", site_id);
        //   match &self.client {
        //       Some(client) => {
        //           client.publish(&topic, QoS::AtLeastOnce, false, &payload).await?;
        //       }
        //       None => {
        //           self.queue_to_disk(&topic, &payload)?;
        //       }
        //   }

        self.queue_to_disk("metrics", &payload)?;
        debug!("Metrics queued for sync");

        Ok(())
    }

    /// Start the MQTT connection and background event loop.
    ///
    /// This should be called once at startup. The event loop handles
    /// reconnection, keep-alive, and replaying queued messages.
    pub async fn start(&self) -> anyhow::Result<()> {
        // Ensure queue directory exists
        std::fs::create_dir_all(&self.queue_dir)?;

        // TODO: Implement MQTT connection with rumqttc:
        //
        //   let mut mqtt_options = MqttOptions::new("microgrid-agent", &self.broker, self.port);
        //   mqtt_options.set_keep_alive(Duration::from_secs(30));
        //   mqtt_options.set_clean_session(false); // Persistent session for QoS 1
        //
        //   let (client, mut eventloop) = AsyncClient::new(mqtt_options, 100);
        //   self.client = Some(client);
        //
        //   // Spawn background task for event loop
        //   tokio::spawn(async move {
        //       loop {
        //           match eventloop.poll().await {
        //               Ok(event) => debug!(?event, "MQTT event"),
        //               Err(e) => {
        //                   warn!(error = %e, "MQTT connection error, will retry");
        //                   tokio::time::sleep(Duration::from_secs(5)).await;
        //               }
        //           }
        //       }
        //   });
        //
        //   // Replay any queued messages
        //   self.replay_queue().await?;

        info!(
            broker = %self.broker,
            port = self.port,
            "Fleet sync started (store-and-forward mode)"
        );

        Ok(())
    }

    /// Write a message to the on-disk queue for later replay.
    fn queue_to_disk(&self, topic: &str, payload: &[u8]) -> anyhow::Result<()> {
        std::fs::create_dir_all(&self.queue_dir)?;

        let timestamp = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
        let filename = format!("{}-{}.json", topic, timestamp);
        let path = self.queue_dir.join(filename);

        std::fs::write(&path, payload)?;
        debug!(path = %path.display(), "Message queued to disk");

        Ok(())
    }

    // TODO: Add methods for:
    // - `replay_queue()` — read all queued messages and publish them, deleting on success
    // - `drain_old_queue(max_age: Duration)` — delete queued messages older than max_age
    // - `queue_depth() -> usize` — count of pending messages
    // - `subscribe(topic: &str)` — subscribe to fleet-wide commands (firmware updates, config changes)
}
