use crate::{timestamp::Timestamped, world::World};
use bevy::prelude::*;
use bevy_networking_turbulence::{
    ConnectionChannelsBuilder, MessageChannelMode, MessageChannelSettings, NetworkResource,
    ReliableChannelSettings,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClockSyncMessage {
    pub client_send_seconds_since_startup: f64,
    pub server_seconds_since_startup: f64,
    pub client_id: usize,
}

pub fn network_setup<WorldType: World>(mut net: ResMut<NetworkResource>) {
    net.set_channels_builder(|builder: &mut ConnectionChannelsBuilder| {
        builder
            .register::<Timestamped<WorldType::CommandType>>(MessageChannelSettings {
                channel: 0,
                channel_mode: MessageChannelMode::Compressed {
                    reliability_settings: ReliableChannelSettings {
                        bandwidth: 4096,
                        recv_window_size: 1024,
                        send_window_size: 1024,
                        burst_bandwidth: 1024,
                        init_send: 512,
                        wakeup_time: Duration::from_millis(100),
                        initial_rtt: Duration::from_millis(200),
                        max_rtt: Duration::from_secs(2),
                        rtt_update_factor: 0.1,
                        rtt_resend_factor: 1.5,
                    },
                    max_chunk_len: 1024,
                },
                message_buffer_size: 64,
                packet_buffer_size: 64,
            })
            .unwrap();

        builder
            .register::<Timestamped<WorldType::SnapshotType>>(MessageChannelSettings {
                channel: 1,
                channel_mode: MessageChannelMode::Unreliable,
                message_buffer_size: 64,
                packet_buffer_size: 64,
            })
            .unwrap();

        builder
            .register::<ClockSyncMessage>(MessageChannelSettings {
                channel: 2,
                channel_mode: MessageChannelMode::Unreliable,
                message_buffer_size: 64,
                packet_buffer_size: 64,
            })
            .unwrap();
    });
}
