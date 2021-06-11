#![feature(generic_associated_types)]
#![feature(extended_key_value_attributes)]
#![doc = include_str!("../README.markdown")]

use bevy_app::{AppBuilder, EventReader, EventWriter, Plugin};
use bevy_core::Time;
use bevy_ecs::prelude::*;
use bevy_networking_turbulence::{
    ConnectionChannelsBuilder, ConnectionHandle, MessageChannelMode, MessageChannelSettings,
    NetworkEvent, NetworkResource, NetworkingPlugin, ReliableChannelSettings,
};
use crystalorb::{
    client::{stage::Stage as ClientStage, Client},
    clocksync::ClockSyncMessage,
    network_resource::{
        Connection as ConnectionTrait, ConnectionHandleType,
        NetworkResource as NetworkResourceTrait,
    },
    server::Server,
    timestamp::Timestamped,
    world::World,
    Config,
};
use serde::{de::DeserializeOwned, Serialize};
use std::{fmt::Debug, marker::PhantomData, time::Duration};
use turbulence::MessageChannels;

pub use bevy_networking_turbulence;
pub use crystalorb;

pub struct WrappedNetworkResource<'a>(pub &'a mut NetworkResource);
pub struct WrappedConnection<'a>(pub &'a mut MessageChannels);

impl<'a> NetworkResourceTrait for WrappedNetworkResource<'a> {
    type ConnectionType<'b> = WrappedConnection<'b>;

    fn get_connection(&mut self, handle: ConnectionHandleType) -> Option<Self::ConnectionType<'_>> {
        self.0
            .connections
            .get_mut(&(handle as ConnectionHandle))
            .map(|connection| WrappedConnection(connection.channels().unwrap()))
    }

    fn connections<'c>(
        &'c mut self,
    ) -> Box<dyn Iterator<Item = (usize, WrappedConnection<'c>)> + 'c> {
        Box::new(self.0.connections.iter_mut().map(|(handle, connection)| {
            (
                *handle as usize,
                WrappedConnection(connection.channels().unwrap()),
            )
        }))
    }
}

impl<'a> ConnectionTrait for WrappedConnection<'a> {
    fn recv<MessageType>(&mut self) -> Option<MessageType>
    where
        MessageType: Debug + Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
    {
        self.0.recv()
    }

    fn send<MessageType>(&mut self, message: MessageType) -> Option<MessageType>
    where
        MessageType: Debug + Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
    {
        self.0.send(message)
    }

    fn flush<MessageType>(&mut self)
    where
        MessageType: Debug + Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
    {
        self.0.flush::<MessageType>()
    }
}

pub struct CommandChannelSettings(pub MessageChannelSettings);
pub struct SnapshotChannelSettings(pub MessageChannelSettings);
pub struct ClockSyncChannelSettings(pub MessageChannelSettings);

trait MyClone {
    fn clone(&self) -> Self;
}

impl MyClone for MessageChannelMode {
    fn clone(&self) -> Self {
        match self {
            MessageChannelMode::Unreliable => MessageChannelMode::Unreliable,
            MessageChannelMode::Reliable {
                reliability_settings,
                max_message_len,
            } => MessageChannelMode::Reliable {
                reliability_settings: reliability_settings.clone(),
                max_message_len: *max_message_len,
            },
            MessageChannelMode::Compressed {
                reliability_settings,
                max_chunk_len,
            } => MessageChannelMode::Compressed {
                reliability_settings: reliability_settings.clone(),
                max_chunk_len: *max_chunk_len,
            },
        }
    }
}

impl MyClone for MessageChannelSettings {
    fn clone(&self) -> Self {
        Self {
            channel: self.channel,
            channel_mode: self.channel_mode.clone(),
            message_buffer_size: self.message_buffer_size,
            packet_buffer_size: self.packet_buffer_size,
        }
    }
}

impl Default for CommandChannelSettings {
    fn default() -> Self {
        Self(MessageChannelSettings {
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
    }
}

impl Clone for CommandChannelSettings {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl Default for SnapshotChannelSettings {
    fn default() -> Self {
        Self(MessageChannelSettings {
            channel: 1,
            channel_mode: MessageChannelMode::Unreliable,
            message_buffer_size: 64,
            packet_buffer_size: 64,
        })
    }
}

impl Clone for SnapshotChannelSettings {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl Default for ClockSyncChannelSettings {
    fn default() -> Self {
        Self(MessageChannelSettings {
            channel: 2,
            channel_mode: MessageChannelMode::Unreliable,
            message_buffer_size: 64,
            packet_buffer_size: 64,
        })
    }
}

impl Clone for ClockSyncChannelSettings {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

pub fn network_setup<WorldType: World>(
    mut net: ResMut<NetworkResource>,
    command_channel_settings_res: Res<CommandChannelSettings>,
    snapshot_channel_settings_res: Res<SnapshotChannelSettings>,
    clocksync_channel_settings_res: Res<ClockSyncChannelSettings>,
) {
    // Ugly consequence of set_channels_builder not accepting a FnOnce.
    let command_channel_settings = command_channel_settings_res.clone();
    let snapshot_channel_settings = snapshot_channel_settings_res.clone();
    let clocksync_channel_settings = clocksync_channel_settings_res.clone();

    net.set_channels_builder(move |builder: &mut ConnectionChannelsBuilder| {
        builder
            .register::<Timestamped<WorldType::CommandType>>(command_channel_settings.clone().0)
            .unwrap();

        builder
            .register::<Timestamped<WorldType::SnapshotType>>(snapshot_channel_settings.clone().0)
            .unwrap();

        builder
            .register::<ClockSyncMessage>(clocksync_channel_settings.clone().0)
            .unwrap();
    });
}

pub fn server_system<WorldType: World>(
    mut server: ResMut<Server<WorldType>>,
    time: Res<Time>,
    mut net: ResMut<NetworkResource>,
) {
    server.update(
        time.delta_seconds_f64(),
        time.seconds_since_startup(),
        &mut WrappedNetworkResource(&mut *net),
    );
}

pub struct CrystalOrbServerPlugin<WorldType: World> {
    config: Config,
    _world: PhantomData<WorldType>,
}

impl<WorldType: World> CrystalOrbServerPlugin<WorldType> {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            _world: PhantomData,
        }
    }
}

impl<WorldType: World> Plugin for CrystalOrbServerPlugin<WorldType> {
    fn build(&self, app: &mut AppBuilder) {
        app.add_plugin(NetworkingPlugin::default())
            .insert_resource(Server::<WorldType>::new(self.config.clone(), 0.0))
            .init_resource::<CommandChannelSettings>()
            .init_resource::<SnapshotChannelSettings>()
            .init_resource::<ClockSyncChannelSettings>()
            .add_startup_system(network_setup::<WorldType>.system())
            .add_system(server_system::<WorldType>.system());
    }
}

#[derive(Debug)]
pub enum ClientConnectionEvent {
    Connected(usize),
    Disconnected(usize),
}

#[derive(Default)]
pub struct ClientSystemState {
    is_ready: bool,
}

pub fn client_system<WorldType: World>(
    mut client_system_state: Local<ClientSystemState>,
    mut network_event_reader: EventReader<NetworkEvent>,
    mut client: ResMut<Client<WorldType>>,
    time: Res<Time>,
    mut net: ResMut<NetworkResource>,
    mut client_connection_events: EventWriter<ClientConnectionEvent>,
) {
    // TODO: For now, disconnection events are fatal and we do not attempt to reconnect. This
    // is why it is handled specially, in this location, rather than as a ClientState
    // transition.
    for network_event in network_event_reader.iter() {
        if let NetworkEvent::Disconnected(handle) = network_event {
            client_connection_events.send(ClientConnectionEvent::Disconnected(*handle as usize));
        }
    }
    client.update(
        time.delta_seconds_f64(),
        time.seconds_since_startup(),
        &mut WrappedNetworkResource(&mut *net),
    );
    match client.stage() {
        ClientStage::Ready(client) => {
            if !client_system_state.is_ready {
                client_connection_events.send(ClientConnectionEvent::Connected(client.client_id()));
            }
            client_system_state.is_ready = true;
        }
        _ => {
            client_system_state.is_ready = false;
        }
    }
}

pub struct CrystalOrbClientPlugin<WorldType: World> {
    config: Config,
    _world: PhantomData<WorldType>,
}

impl<WorldType: World> CrystalOrbClientPlugin<WorldType> {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            _world: PhantomData,
        }
    }
}

impl<WorldType: World> Plugin for CrystalOrbClientPlugin<WorldType> {
    fn build(&self, app: &mut AppBuilder) {
        app.add_plugin(NetworkingPlugin::default())
            .insert_resource(Client::<WorldType>::new(self.config.clone()))
            .init_resource::<CommandChannelSettings>()
            .init_resource::<SnapshotChannelSettings>()
            .init_resource::<ClockSyncChannelSettings>()
            .add_event::<ClientConnectionEvent>()
            .add_startup_system(network_setup::<WorldType>.system())
            .add_system(client_system::<WorldType>.system());
    }
}
