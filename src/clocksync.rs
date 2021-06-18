//! The `clocksync` module is responsible for managing the difference between the clocks on the
//! local and remote machine (aka client and server clocks), and for the client to estimate the
//! server's local time.
//!
//! The design of this module is currently suboptimal and could be improved:
//! - Only the client-side clocksyncing logic is located here. The server-side logic is located in
//!   the [`Server`](crate::server::Server) itself.
//! - This clocksync module serves double duty for also informing the client which `client_id` has
//!   been allocated to this client by the server.

use crate::{
    network_resource::{Connection, NetworkResource},
    Config,
};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use tracing::{info, trace, warn};

/// A message sent between client and server to measure the number of seconds difference between
/// the clocks of the two machines.
///
/// 1. The client first sends this structure to the server, ignoring the
///    `server_seconds_since_startup` and `client_id` fields. The
///    `client_send_seconds_since_startup` records the client's local time that this message was
///    prepared and sent.
/// 2. The server sends back this structure to the client, preserving the same
///    `client_send_seconds_since_startup` value as it received, but populating the remaining
///    `client_id` and `server_seconds_since_startup` values. The `server_seconds_since_startup` is
///    the server's local time at which it prepared and sent this message back to the client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClockSyncMessage {
    /// The time (in seconds) when the client side sends this `ClockSyncMessage` request to the
    /// server.
    pub client_send_seconds_since_startup: f64,

    /// The time (in seconds) when the server side sends this `ClockSyncMessage` back to the client
    /// as a reply to the client's request.
    pub server_seconds_since_startup: f64,
}

/// Client-side clock syncing logic. This is responsible for sending out clock synchronization
/// request messages to the server, and handling the server's responses. This also serves double
/// duty for determining what client identifier has been allocated by the server to this client.
/// For each server response, a sample of the time clock offset between the client and the server
/// is calculated, and a rolling average (ignoring outliers) of these samples is used to update the
/// effective clock offset that is used by the client once the effective clock offset deviates too
/// far from the rolling average.
#[derive(Debug)]
pub struct ClockSyncer<ConfigType: Config> {
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
    client_id: Option<<ConfigType::NetworkResourceType as NetworkResource>::ConnectionHandleType>,
}

impl<ConfigType: Config> ClockSyncer<ConfigType> {
    /// Create a new [`ClockSyncer`] with the given configuration parameters. The [`ClockSyncer`]
    /// will start off in a ["not ready" state](ClockSyncer::is_ready) until after multiple
    /// [`update`](ClockSyncer::update) calls. During the time when the [`ClockSyncer`] is not
    /// ready, some of the methods may return `None` as documented.
    pub fn new() -> Self {
        Self {
            server_seconds_offset: None,
            server_seconds_offset_samples: VecDeque::new(),
            seconds_since_last_request_sent: 0.0,
            client_id: None,
        }
    }

    /// Perform the next update, where the [`ClockSyncer`] tries to gather more information about
    /// the client-server clock differences and makes adjustments when needed.
    ///
    /// # Panics
    ///
    /// Panics when the [`ClockSyncer`] receives inconsistent `client_id` values from the server.
    pub fn update(
        &mut self,
        delta_seconds: f64,
        seconds_since_startup: f64,
        net: &mut ConfigType::NetworkResourceType,
        client_id: <ConfigType::NetworkResourceType as NetworkResource>::ConnectionHandleType,
    ) {
        self.seconds_since_last_request_sent += delta_seconds;
        if self.seconds_since_last_request_sent > ConfigType::CLOCK_SYNC_REQUEST_PERIOD {
            self.seconds_since_last_request_sent = 0.0;

            trace!("Sending clocksync request");
            net.broadcast_message(ClockSyncMessage {
                client_send_seconds_since_startup: seconds_since_startup,
                server_seconds_since_startup: 0.0,
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
                    client_id,
                    offset,
                );

                // Only one sample per update. Save the latest one.
                latest_server_seconds_offset = Some(offset);

                let existing_id = self.client_id.get_or_insert(client_id);
                assert_eq!(*existing_id, client_id);
            }
        }

        if let Some(measured_offset) = latest_server_seconds_offset {
            self.add_sample(measured_offset);
        }
    }

    /// Whether the [`ClockSyncer`] has enough information to make useful estimates.
    ///
    /// It is guaranteed that once the [`ClockSyncer`] becomes "ready", it stays "ready".
    ///
    /// TODO: Enforce this invariant.
    pub fn is_ready(&self) -> bool {
        self.server_seconds_offset.is_some() && self.client_id.is_some()
    }

    /// How many measurement samples the [`ClockSyncer`] has collected and currently stored.
    /// Previously collected samples that have since then been discarded are not counted. This
    /// merely counts the number of samples in the current moving window.
    pub fn sample_count(&self) -> usize {
        self.server_seconds_offset_samples.len()
    }

    /// How many samples are needed to start making useful estimates.
    pub fn samples_needed(&self) -> usize {
        ConfigType::clock_sync_samples_needed_to_store()
    }

    /// An identifier issued by the server for us to identify ourselves from other clients. Used,
    /// for example, for issuing our player's commands to the server.
    ///
    /// This is `None` if no server responses had been received yet.
    pub fn client_id(
        &self,
    ) -> &Option<<ConfigType::NetworkResourceType as NetworkResource>::ConnectionHandleType> {
        &self.client_id
    }

    /// The difference in seconds between client's `seconds_since_startup` and server's
    /// `seconds_since_startup`, where a positive value refers that an earlier client time value
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
                <= ConfigType::clock_sync_samples_needed_to_store()
        );

        if self.server_seconds_offset_samples.len()
            >= ConfigType::clock_sync_samples_needed_to_store()
        {
            let rolling_mean_offset_seconds = self.rolling_mean_offset_seconds();

            let is_initial_sync = self.server_seconds_offset.is_none();
            let has_desynced = self.server_seconds_offset.map_or(false, |offset| {
                (rolling_mean_offset_seconds - offset).abs()
                    > ConfigType::MAX_TOLERABLE_CLOCK_DEVIATION
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

        let samples_without_outliers = &samples
            [ConfigType::clock_sync_samples_to_discard_per_extreme()
                ..samples.len() - ConfigType::clock_sync_samples_to_discard_per_extreme()];

        samples_without_outliers.iter().sum::<f64>() / samples_without_outliers.len() as f64
    }
}
