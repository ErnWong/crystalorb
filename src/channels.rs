use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClockSyncMessage {
    pub client_send_seconds_since_startup: f64,
    pub server_seconds_since_startup: f64,
    pub client_id: usize,
}
