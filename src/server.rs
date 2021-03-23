use crate::{
    channels::{network_setup, ClockSyncMessage},
    command::CommandBuffer,
    events::ClientConnectionEvent,
    fixed_timestepper,
    fixed_timestepper::{FixedTimestepper, Stepper},
    timestamp::{EarliestPrioritized, Timestamp, Timestamped},
    world::World,
    Config,
};
use bevy::prelude::*;
use bevy_networking_turbulence::{
    ConnectionHandle, NetworkEvent, NetworkResource, NetworkingPlugin,
};
use std::{
    collections::{BinaryHeap, HashMap},
    convert::TryInto,
    marker::PhantomData,
    net::SocketAddr,
};

pub struct Server<WorldType: World> {
    world: Timestamped<WorldType>,
    command_buffer: CommandBuffer<WorldType::CommandType>,
    timestep_overshoot_seconds: f32,
    seconds_since_last_snapshot: f32,
    config: Config,
}

impl<WorldType: World> Server<WorldType> {
    fn new(config: Config) -> Self {
        Self {
            world: Default::default(),
            command_buffer: CommandBuffer::new(),
            timestep_overshoot_seconds: 0.0,
            seconds_since_last_snapshot: 0.0,
            config,
        }
    }

    fn timestamp(&self) -> Timestamp {
        self.world.timestamp() + self.config.lag_compensation_frame_count()
    }

    /// Positive refers that our world is ahead of the timestamp it is supposed to be, and
    /// negative refers that our world needs to catchup in the next frame.
    fn timestamp_drift(&self, time: &Time) -> Timestamp {
        self.timestamp()
            - Timestamp::from_seconds(time.seconds_since_startup(), self.config.timestep_seconds)
    }

    /// Positive refers that our world is ahead of the timestamp it is supposed to be, and
    /// negative refers that our world needs to catchup in the next frame.
    fn timestamp_drift_seconds(&self, time: &Time) -> f32 {
        self.timestamp_drift(time)
            .as_seconds(self.config.timestep_seconds)
    }

    fn update_timestamp(&mut self, time: &Time) {
        let timestamp =
            Timestamp::from_seconds(time.seconds_since_startup(), self.config.timestep_seconds)
                - self.config.lag_compensation_frame_count();
        info!(
            "Updating server world timestamp to {:?} (note: lag compensation frame count = {})",
            timestamp,
            self.config.lag_compensation_frame_count()
        );
        self.world.set_timestamp(timestamp);
    }

    fn apply_validated_command(
        &mut self,
        command: Timestamped<WorldType::CommandType>,
        command_source: Option<ConnectionHandle>,
        net: &mut NetworkResource,
    ) {
        info!("Received command from {:?} - {:?}", command_source, command);

        // Apply this command to our world later on.
        self.command_buffer.insert(command.clone());

        // Relay command to every other client.
        for (handle, connection) in net.connections.iter_mut() {
            // Don't send it back to the same client if there is one.
            if let Some(source_handle) = command_source {
                if *handle == source_handle {
                    continue;
                }
            }
            let channels = connection.channels().unwrap();
            let result = channels.send(command.clone());
            channels.flush::<Timestamped<WorldType::CommandType>>();
            if let Some(message) = result {
                error!("Failed to relay command to [{}]: {:?}", handle, message);
            }
        }
    }

    fn receive_command(
        &mut self,
        command: Timestamped<WorldType::CommandType>,
        command_source: ConnectionHandle,
        net: &mut NetworkResource,
    ) {
        if WorldType::command_is_valid(command.inner(), command_source as usize)
            && command.timestamp() >= self.world.timestamp()
            && command.timestamp() <= self.timestamp()
        {
            self.apply_validated_command(command, Some(command_source), net);
        }
    }

    pub fn issue_command(&mut self, command: WorldType::CommandType, net: &mut NetworkResource) {
        self.apply_validated_command(Timestamped::new(command, self.timestamp()), None, net);
    }

    pub fn display_state(&self) -> WorldType::DisplayStateType {
        self.world.inner().display_state()
    }

    fn send_snapshot(&mut self, time: &Time, net: &mut NetworkResource) {
        self.seconds_since_last_snapshot += time.delta_seconds();
        if self.seconds_since_last_snapshot > self.config.snapshot_send_period {
            trace!(
                "Broadcasting snapshot at timestamp: {:?} (note: drift error: {})",
                self.world.timestamp(),
                self.timestamp_drift_seconds(time),
            );
            self.seconds_since_last_snapshot = 0.0;
            net.broadcast_message(self.world.snapshot());
        }
    }
}

impl<WorldType: World> Stepper for Server<WorldType> {
    fn step(&mut self) -> f32 {
        self.command_buffer.discard_old(self.world.timestamp());
        self.world.apply_commands(&self.command_buffer);
        self.world.step();

        self.config.timestep_seconds
    }
}

impl<WorldType: World> FixedTimestepper for Server<WorldType> {
    fn advance(&mut self, delta_seconds: f32) {
        self.timestep_overshoot_seconds =
            fixed_timestepper::advance(self, delta_seconds, self.timestep_overshoot_seconds);
    }
}

#[derive(Default)]
pub struct ServerSystemState {
    network_event_reader: EventReader<NetworkEvent>,
}

#[derive(Default)]
pub struct TimeRemainingBeforeDisconnect(HashMap<ConnectionHandle, f64>);

pub fn server_system<WorldType: World>(
    mut state: Local<ServerSystemState>,
    mut server: ResMut<Server<WorldType>>,
    mut client_time_remaining_before_disconnect: Local<TimeRemainingBeforeDisconnect>,
    time: Res<Time>,
    mut net: ResMut<NetworkResource>,
    network_events: Res<Events<NetworkEvent>>,
    mut client_connection_events: ResMut<Events<ClientConnectionEvent>>,
) {
    for network_event in state.network_event_reader.iter(&network_events) {
        let handle = match network_event {
            NetworkEvent::Packet(handle, ..) => handle,
            NetworkEvent::Connected(handle) => handle,
            NetworkEvent::Disconnected(handle) => handle,
        };

        // TODO: Deduplicate code with below.
        client_time_remaining_before_disconnect.0.insert(
            *handle,
            time.seconds_since_startup() + server.config.connection_timeout_seconds as f64,
        );

        if let Ok(client_connection_event) = network_event.try_into() {
            info!("Connection event: {:?}", client_connection_event);
            client_connection_events.send(client_connection_event);
        }
    }

    let mut new_commands: BinaryHeap<(
        EarliestPrioritized<WorldType::CommandType>,
        ConnectionHandle,
    )> = Default::default();
    let mut clock_syncs = Vec::new();
    for (handle, connection) in net.connections.iter_mut() {
        let channels = connection.channels().unwrap();

        // For diagnostic purposes:
        if !channels.is_connected() {
            warn!("Channels to client {} disconnected", handle);
        }

        while let Some(command) = channels.recv::<Timestamped<WorldType::CommandType>>() {
            new_commands.push((command.into(), *handle));
        }
        while let Some(mut clock_sync_message) = channels.recv::<ClockSyncMessage>() {
            trace!("Replying to clock sync message. client_id: {}", handle);
            clock_sync_message.server_seconds_since_startup = time.seconds_since_startup();
            clock_sync_message.client_id = *handle as usize;
            clock_syncs.push((*handle, clock_sync_message));

            // TODO: Deduplicate code with above.
            client_time_remaining_before_disconnect.0.insert(
                *handle,
                time.seconds_since_startup() + server.config.connection_timeout_seconds as f64,
            );
        }
    }
    while let Some((command, handle)) = new_commands.pop() {
        server.receive_command(command.clone(), handle, &mut *net);
    }
    for (handle, clock_sync_message) in clock_syncs {
        net.send_message(handle, clock_sync_message).unwrap();
    }

    // Compensate for any drift.
    // TODO: Remove duplicate code between client and server.
    let next_delta_seconds = (time.delta_seconds() - server.timestamp_drift_seconds(&time))
        .clamp(0.0, server.config.update_delta_seconds_max);

    server.advance(next_delta_seconds);

    // If drift is too large and we still couldn't keep up, do a time skip.
    if server.timestamp_drift_seconds(&time) > server.config.timestamp_skip_threshold_seconds {
        // Note: only skip on the old world's timestamp.
        // If new world couldn't catch up, then it can simply grab the next server snapshot
        // when it arrives.
        let corrected_timestamp = server.timestamp() - server.timestamp_drift(&time);
        server.world.set_timestamp(corrected_timestamp);
    }

    server.send_snapshot(&*time, &mut *net);

    // Disconnect from clients that have timed out, since turbulence's disconnection mechanism
    // doesn't seem to work at time of writing.
    let mut clients_to_disconnect = Vec::new();
    for (connection_handle, disconnect_time) in &client_time_remaining_before_disconnect.0 {
        if time.seconds_since_startup() > *disconnect_time {
            clients_to_disconnect.push(*connection_handle);
        }
    }
    for connection_handle in clients_to_disconnect {
        info!("Client timed-out, disconnecting: {:?}", connection_handle);
        net.connections.remove(&connection_handle);
        client_time_remaining_before_disconnect
            .0
            .remove(&connection_handle);
        client_connection_events.send(ClientConnectionEvent::Disconnected(
            connection_handle as usize,
        ));
    }
}

pub fn server_setup<WorldType: World>(
    mut server: ResMut<Server<WorldType>>,
    time: Res<Time>,
    mut net: ResMut<NetworkResource>,
) {
    server.update_timestamp(&*time);

    let socket_address = SocketAddr::new(
        "0.0.0.0".parse().unwrap(),
        std::env::var("PORT").map_or(9001, |port| port.parse().unwrap()),
    );
    info!("Starting server - listening at {}", socket_address);
    net.listen(socket_address);
}

#[derive(Default)]
pub struct NetworkedPhysicsServerPlugin<WorldType: World> {
    config: Config,
    _world_type: PhantomData<WorldType>,
}

impl<WorldType: World> NetworkedPhysicsServerPlugin<WorldType> {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            _world_type: PhantomData,
        }
    }
}

impl<WorldType: World> Plugin for NetworkedPhysicsServerPlugin<WorldType> {
    fn build(&self, app: &mut AppBuilder) {
        app.add_plugin(NetworkingPlugin)
            .add_event::<ClientConnectionEvent>()
            .add_resource(Server::<WorldType>::new(self.config.clone()))
            .add_startup_system(network_setup::<WorldType>.system())
            .add_startup_system(server_setup::<WorldType>.system())
            .add_system(server_system::<WorldType>.system());
    }
}
