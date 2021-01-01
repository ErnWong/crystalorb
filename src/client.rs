use crate::{
    channels::{network_setup, ClockSyncMessage},
    events::ClientConnectionEvent,
    fixed_timestepper,
    fixed_timestepper::{FixedTimestepper, Stepper},
    old_new::OldNew,
    timestamp::{EarliestPrioritized, Timestamp, Timestamped},
    world::{State, World},
    Config,
};
use bevy::prelude::*;
use bevy_networking_turbulence::{NetworkResource, NetworkingPlugin};
use std::{collections::BinaryHeap, marker::PhantomData, net::SocketAddr};

pub struct Client<WorldType: World> {
    state: Option<ClientState<WorldType>>,
}

impl<WorldType: World> Client<WorldType> {
    fn new(config: Config) -> Self {
        Self {
            state: Some(ClientState::SyncingInitialTimestamp(
                SyncingInitialTimestampClient::new(config),
            )),
        }
    }

    fn update(
        &mut self,
        time: &Time,
        net: &mut NetworkResource,
        client_connection_events: &mut Events<ClientConnectionEvent>,
    ) {
        let should_transition = match &mut self.state.as_mut().unwrap() {
            ClientState::SyncingInitialTimestamp(client) => client.update(time, net),
            ClientState::SyncingInitialState(client) => client.update(time, net),
            ClientState::Ready(client) => client.update(time, net),
        };
        if should_transition {
            self.state = Some(match self.state.take().unwrap() {
                ClientState::SyncingInitialTimestamp(client) => {
                    client.into_next_state(time).unwrap()
                }
                ClientState::SyncingInitialState(client) => {
                    client.into_next_state(client_connection_events).unwrap()
                }
                ClientState::Ready(client) => client
                    .into_next_state(net, client_connection_events)
                    .unwrap(),
            });
        }
    }

    pub fn state(&self) -> &ClientState<WorldType> {
        self.state.as_ref().unwrap()
    }

    pub fn state_mut(&mut self) -> &mut ClientState<WorldType> {
        self.state.as_mut().unwrap()
    }
}

pub enum ClientState<WorldType: World> {
    SyncingInitialTimestamp(SyncingInitialTimestampClient<WorldType>),
    SyncingInitialState(SyncingInitialStateClient<WorldType>),
    Ready(ReadyClient<WorldType>),
}

pub struct SyncingInitialTimestampClient<WorldType: World> {
    config: Config,
    server_seconds_offset_sum: f64,
    sample_count: usize,
    seconds_since_last_send: f32,
    client_id: usize,
    _world_type: PhantomData<WorldType>,
}

impl<WorldType: World> SyncingInitialTimestampClient<WorldType> {
    fn new(config: Config) -> Self {
        info!("Syncing timestamp");
        SyncingInitialTimestampClient {
            config,
            server_seconds_offset_sum: 0.0,
            sample_count: 0,
            seconds_since_last_send: 0.0,
            client_id: 0,
            _world_type: PhantomData,
        }
    }
    fn update(&mut self, time: &Time, net: &mut NetworkResource) -> bool {
        for (_, connection) in net.connections.iter_mut() {
            let channels = connection.channels().unwrap();
            while let Some(sync) = channels.recv::<ClockSyncMessage>() {
                let received_time = time.seconds_since_startup();
                let corresponding_client_time =
                    (sync.client_send_seconds_since_startup + received_time) / 2.0;
                let offset = sync.server_seconds_since_startup - corresponding_client_time;
                info!(
                    "Received clock sync message. ClientId: {}. Estimated clock offset: {}",
                    sync.client_id, offset,
                );
                self.server_seconds_offset_sum += offset;
                self.sample_count += 1;
                self.client_id = sync.client_id;
            }
        }

        self.seconds_since_last_send += time.delta_seconds();
        if self.seconds_since_last_send > self.config.initial_clock_sync_period {
            self.seconds_since_last_send = 0.0;
            info!("Sending clock sync message");
            net.broadcast_message(ClockSyncMessage {
                client_send_seconds_since_startup: time.seconds_since_startup(),
                server_seconds_since_startup: 0.0,
                client_id: 0,
            });
        }

        self.should_transition()
    }

    fn should_transition(&self) -> bool {
        self.sample_count > self.config.timestamp_sync_needed_sample_count
    }

    fn into_next_state(self, time: &Time) -> Option<ClientState<WorldType>> {
        if self.should_transition() {
            let seconds_offset = self.server_seconds_offset_sum / self.sample_count as f64;
            let server_time = time.seconds_since_startup() + seconds_offset;
            let initial_timestamp =
                Timestamp::from_seconds(server_time, self.config.timestep_seconds);
            Some(ClientState::SyncingInitialState(
                SyncingInitialStateClient::new(self.config, initial_timestamp, self.client_id),
            ))
        } else {
            None
        }
    }
}

pub struct SyncingInitialStateClient<WorldType: World> {
    client: ActiveClient<WorldType>,
}

impl<WorldType: World> SyncingInitialStateClient<WorldType> {
    fn new(config: Config, initial_timestamp: Timestamp, client_id: usize) -> Self {
        info!(
            "Initial timestamp: {:?}, client_id: {}",
            initial_timestamp, client_id
        );
        info!("Syncing initial state");
        Self {
            client: ActiveClient::new(config, initial_timestamp, client_id),
        }
    }
    fn update(&mut self, time: &Time, net: &mut NetworkResource) -> bool {
        self.client.update(time, net);
        self.should_transition()
    }

    fn should_transition(&self) -> bool {
        self.client.last_queued_snapshot_timestamp.is_some()
    }

    fn into_next_state(
        self,
        client_connection_events: &mut Events<ClientConnectionEvent>,
    ) -> Option<ClientState<WorldType>> {
        if self.should_transition() {
            client_connection_events.send(ClientConnectionEvent::Connected(self.client.client_id));
            Some(ClientState::Ready(ReadyClient::new(self.client)))
        } else {
            None
        }
    }
}

pub struct ReadyClient<WorldType: World> {
    client: ActiveClient<WorldType>,
}

impl<WorldType: World> ReadyClient<WorldType> {
    pub fn timestamp(&self) -> Timestamp {
        self.client.timestamp()
    }

    pub fn client_id(&self) -> usize {
        self.client.client_id
    }

    /// Issue a command from this client's player to the world.
    pub fn issue_command(&mut self, command: WorldType::CommandType, net: &mut NetworkResource) {
        let command = Timestamped::new(command, self.timestamp());
        self.client.receive_command(command.clone());
        net.broadcast_message(command);
    }

    pub fn world_state(&self) -> &WorldType::StateType {
        &self.client.display_state
    }

    fn new(client: ActiveClient<WorldType>) -> Self {
        info!("Client ready");
        Self { client }
    }

    fn update(&mut self, time: &Time, net: &mut NetworkResource) -> bool {
        self.client.update(time, net);
        self.should_transition(net)
    }

    fn should_transition(&self, _net: &mut NetworkResource) -> bool {
        // TODO: Check for disconnection.
        false
    }

    fn into_next_state(
        self,
        net: &mut NetworkResource,
        client_connection_events: &mut Events<ClientConnectionEvent>,
    ) -> Option<ClientState<WorldType>> {
        if self.should_transition(net) {
            client_connection_events
                .send(ClientConnectionEvent::Disconnected(self.client.client_id));
            Some(ClientState::SyncingInitialTimestamp(
                SyncingInitialTimestampClient::new(self.client.config),
            ))
        } else {
            None
        }
    }
}

pub struct ActiveClient<WorldType: World> {
    client_id: usize,

    /// The next server snapshot that needs applying after the current latest snapshot has been
    /// fully interpolated into.
    queued_snapshot: Option<Timestamped<WorldType::StateType>>,

    /// The timestamp of the last queued snapshot from the server, so we can discard stale
    /// snapshots from the server when the arrive out of order. This persists even after the queued
    /// snapshot has been cleared after it has been applied to the world.
    last_queued_snapshot_timestamp: Option<Timestamp>,

    /// The physics world simulation with and without the latest server snapshot applied.
    /// `world.get_new()` has the latest server snapshot applied.
    /// `world.get_old()` does not have the latest server snapshot applied.
    /// Old and new gets swapped every time a new queued server snapshot is applied.
    worlds: OldNew<Timestamped<WorldType>>,

    /// The interpolation paramater to blend the `old_world` and `new_world` together into a
    /// single world state. The parameter is in the range [0,1] where 0 represents using only
    /// the `old_world`, and where 1 represents using only the `new_world`.
    old_new_interpolation_t: f32,

    /// The latest interpolated state between `old_world` and `new_world` just before and just
    /// after the current requested render timestamp.
    /// `states.get_old()` is the state just before the requested timestamp.
    /// `states.get_new()` is the state just after the requested timestamp.
    /// Old and new gets swapped every step.
    states: OldNew<WorldType::StateType>,

    /// The number of seconds that `current_state` has overshooted the requested render timestamp.
    timestep_overshoot_seconds: f32,

    /// The interpolation between `previous_state` and `current_state` for the requested render
    /// timestamp.
    display_state: WorldType::StateType,

    /// Buffers user commands from all players that are needed to be replayed after receiving the
    /// server's snapshot, since the server snapshot is behind the actual timestamp.
    command_buffer: BinaryHeap<EarliestPrioritized<WorldType::CommandType>>,

    config: Config,
}

impl<WorldType: World> ActiveClient<WorldType> {
    fn new(config: Config, initial_timestamp: Timestamp, client_id: usize) -> Self {
        let mut client = Self {
            client_id,
            queued_snapshot: None,
            last_queued_snapshot_timestamp: None,
            worlds: OldNew::new(),
            old_new_interpolation_t: 1.0,
            states: OldNew::new(),
            display_state: Default::default(),
            timestep_overshoot_seconds: 0.0,
            command_buffer: Default::default(),
            config,
        };
        let (old_world, new_world) = client.worlds.get_mut();
        old_world.set_timestamp(initial_timestamp);
        new_world.set_timestamp(initial_timestamp);
        client
    }

    fn timestamp(&self) -> Timestamp {
        self.worlds.get().0.timestamp()
    }

    fn update(&mut self, time: &Time, net: &mut NetworkResource) {
        for (_, connection) in net.connections.iter_mut() {
            let channels = connection.channels().unwrap();
            while let Some(command) = channels.recv::<Timestamped<WorldType::CommandType>>() {
                self.receive_command(command);
            }
            while let Some(snapshot) = channels.recv::<Timestamped<WorldType::StateType>>() {
                self.receive_snapshot(snapshot);
            }
        }
        self.advance(time.delta_seconds());
    }

    fn receive_command(&mut self, command: Timestamped<WorldType::CommandType>) {
        info!("Received command");
        let (old_world, new_world) = self.worlds.get_mut();
        old_world.apply_command(command.inner());
        new_world.apply_command(command.inner());
        self.command_buffer.push(command.into());
    }

    fn receive_snapshot(&mut self, snapshot: Timestamped<WorldType::StateType>) {
        info!(
            "Received snapshot: {:?} frames behind",
            self.timestamp() - snapshot.timestamp()
        );
        match &self.last_queued_snapshot_timestamp {
            None => self.queued_snapshot = Some(snapshot),
            Some(last_timestamp) => {
                // Ignore stale snapshots.
                if snapshot.timestamp() > *last_timestamp {
                    self.queued_snapshot = Some(snapshot);
                }
            }
        }
        self.last_queued_snapshot_timestamp =
            Some(self.queued_snapshot.as_ref().unwrap().timestamp());
    }
}

impl<WorldType: World> Stepper for ActiveClient<WorldType> {
    fn step(&mut self) -> f32 {
        trace!("Step...");

        if self.old_new_interpolation_t < 1.0 {
            self.old_new_interpolation_t += self.config.interpolation_progress_per_frame();
            self.old_new_interpolation_t = self.old_new_interpolation_t.clamp(0.0, 1.0);
        } else if let Some(snapshot) = self.queued_snapshot.take() {
            info!("Applying new snapshot from server");
            self.worlds.swap();
            let (old_world, new_world) = self.worlds.get_mut();
            new_world.set_state(snapshot);
            info!(
                "Fastforwarding old snapshot from timestamp {:?} to current timestamp {:?}",
                new_world.timestamp(),
                old_world.timestamp()
            );
            new_world.fast_forward_to_timestamp(&old_world.timestamp(), &mut self.command_buffer);
            self.old_new_interpolation_t = 0.0;
        }

        trace!("Stepping each world by one step");
        let (old_world, new_world) = self.worlds.get_mut();
        old_world.step();
        new_world.step();

        // TODO: Optimizable - the states only need to be updated at the last step of the
        // current advance.
        trace!("Blending the old and new world states");
        self.states.swap();
        self.states
            .set_new(WorldType::StateType::from_interpolation(
                &old_world.state(),
                &new_world.state(),
                self.old_new_interpolation_t,
            ));

        self.config.timestep_seconds
    }
}

impl<WorldType: World> FixedTimestepper for ActiveClient<WorldType> {
    fn advance(&mut self, delta_seconds: f32) {
        trace!("Advancing by {} seconds", delta_seconds);
        self.timestep_overshoot_seconds =
            fixed_timestepper::advance(self, delta_seconds, self.timestep_overshoot_seconds);

        // We display an interpolation between the undershot and overshot states.
        trace!("Interpolating undershot/overshot states");
        let (undershot_state, overshot_state) = self.states.get();
        self.display_state = WorldType::StateType::from_interpolation(
            undershot_state,
            overshot_state,
            1.0 - self.timestep_overshoot_seconds / self.config.timestep_seconds,
        );

        trace!("Done advancing");
    }
}

pub fn client_system<WorldType: World>(
    mut client: ResMut<Client<WorldType>>,
    time: Res<Time>,
    mut net: ResMut<NetworkResource>,
    mut client_connection_events: ResMut<Events<ClientConnectionEvent>>,
) {
    client.update(&*time, &mut *net, &mut *client_connection_events);
}

pub fn client_setup<WorldType: World>(mut net: ResMut<NetworkResource>) {
    let ip_address =
        bevy_networking_turbulence::find_my_ip_address().expect("Cannot find IP address");
    // TODO: Configurable port number and IP address.
    let socket_address = SocketAddr::new(ip_address, 14192);
    info!("Starting client");
    net.connect(socket_address);
}

#[derive(Default)]
pub struct NetworkedPhysicsClientPlugin<WorldType: World> {
    config: Config,
    _world_type: PhantomData<WorldType>,
}

impl<WorldType: World> NetworkedPhysicsClientPlugin<WorldType> {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            _world_type: PhantomData,
        }
    }
}

impl<WorldType: World> Plugin for NetworkedPhysicsClientPlugin<WorldType> {
    fn build(&self, app: &mut AppBuilder) {
        app.add_plugin(NetworkingPlugin)
            .add_event::<ClientConnectionEvent>()
            .add_resource(Client::<WorldType>::new(self.config.clone()))
            .add_startup_system(network_setup::<WorldType>.system())
            .add_startup_system(client_setup::<WorldType>.system())
            .add_system(client_system::<WorldType>.system());
    }
}
