use crate::{
    channels::{network_setup, ClockSyncMessage},
    events::ClientConnectionEvent,
    fixed_timestepper,
    fixed_timestepper::{FixedTimestepper, Stepper},
    timestamp::{EarliestPrioritized, Timestamp, Timestamped},
    world::{World, WorldSimulation},
    Config,
};
use bevy::prelude::*;
use bevy_networking_turbulence::{
    ConnectionHandle, NetworkEvent, NetworkResource, NetworkingPlugin,
};
use std::{collections::BinaryHeap, convert::TryInto, marker::PhantomData};

pub struct Server<WorldType: World> {
    world_simulation: WorldSimulation<WorldType>,
    timestep_overshoot_seconds: f32,
    seconds_since_last_snapshot: f32,
    config: Config,
}

impl<WorldType: World> Server<WorldType> {
    fn new(config: Config) -> Self {
        Self {
            world_simulation: Default::default(),
            timestep_overshoot_seconds: 0.0,
            seconds_since_last_snapshot: 0.0,
            config,
        }
    }

    /// The timestamp that clients are supposed to be at (which should always be ahead of the
    /// server to compensate for the latency between the server and the clients).
    fn estimated_client_timestamp(&self) -> Timestamp {
        self.world_simulation.last_completed_timestamp()
            + self.config.lag_compensation_frame_count()
    }

    /// Positive refers that our world is ahead of the timestamp it is supposed to be, and
    /// negative refers that our world needs to catchup in the next frame.
    fn timestamp_drift(&self, time: &Time) -> Timestamp {
        self.estimated_client_timestamp()
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
        self.world_simulation
            .reset_last_completed_timestamp(timestamp);
    }

    fn apply_validated_command(
        &mut self,
        command: Timestamped<WorldType::CommandType>,
        command_source: Option<ConnectionHandle>,
        net: &mut NetworkResource,
    ) {
        info!("Received command from {:?} - {:?}", command_source, command);

        // Apply this command to our world later on.
        self.world_simulation.schedule_command(command.clone());

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
        // TODO: Is it valid to validate the timestamps?
        // && command.timestamp() >= self.world_simulation.last_completed_timestamp()
        // && command.timestamp() <= self.estimated_client_timestamp()
        {
            self.apply_validated_command(command, Some(command_source), net);
        }
    }

    /// Issue a command from the server to the world. The command will be scheduled to the
    /// estimated client's current timestamp.
    pub fn issue_command(&mut self, command: WorldType::CommandType, net: &mut NetworkResource) {
        self.apply_validated_command(
            Timestamped::new(command, self.estimated_client_timestamp()),
            None,
            net,
        );
    }

    pub fn display_state(&self) -> WorldType::DisplayStateType {
        self.world_simulation.display_state()
    }

    fn send_snapshot_if_needed(&mut self, time: &Time, net: &mut NetworkResource) {
        self.seconds_since_last_snapshot += time.delta_seconds();
        if self.seconds_since_last_snapshot > self.config.snapshot_send_period {
            trace!(
                "Broadcasting snapshot at timestamp: {:?} (note: drift error: {})",
                self.world_simulation.last_completed_timestamp(),
                self.timestamp_drift_seconds(time),
            );
            self.seconds_since_last_snapshot = 0.0;
            net.broadcast_message(self.world_simulation.last_completed_snapshot());
        }
    }
}

impl<WorldType: World> Stepper for Server<WorldType> {
    fn step(&mut self) -> f32 {
        trace!(
            "Server world timestamp: {:?}, estimated client timestamp: {:?}",
            self.world_simulation.last_completed_timestamp(),
            self.estimated_client_timestamp(),
        );
        self.world_simulation.step()
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

pub fn server_system<WorldType: World>(
    mut state: Local<ServerSystemState>,
    mut server: ResMut<Server<WorldType>>,
    time: Res<Time>,
    mut net: ResMut<NetworkResource>,
    network_events: Res<Events<NetworkEvent>>,
    mut client_connection_events: ResMut<Events<ClientConnectionEvent>>,
) {
    for network_event in state.network_event_reader.iter(&network_events) {
        info!("Server NetworkEvent - {:?}", network_event);

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
    trace!(
        "Timestamp drift: {:?}",
        server.timestamp_drift_seconds(&time)
    );
    if -server.timestamp_drift_seconds(&time) > server.config.timestamp_skip_threshold_seconds {
        let corrected_timestamp =
            server.world_simulation.last_completed_timestamp() - server.timestamp_drift(&time);
        warn!(
            "Server is too far behind. Skipping timestamp from {:?} to {:?}",
            server.world_simulation.last_completed_timestamp(),
            corrected_timestamp
        );
        server
            .world_simulation
            .reset_last_completed_timestamp(corrected_timestamp);
    }

    server.send_snapshot_if_needed(&*time, &mut *net);
}

pub struct EndpointUrl(pub String);

pub fn server_setup<WorldType: World>(
    mut server: ResMut<Server<WorldType>>,
    time: Res<Time>,
    mut net: ResMut<NetworkResource>,
    endpoint_url: Res<EndpointUrl>,
) {
    server.update_timestamp(&*time);
    info!("Starting server - listening at {}", endpoint_url.0.clone());
    net.listen(endpoint_url.0.clone());
}

#[derive(Default)]
pub struct NetworkedPhysicsServerPlugin<WorldType: World> {
    config: Config,
    endpoint_url: String,
    _world_type: PhantomData<WorldType>,
}

impl<WorldType: World> NetworkedPhysicsServerPlugin<WorldType> {
    pub fn new(config: Config, endpoint_url: String) -> Self {
        Self {
            config,
            endpoint_url,
            _world_type: PhantomData,
        }
    }
}

impl<WorldType: World> Plugin for NetworkedPhysicsServerPlugin<WorldType> {
    fn build(&self, app: &mut AppBuilder) {
        app.add_plugin(NetworkingPlugin::default())
            .add_event::<ClientConnectionEvent>()
            .add_resource(Server::<WorldType>::new(self.config.clone()))
            .add_resource(EndpointUrl(self.endpoint_url.clone()))
            .add_startup_system(network_setup::<WorldType>.system())
            .add_startup_system(server_setup::<WorldType>.system())
            .add_system(server_system::<WorldType>.system());
    }
}
