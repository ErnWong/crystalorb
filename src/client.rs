use crate::{
    channels::{network_setup, ClockSyncMessage},
    command::CommandBuffer,
    events::ClientConnectionEvent,
    fixed_timestepper,
    fixed_timestepper::{FixedTimestepper, Stepper},
    old_new::OldNew,
    timestamp::{Timestamp, Timestamped},
    world::{DisplayState, World},
    Config,
};
use bevy::prelude::*;
use bevy_networking_turbulence::{NetworkResource, NetworkingPlugin};
use std::{marker::PhantomData, net::SocketAddr};

pub struct Client<WorldType: World> {
    config: Config,
    state: Option<ClientState<WorldType>>,
    seconds_since_last_heartbeat: f32,
}

impl<WorldType: World> Client<WorldType> {
    fn new(config: Config) -> Self {
        Self {
            config: config.clone(),
            state: Some(ClientState::SyncingInitialTimestamp(
                SyncingInitialTimestampClient::new(config),
            )),
            seconds_since_last_heartbeat: 0.0,
        }
    }

    fn update(
        &mut self,
        time: &Time,
        net: &mut NetworkResource,
        client_connection_events: &mut Events<ClientConnectionEvent>,
    ) {
        self.seconds_since_last_heartbeat += time.delta_seconds();
        if self.seconds_since_last_heartbeat > self.config.heartbeat_period {
            self.seconds_since_last_heartbeat = 0.0;
            info!("Sending heartbeat");
            net.broadcast_message(ClockSyncMessage {
                client_send_seconds_since_startup: time.seconds_since_startup(),
                server_seconds_since_startup: 0.0,
                client_id: 0,
            });
        }

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

        // Drop any remaining unhandled clock sync replies from the server.
        // This is, for some reason, to prevent our heartbeats from being blocked.
        // TODO: Use these replies to keep client in sync with server.
        for (_, connection) in net.connections.iter_mut() {
            let channels = connection.channels().unwrap();
            while let Some(_) = channels.recv::<ClockSyncMessage>() {}
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
            if !channels.is_connected() {
                // TODO: rely on this to initiate reconnection? Or rely on timeouts?
                warn!("Channels disconnected!");
            }
            while let Some(sync) = channels.recv::<ClockSyncMessage>() {
                let received_time = time.seconds_since_startup();
                let corresponding_client_time =
                    (sync.client_send_seconds_since_startup + received_time) / 2.0;
                let offset = sync.server_seconds_since_startup - corresponding_client_time;
                trace!(
                    "Received clock sync message. ClientId: {}. Estimated clock offset: {}",
                    sync.client_id,
                    offset,
                );
                self.server_seconds_offset_sum += offset;
                self.sample_count += 1;
                self.client_id = sync.client_id;
            }
        }

        self.seconds_since_last_send += time.delta_seconds();
        if self.seconds_since_last_send > self.config.initial_clock_sync_period {
            self.seconds_since_last_send = 0.0;
            trace!("Sending clock sync message");
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
                SyncingInitialStateClient::new(
                    self.config,
                    initial_timestamp,
                    seconds_offset,
                    self.client_id,
                ),
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
    fn new(
        config: Config,
        initial_timestamp: Timestamp,
        server_seconds_offset: f64,
        client_id: usize,
    ) -> Self {
        info!(
            "Initial timestamp: {:?}, client_id: {}",
            initial_timestamp, client_id
        );
        info!("Syncing initial state");
        Self {
            client: ActiveClient::new(config, initial_timestamp, server_seconds_offset, client_id),
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

    pub fn display_state(&self) -> &WorldType::DisplayStateType {
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

    server_seconds_offset: f64,

    /// The next server snapshot that needs applying after the current latest snapshot has been
    /// fully interpolated into.
    queued_snapshot: Option<Timestamped<WorldType::SnapshotType>>,

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
    states: OldNew<WorldType::DisplayStateType>,

    /// The number of seconds that `current_state` has overshooted the requested render timestamp.
    timestep_overshoot_seconds: f32,

    /// The interpolation between `previous_state` and `current_state` for the requested render
    /// timestamp.
    display_state: WorldType::DisplayStateType,

    /// Buffers user commands from all players that are needed to be replayed after receiving the
    /// server's snapshot, since the server snapshot is behind the actual timestamp.
    command_buffer: CommandBuffer<WorldType::CommandType>,

    config: Config,
}

impl<WorldType: World> ActiveClient<WorldType> {
    fn new(
        config: Config,
        initial_timestamp: Timestamp,
        server_seconds_offset: f64,
        client_id: usize,
    ) -> Self {
        let mut client = Self {
            client_id,
            server_seconds_offset,
            queued_snapshot: None,
            last_queued_snapshot_timestamp: None,
            worlds: OldNew::new(),
            old_new_interpolation_t: 1.0,
            states: OldNew::new(),
            display_state: Default::default(),
            timestep_overshoot_seconds: 0.0,
            command_buffer: CommandBuffer::new(),
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

    /// Positive refers that our world is ahead of the timestamp it is supposed to be, and
    /// negative refers that our world needs to catchup in the next frame.
    fn timestamp_drift(&self, time: &Time) -> Timestamp {
        let server_time = time.seconds_since_startup() + self.server_seconds_offset;
        self.timestamp() - Timestamp::from_seconds(server_time, self.config.timestep_seconds)
    }

    /// Positive refers that our world is ahead of the timestamp it is supposed to be, and
    /// negative refers that our world needs to catchup in the next frame.
    fn timestamp_drift_seconds(&self, time: &Time) -> f32 {
        self.timestamp_drift(time)
            .as_seconds(self.config.timestep_seconds)
    }

    fn update(&mut self, time: &Time, net: &mut NetworkResource) {
        for (_, connection) in net.connections.iter_mut() {
            let channels = connection.channels().unwrap();
            if !channels.is_connected() {
                // TODO: rely on this to initiate reconnection? Or rely on timeouts?
                warn!("Channels disconnected!");
            }
            while let Some(command) = channels.recv::<Timestamped<WorldType::CommandType>>() {
                self.receive_command(command);
            }
            while let Some(snapshot) = channels.recv::<Timestamped<WorldType::SnapshotType>>() {
                self.receive_snapshot(snapshot);
            }
        }

        // Compensate for any drift.
        // TODO: Remove duplicate code between client and server.
        let next_delta_seconds = (time.delta_seconds() - self.timestamp_drift_seconds(time))
            .clamp(0.0, self.config.update_delta_seconds_max);

        self.advance(next_delta_seconds);

        // If drift is too large and we still couldn't keep up, do a time skip.
        if self.timestamp_drift_seconds(time) < -self.config.timestamp_skip_threshold_seconds {
            // Note: only skip on the old world's timestamp.
            // If new world couldn't catch up, then it can simply grab the next server snapshot
            // when it arrives.
            let corrected_timestamp = self.timestamp() - self.timestamp_drift(time);
            warn!(
                "Client is too far behind. Skipping timestamp from {:?} to {:?}",
                self.timestamp(),
                corrected_timestamp
            );
            self.worlds.get_mut().0.set_timestamp(corrected_timestamp);
        }
    }

    fn receive_command(&mut self, command: Timestamped<WorldType::CommandType>) {
        info!("Received command {:?}", command);
        let (old_world, new_world) = self.worlds.get_mut();
        old_world.apply_command(command.inner());
        new_world.apply_command(command.inner());
        self.command_buffer.insert(command);
    }

    fn receive_snapshot(&mut self, snapshot: Timestamped<WorldType::SnapshotType>) {
        trace!(
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
        if let Some(queued_snapshot) = &self.queued_snapshot {
            self.last_queued_snapshot_timestamp = Some(queued_snapshot.timestamp());
        }
    }
}

impl<WorldType: World> Stepper for ActiveClient<WorldType> {
    fn step(&mut self) -> f32 {
        trace!("Step...");

        // Figure out what state we are in.
        // TODO: This logic needs tidying up and untangling.
        let has_finished_interpolating_to_new_world = self.old_new_interpolation_t >= 1.0;
        let (is_fastforwarding_and_snapshot_is_newer, is_interpolating) = {
            let (old_world, new_world) = self.worlds.get();
            let is_fastforwarding = new_world.timestamp() < old_world.timestamp();
            let snapshot_is_newer = self.queued_snapshot.as_ref().map_or(false, |snapshot| {
                snapshot.timestamp() > new_world.timestamp()
            });
            let is_interpolating = new_world.timestamp() == old_world.timestamp();
            (is_fastforwarding && snapshot_is_newer, is_interpolating)
        };

        // Note: Don't progress with interpolation if new world is still fast forwarding or is
        // ahead of old world.
        if is_interpolating && !has_finished_interpolating_to_new_world {
            self.old_new_interpolation_t += self.config.interpolation_progress_per_frame();
            self.old_new_interpolation_t = self.old_new_interpolation_t.clamp(0.0, 1.0);
        } else if is_fastforwarding_and_snapshot_is_newer || has_finished_interpolating_to_new_world
        {
            if let Some(snapshot) = self.queued_snapshot.take() {
                trace!("Applying new snapshot from server");

                if has_finished_interpolating_to_new_world {
                    self.worlds.swap();
                } else {
                    warn!("Abandoning previous snapshot for newer shapshot! Couldn't fastforward the previous snapshot in time,");
                }

                let (old_world, new_world) = self.worlds.get_mut();
                new_world.apply_snapshot(snapshot);

                // We can now safely discard commands from the buffer that are older than
                // this server snapshot.
                self.command_buffer.discard_old(new_world.timestamp());

                if new_world.timestamp() > old_world.timestamp() {
                    warn!("Server's snapshot is newer than client!");
                }
                self.old_new_interpolation_t = 0.0;
            }
        }

        trace!("Stepping old world by one frame");
        let (old_world, new_world) = self.worlds.get_mut();
        old_world.step();

        trace!(
            "Fastforwarding new world from timestamp {:?} to current timestamp {:?}",
            new_world.timestamp(),
            old_world.timestamp()
        );
        new_world.fast_forward_to_timestamp(
            &old_world.timestamp(),
            &self.command_buffer,
            self.config.fastforward_max_per_step,
        );

        // TODO: Optimizable - the states only need to be updated at the last step of the
        // current advance.
        trace!("Blending the old and new world states");
        self.states.swap();
        self.states
            .set_new(WorldType::DisplayStateType::from_interpolation(
                &old_world.display_state(),
                &new_world.display_state(),
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
        self.display_state = WorldType::DisplayStateType::from_interpolation(
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

pub struct EndpointUrl(pub String);

pub fn client_setup<WorldType: World>(
    mut net: ResMut<NetworkResource>,
    endpoint_url: Res<EndpointUrl>,
) {
    // let socket_address = "http://dango-daikazoku.herokuapp.com/join/1234";
    // let socket_address = "http://192.168.1.9:8080/join/1234";
    info!("Starting client - connecting to {}", endpoint_url.0.clone());
    net.connect(endpoint_url.0.clone());
}

#[derive(Default)]
pub struct NetworkedPhysicsClientPlugin<WorldType: World> {
    config: Config,
    endpoint_url: String,
    _world_type: PhantomData<WorldType>,
}

impl<WorldType: World> NetworkedPhysicsClientPlugin<WorldType> {
    pub fn new(config: Config, endpoint_url: String) -> Self {
        Self {
            config,
            endpoint_url,
            _world_type: PhantomData,
        }
    }
}

impl<WorldType: World> Plugin for NetworkedPhysicsClientPlugin<WorldType> {
    fn build(&self, app: &mut AppBuilder) {
        app.add_plugin(NetworkingPlugin::default())
            .add_event::<ClientConnectionEvent>()
            .add_resource(Client::<WorldType>::new(self.config.clone()))
            .add_resource(EndpointUrl(self.endpoint_url.clone()))
            .add_startup_system(network_setup::<WorldType>.system())
            .add_startup_system(client_setup::<WorldType>.system())
            .add_system(client_system::<WorldType>.system());
    }
}
