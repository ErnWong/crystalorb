use crate::{
    channels::{network_setup, ClockSyncMessage},
    command::CommandBuffer,
    events::ClientConnectionEvent,
    fixed_timestepper,
    fixed_timestepper::{FixedTimestepper, Stepper},
    old_new::{OldNew, OldNewResult},
    timestamp::{Timestamp, Timestamped},
    world::{DisplayState, World, WorldSimulation},
    Config,
};
use bevy::prelude::*;
use bevy_networking_turbulence::{NetworkResource, NetworkingPlugin};
use std::marker::PhantomData;

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
            trace!("Sending heartbeat");
            net.broadcast_message(ClockSyncMessage {
                client_send_seconds_since_startup: time.seconds_since_startup(),
                server_seconds_since_startup: 0.0,
                client_id: 0,
            });
        }

        let mut server_seconds_offset_sum: f64 = 0.0;
        let mut sample_count: usize = 0;
        let mut client_id: usize = 0;
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
                server_seconds_offset_sum += offset;
                sample_count += 1;
                client_id = sync.client_id;
            }
        }
        let offset_update = if sample_count > 0 {
            Some(server_seconds_offset_sum / sample_count as f64)
        } else {
            None
        };

        let should_transition = match &mut self.state.as_mut().unwrap() {
            ClientState::SyncingInitialTimestamp(client) => client.update(
                time,
                net,
                server_seconds_offset_sum,
                sample_count,
                client_id,
            ),
            ClientState::SyncingInitialState(client) => client.update(time, net, offset_update),
            ClientState::Ready(client) => client.update(time, net, offset_update),
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
    fn update(
        &mut self,
        time: &Time,
        net: &mut NetworkResource,
        server_seconds_offset_addition: f64,
        new_sample_count: usize,
        client_id: usize,
    ) -> bool {
        self.server_seconds_offset_sum += server_seconds_offset_addition;
        self.sample_count += new_sample_count;
        self.client_id = client_id;

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
    fn update(
        &mut self,
        time: &Time,
        net: &mut NetworkResource,
        offset_update: Option<f64>,
    ) -> bool {
        self.client.update(time, net, offset_update);
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
    pub fn last_completed_timestamp(&self) -> Timestamp {
        self.client.last_completed_timestamp()
    }

    pub fn simulating_timestamp(&self) -> Timestamp {
        self.client.simulating_timestamp()
    }

    pub fn client_id(&self) -> usize {
        self.client.client_id
    }

    /// Issue a command from this client's player to the world. The command will be scheduled
    /// to the current simulating timestamp (the previously completed timestamp + 1).
    pub fn issue_command(&mut self, command: WorldType::CommandType, net: &mut NetworkResource) {
        let command = Timestamped::new(command, self.simulating_timestamp());
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

    fn update(
        &mut self,
        time: &Time,
        net: &mut NetworkResource,
        offset_update: Option<f64>,
    ) -> bool {
        self.client.update(time, net, offset_update);
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
    /// An identifier issued by the server for us to identify ourselves from other clients. Used,
    /// for example, for issuing our player's commands to the server.
    client_id: usize,

    /// The difference in seconds between client's time.seconds_since_startup() and server's
    /// time.seconds_since_startup(), where a positive value refers that an earlier client time
    /// value corresponds to the same instant as a later server time value. Since servers start
    /// earlier than clients, this value should in theory always be positive.
    server_seconds_offset: f64,

    /// The next server snapshot that needs applying after the current latest snapshot has been
    /// fully interpolated into.
    queued_snapshot: Option<Timestamped<WorldType::SnapshotType>>,

    /// The timestamp of the last queued snapshot from the server, so we can discard stale
    /// snapshots from the server when the arrive out of order. This persists even after the queued
    /// snapshot has been cleared after it has been applied to the world.
    last_queued_snapshot_timestamp: Option<Timestamp>,

    /// The command buffer that is used to initialize the new world simulation's command
    /// buffers whenever a queued snapshot is applied to it. Contains older commands that the
    /// individual world simulation's internal command buffers would have already dropped, but
    /// would otherwise need to replay onto the server snapshot to get it back to the current
    /// timestamp.
    base_command_buffer: CommandBuffer<WorldType::CommandType>,

    /// The physics world simulation with and without the latest server snapshot applied.
    /// `world_simulation.get().new` has the latest server snapshot applied.
    /// `world_simulation.get().old` does not have the latest server snapshot applied.
    /// Old and new gets swapped every time a new queued server snapshot is applied.
    world_simulations: OldNew<WorldSimulation<WorldType>>,

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
            base_command_buffer: Default::default(),
            world_simulations: OldNew::new(),
            old_new_interpolation_t: 1.0,
            states: OldNew::new(),
            display_state: Default::default(),
            timestep_overshoot_seconds: 0.0,
            config,
        };
        let OldNewResult { old, new } = client.world_simulations.get_mut();
        old.reset_last_completed_timestamp(initial_timestamp);
        new.reset_last_completed_timestamp(initial_timestamp);
        client
    }

    fn last_completed_timestamp(&self) -> Timestamp {
        self.world_simulations.get().old.last_completed_timestamp()
    }

    fn simulating_timestamp(&self) -> Timestamp {
        self.world_simulations.get().old.simulating_timestamp()
    }

    /// Positive refers that our world is ahead of the timestamp it is supposed to be, and
    /// negative refers that our world needs to catchup in the next frame.
    fn timestamp_drift(&self, time: &Time) -> Timestamp {
        let server_time = time.seconds_since_startup() + self.server_seconds_offset;
        self.last_completed_timestamp()
            - Timestamp::from_seconds(server_time, self.config.timestep_seconds)
    }

    /// Positive refers that our world is ahead of the timestamp it is supposed to be, and
    /// negative refers that our world needs to catchup in the next frame.
    fn timestamp_drift_seconds(&self, time: &Time) -> f32 {
        self.timestamp_drift(time)
            .as_seconds(self.config.timestep_seconds)
    }

    fn update(&mut self, time: &Time, net: &mut NetworkResource, offset_update: Option<f64>) {
        if let Some(offset) = offset_update {
            let old_server_seconds_offset = self.server_seconds_offset;
            self.server_seconds_offset +=
                (offset - self.server_seconds_offset) * self.config.clock_offset_update_factor;
            trace!(
                "Client updated its clock offset from {:?} to {:?}",
                old_server_seconds_offset,
                self.server_seconds_offset
            );
        }

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
        trace!("Timestamp drift: {:?}", self.timestamp_drift_seconds(time));
        if -self.timestamp_drift_seconds(time) < -self.config.timestamp_skip_threshold_seconds {
            // Note: only skip on the old world's timestamp.
            // If new world couldn't catch up, then it can simply grab the next server snapshot
            // when it arrives.
            let corrected_timestamp = self.last_completed_timestamp() - self.timestamp_drift(time);
            warn!(
                "Client is too far behind. Skipping timestamp from {:?} to {:?}",
                self.last_completed_timestamp(),
                corrected_timestamp
            );
            self.world_simulations
                .get_mut()
                .old
                .reset_last_completed_timestamp(corrected_timestamp);
        }
    }

    fn receive_command(&mut self, command: Timestamped<WorldType::CommandType>) {
        info!("Received command {:?}", command);
        let OldNewResult { old, new } = self.world_simulations.get_mut();
        self.base_command_buffer.insert(command.clone());
        old.schedule_command(command.clone());
        new.schedule_command(command);
    }

    fn receive_snapshot(&mut self, snapshot: Timestamped<WorldType::SnapshotType>) {
        trace!(
            "Received snapshot: {:?} frames behind",
            self.last_completed_timestamp() - snapshot.timestamp()
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
            let OldNewResult {
                old: old_world_simulation,
                new: new_world_simulation,
            } = self.world_simulations.get();
            let is_fastforwarding = new_world_simulation.last_completed_timestamp()
                < old_world_simulation.last_completed_timestamp();
            trace!(
                "Old New Interpolation t: {:?}",
                self.old_new_interpolation_t
            );
            trace!(
                "Old world timestamp: {:?}",
                old_world_simulation.last_completed_timestamp()
            );
            trace!(
                "New world timestamp: {:?}",
                new_world_simulation.last_completed_timestamp()
            );
            let snapshot_is_newer = self.queued_snapshot.as_ref().map_or(false, |snapshot| {
                trace!("Snapshot timestamp:  {:?}", snapshot.timestamp());
                snapshot.timestamp() > new_world_simulation.last_completed_timestamp()
            });
            let is_interpolating = new_world_simulation.last_completed_timestamp()
                == old_world_simulation.last_completed_timestamp();
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
                    self.world_simulations.swap();
                } else {
                    warn!("Abandoning previous snapshot for newer shapshot! Couldn't fastforward the previous snapshot in time,");
                }

                let OldNewResult {
                    old: old_world_simulation,
                    new: new_world_simulation,
                } = self.world_simulations.get_mut();

                // We can now safely discard commands from the buffer that are older than
                // this server snapshot.
                // Off-by-one check:
                // snapshot has completed the frame at t=snapshot.timestamp(), and therefore has
                // already applied commands that are scheduled for t=snapshot.timestamp().
                self.base_command_buffer.drain_up_to(snapshot.timestamp());

                new_world_simulation
                    .apply_completed_snapshot(snapshot, self.base_command_buffer.clone());

                if new_world_simulation.last_completed_timestamp()
                    > old_world_simulation.last_completed_timestamp()
                {
                    // The server should always be behind the client, even excluding the network
                    // latency. The client may momentarily fall behind due to, e.g., browser tab
                    // sleeping, but once the browser tab wakes up, the client should automatically
                    // compensate, and if necessary, time skip to the correct timestamp to be ahead
                    // of the server. If even then the server continues to be ahead, then it might
                    // suggest that the client and the server's clocks are running at different
                    // rates, and some additional time syncing mechanism is needed.
                    warn!("Server's snapshot is newer than client!");
                }

                // We reset the old/new interpolation factor and begin slowly blending in from the
                // old world to the new world once the new world has caught up (aka
                // "fast-forwarded") to the old world's timestamp.
                self.old_new_interpolation_t = 0.0;
            }
        }

        trace!("Stepping old world by one frame");
        let OldNewResult {
            old: old_world_simulation,
            new: new_world_simulation,
        } = self.world_simulations.get_mut();
        old_world_simulation.step();

        trace!(
            "Fastforwarding new world from timestamp {:?} to current timestamp {:?}",
            new_world_simulation.last_completed_timestamp(),
            old_world_simulation.last_completed_timestamp()
        );
        new_world_simulation.try_completing_simulations_up_to(
            &old_world_simulation.last_completed_timestamp(),
            self.config.fastforward_max_per_step,
        );

        // TODO: Optimizable - the states only need to be updated at the last step of the
        // current advance.
        trace!("Blending the old and new world states");
        self.states.swap();
        self.states
            .set_new(WorldType::DisplayStateType::from_interpolation(
                &old_world_simulation.display_state(),
                &new_world_simulation.display_state(),
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
        let OldNewResult {
            old: undershot_state,
            new: overshot_state,
        } = self.states.get();
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
