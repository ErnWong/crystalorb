use crate::{
    network_resource::{Connection, NetworkResource},
    Config,
};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use tracing::{info, trace, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClockSyncMessage {
    pub client_send_seconds_since_startup: f64,
    pub server_seconds_since_startup: f64,
    pub client_id: usize,
}

/// Client-side clock syncing logic. This is responsible for sending out clock synchronization
/// request messages to the server, and handling the server's responses. This also serves double
/// duty for determining what client identifier has been allocated by the server to this client.
/// For each server response, a sample of the time clock offset between the client and the server
/// is calculated, and a rolling average (ignoring outliers) of these samples is used to update the
/// effective clock offset that is used by the client once the effective clock offset deviates too
/// far from the rolling average.
pub struct ClockSyncer {
    /// The difference in seconds between client's seconds_since_startup and server's
    /// seconds_since_startup, where a positive value refers that an earlier client time value
    /// corresponds to the same instant as a later server time value. Since servers start
    /// earlier than clients, this value should in theory always be positive. The value stored
    /// here is the "effective" offset that is used by the client, and may be up to date with
    /// the latest measurements. Before initialization, the value is `None`.
    server_seconds_offset: Option<f64>,

    /// A ring buffer of the latest measurements of the clock difference between client and server.
    server_seconds_offset_samples: VecDeque<f64>,

    seconds_since_last_request_sent: f64,

    /// An identifier issued by the server for us to identify ourselves from other clients. Used,
    /// for example, for issuing our player's commands to the server.
    client_id: Option<usize>,

    config: Config,
}

impl ClockSyncer {
    pub fn new(config: Config) -> Self {
        Self {
            server_seconds_offset: None,
            server_seconds_offset_samples: Default::default(),
            seconds_since_last_request_sent: 0.0,
            client_id: None,
            config,
        }
    }

    pub fn update<NetworkResourceType: NetworkResource>(
        &mut self,
        delta_seconds: f64,
        seconds_since_startup: f64,
        net: &mut NetworkResourceType,
    ) {
        self.seconds_since_last_request_sent += delta_seconds;
        if self.seconds_since_last_request_sent > self.config.clock_sync_request_period {
            self.seconds_since_last_request_sent = 0.0;

            trace!("Sending clocksync request");
            net.broadcast_message(ClockSyncMessage {
                client_send_seconds_since_startup: seconds_since_startup,
                server_seconds_since_startup: 0.0,
                client_id: 0,
            });
        }

        let mut latest_server_seconds_offset: Option<f64> = None;
        for (_, mut connection) in net.connections() {
            while let Some(sync) = connection.recv::<ClockSyncMessage>() {
                let received_time = seconds_since_startup;
                let corresponding_client_time =
                    (sync.client_send_seconds_since_startup + received_time) / 2.0;
                let offset = sync.server_seconds_since_startup - corresponding_client_time;

                trace!(
                    "Received clocksync response. ClientId: {}. Estimated clock offset: {}",
                    sync.client_id,
                    offset,
                );

                // Only one sample per update. Save the latest one.
                latest_server_seconds_offset = Some(offset);

                let existing_id = self.client_id.get_or_insert(sync.client_id);
                assert_eq!(*existing_id, sync.client_id);
            }
        }

        if let Some(measured_offset) = latest_server_seconds_offset {
            self.add_sample(measured_offset);
        }
    }

    pub fn is_ready(&self) -> bool {
        self.server_seconds_offset.is_some() && self.client_id.is_some()
    }

    /// An identifier issued by the server for us to identify ourselves from other clients. Used,
    /// for example, for issuing our player's commands to the server.
    ///
    /// This is `None` if no server responses had been received yet.
    pub fn client_id(&self) -> &Option<usize> {
        &self.client_id
    }

    /// The difference in seconds between client's seconds_since_startup and server's
    /// seconds_since_startup, where a positive value refers that an earlier client time value
    /// corresponds to the same instant as a later server time value. Since servers start
    /// earlier than clients, this value should in theory always be positive. The value stored
    /// here is the "effective" offset that is used by the client, and may be up to date with
    /// the latest measurements.
    ///
    /// Before initialization, the value is `None`.
    pub fn server_seconds_offset(&self) -> &Option<f64> {
        &self.server_seconds_offset
    }

    /// Convert local client seconds since startup into server seconds since startup using the
    /// latest estimated time difference between the client and the server, if available.
    pub fn server_seconds_since_startup(&self, client_seconds_since_startup: f64) -> Option<f64> {
        self.server_seconds_offset
            .map(|offset| client_seconds_since_startup + offset)
    }

    fn add_sample(&mut self, measured_seconds_offset: f64) {
        self.server_seconds_offset_samples
            .push_back(measured_seconds_offset);

        assert!(
            self.server_seconds_offset_samples.len()
                <= self.config.clock_sync_samples_needed_to_store()
        );

        if self.server_seconds_offset_samples.len()
            >= self.config.clock_sync_samples_needed_to_store()
        {
            let rolling_mean_offset_seconds = self.rolling_mean_offset_seconds();

            let is_initial_sync = self.server_seconds_offset.is_none();
            let has_desynced = self.server_seconds_offset.map_or(false, |offset| {
                (rolling_mean_offset_seconds - offset).abs()
                    > self.config.max_tolerable_clock_deviation
            });

            if is_initial_sync || has_desynced {
                if is_initial_sync {
                    info!(
                        "Initial server seconds offset: {}",
                        rolling_mean_offset_seconds
                    );
                }
                if has_desynced {
                    warn!(
                        "Client updating clock offset from {:?} to {:?}",
                        self.server_seconds_offset, rolling_mean_offset_seconds,
                    );
                }
                self.server_seconds_offset = Some(rolling_mean_offset_seconds);
            }
            self.server_seconds_offset_samples.pop_front();
        }
    }

    fn rolling_mean_offset_seconds(&self) -> f64 {
        let mut samples: Vec<f64> = self.server_seconds_offset_samples.iter().copied().collect();
        samples.sort_unstable_by(|a, b| a.partial_cmp(b).expect("Samples should not be NaN"));

        let samples_without_outliers =
            &samples[self.config.clock_sync_samples_to_discard_per_extreme()
                ..samples.len() - self.config.clock_sync_samples_to_discard_per_extreme()];

        samples_without_outliers.iter().sum::<f64>() / samples_without_outliers.len() as f64
    }
}
