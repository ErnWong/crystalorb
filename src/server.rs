use crate::{
    channels::{network_setup, ClockSyncMessage},
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
use std::{collections::BinaryHeap, convert::TryInto, marker::PhantomData, net::SocketAddr};

pub struct Server<WorldType: World> {
    world: Timestamped<WorldType>,
    commands: BinaryHeap<EarliestPrioritized<WorldType::CommandType>>,
    timestep_overshoot_seconds: f32,
    seconds_since_last_snapshot: f32,
    config: Config,
}

impl<WorldType: World> Server<WorldType> {
    fn new(config: Config) -> Self {
        Self {
            world: Default::default(),
            commands: Default::default(),
            timestep_overshoot_seconds: 0.0,
            seconds_since_last_snapshot: 0.0,
            config,
        }
    }

    fn timestamp(&self) -> Timestamp {
        self.world.timestamp() + self.config.lag_compensation_frame_count()
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
        // Apply this command to our world later on.
        self.commands.push(command.clone().into());

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

    fn send_snapshot(&mut self, time: &Time, net: &mut NetworkResource) {
        self.seconds_since_last_snapshot += time.delta_seconds();
        if self.seconds_since_last_snapshot > self.config.snapshot_send_period {
            info!(
                "Broadcasting snapshot at timestamp: {:?}",
                self.world.timestamp()
            );
            self.seconds_since_last_snapshot = 0.0;
            net.broadcast_message(self.world.state());
        }
    }
}

impl<WorldType: World> Stepper for Server<WorldType> {
    fn step(&mut self) -> f32 {
        self.world.apply_stale_commands(&mut self.commands);
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

pub fn server_system<WorldType: World>(
    mut state: Local<ServerSystemState>,
    mut server: ResMut<Server<WorldType>>,
    time: Res<Time>,
    mut net: ResMut<NetworkResource>,
    network_events: Res<Events<NetworkEvent>>,
    mut client_connection_events: ResMut<Events<ClientConnectionEvent>>,
) {
    for network_event in state.network_event_reader.iter(&network_events) {
        if let Ok(client_connection_event) = network_event.try_into() {
            info!("Connection event: {:?}", client_connection_event);
            client_connection_events.send(client_connection_event);
        }
    }

    let mut new_commands: BinaryHeap<(
        EarliestPrioritized<WorldType::CommandType>,
        ConnectionHandle,
    )> = Default::default();
    for (handle, connection) in net.connections.iter_mut() {
        let channels = connection.channels().unwrap();
        while let Some(command) = channels.recv::<Timestamped<WorldType::CommandType>>() {
            new_commands.push((command.into(), *handle));
        }
        while let Some(mut clock_sync_message) = channels.recv::<ClockSyncMessage>() {
            clock_sync_message.server_seconds_since_startup = time.seconds_since_startup();
            clock_sync_message.client_id = *handle as usize;
            channels.send(clock_sync_message);
        }
    }
    while let Some((command, handle)) = new_commands.pop() {
        server.receive_command(command.clone(), handle, &mut *net);
    }
    server.advance(time.delta_seconds());

    server.send_snapshot(&*time, &mut *net);
}

pub fn server_setup<WorldType: World>(
    mut server: ResMut<Server<WorldType>>,
    time: Res<Time>,
    mut net: ResMut<NetworkResource>,
) {
    server.update_timestamp(&*time);

    let ip_address =
        bevy_networking_turbulence::find_my_ip_address().expect("Cannot find IP address");
    // TODO: Configurable port number and IP address.
    let socket_address = SocketAddr::new(ip_address, 14192);
    info!("Starting server");
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
