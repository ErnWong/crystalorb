use crate::{
    clocksync::ClockSyncer,
    command::CommandBuffer,
    fixed_timestepper::{FixedTimestepper, Stepper, TerminationCondition, TimeKeeper},
    network_resource::{Connection, NetworkResource},
    old_new::{OldNew, OldNewResult},
    timestamp::{Timestamp, Timestamped},
    world::{DisplayState, Tweened, World, WorldSimulation},
    Config,
};
use tracing::{debug, info, trace, warn};

pub struct Client<WorldType: World> {
    config: Config,
    state: Option<ClientState<WorldType>>,
}

impl<WorldType: World> Client<WorldType> {
    pub fn new(config: Config) -> Self {
        Self {
            config: config.clone(),
            state: Some(ClientState::SyncingClock(SyncingClockClient(
                ClockSyncer::new(config),
            ))),
        }
    }

    pub fn update<NetworkResourceType: NetworkResource>(
        &mut self,
        delta_seconds: f64,
        seconds_since_startup: f64,
        net: &mut NetworkResourceType,
    ) {
        let should_transition = match &mut self.state.as_mut().unwrap() {
            ClientState::SyncingClock(SyncingClockClient(clocksyncer)) => {
                clocksyncer.update(delta_seconds, seconds_since_startup, net);
                clocksyncer.is_ready()
            }
            ClientState::SyncingInitialState(SyncingInitialStateClient(client)) => {
                client.update(delta_seconds, seconds_since_startup, net);
                client.is_ready()
            }
            ClientState::Ready(ReadyClient(client)) => {
                client.update(delta_seconds, seconds_since_startup, net);
                false
            }
        };
        if should_transition {
            self.state = Some(match self.state.take().unwrap() {
                ClientState::SyncingClock(SyncingClockClient(clocksyncer)) => {
                    ClientState::SyncingInitialState(SyncingInitialStateClient(ActiveClient::new(
                        seconds_since_startup,
                        self.config.clone(),
                        clocksyncer,
                    )))
                }
                ClientState::SyncingInitialState(SyncingInitialStateClient(client)) => {
                    ClientState::Ready(ReadyClient(client))
                }
                ClientState::Ready(_) => unreachable!(),
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
    SyncingClock(SyncingClockClient),
    SyncingInitialState(SyncingInitialStateClient<WorldType>),
    Ready(ReadyClient<WorldType>),
}

pub struct SyncingClockClient(ClockSyncer);

impl SyncingClockClient {
    pub fn sample_count(&self) -> usize {
        self.0.sample_count()
    }

    pub fn samples_needed(&self) -> usize {
        self.0.samples_needed()
    }
}

pub struct SyncingInitialStateClient<WorldType: World>(ActiveClient<WorldType>);

impl<WorldType: World> SyncingInitialStateClient<WorldType> {
    pub fn last_completed_timestamp(&self) -> Timestamp {
        self.0.last_completed_timestamp()
    }

    pub fn simulating_timestamp(&self) -> Timestamp {
        self.0.simulating_timestamp()
    }
}

pub struct ReadyClient<WorldType: World>(ActiveClient<WorldType>);

impl<WorldType: World> ReadyClient<WorldType> {
    pub fn last_completed_timestamp(&self) -> Timestamp {
        self.0.last_completed_timestamp()
    }

    pub fn simulating_timestamp(&self) -> Timestamp {
        self.0.simulating_timestamp()
    }

    pub fn client_id(&self) -> usize {
        self.0
            .clocksyncer
            .client_id()
            .expect("Client should be connected by the time it is ready")
    }

    /// Issue a command from this client's player to the world. The command will be scheduled
    /// to the current simulating timestamp (the previously completed timestamp + 1).
    pub fn issue_command<NetworkResourceType: NetworkResource>(
        &mut self,
        command: WorldType::CommandType,
        net: &mut NetworkResourceType,
    ) {
        let command = Timestamped::new(command, self.simulating_timestamp());
        self.0
            .timekeeping_simulations
            .receive_command(command.clone());
        net.broadcast_message(command);
    }

    pub fn buffered_commands(
        &self,
    ) -> impl Iterator<Item = (Timestamp, &Vec<WorldType::CommandType>)> {
        self.0.timekeeping_simulations.base_command_buffer.iter()
    }

    pub fn display_state(&self) -> &Tweened<WorldType::DisplayStateType> {
        &self
            .0
            .timekeeping_simulations
            .display_state
            .as_ref()
            .expect("Client should be initialised")
    }

    /// The timestamp used to test whether the next snapshot to be received is newer or older, and
    /// therefore should be discarded or queued.
    /// None if no snapshots have been received yet.
    pub fn last_queued_snapshot_timestamp(&self) -> &Option<Timestamp> {
        &self
            .0
            .timekeeping_simulations
            .last_queued_snapshot_timestamp
    }

    /// The timestamp of the most recently received snapshot, regardless of whether it got queued
    /// or discarded.
    /// None if no spashots have been received yet.
    pub fn last_received_snapshot_timestamp(&self) -> &Option<Timestamp> {
        &self
            .0
            .timekeeping_simulations
            .last_received_snapshot_timestamp
    }
}

pub struct ActiveClient<WorldType: World> {
    clocksyncer: ClockSyncer,

    timekeeping_simulations:
        TimeKeeper<ClientWorldSimulations<WorldType>, { TerminationCondition::FirstOvershoot }>,
}

impl<WorldType: World> ActiveClient<WorldType> {
    fn new(seconds_since_startup: f64, config: Config, clocksyncer: ClockSyncer) -> Self {
        let server_time = clocksyncer
            .server_seconds_since_startup(seconds_since_startup)
            .expect("Active client can only be constructed with a synchronized clock");

        let initial_timestamp = Timestamp::from_seconds(server_time, config.timestep_seconds);

        info!(
            "Initial timestamp: {:?}, client_id: {}",
            initial_timestamp,
            clocksyncer
                .client_id()
                .expect("Active client can only be constructed once connected"),
        );

        Self {
            clocksyncer,
            timekeeping_simulations: TimeKeeper::new(
                ClientWorldSimulations::new(config.clone(), initial_timestamp),
                config,
            ),
        }
    }

    fn last_completed_timestamp(&self) -> Timestamp {
        self.timekeeping_simulations.last_completed_timestamp()
    }

    fn simulating_timestamp(&self) -> Timestamp {
        self.last_completed_timestamp() + 1
    }

    pub fn update<NetworkResourceType: NetworkResource>(
        &mut self,
        delta_seconds: f64,
        seconds_since_startup: f64,
        net: &mut NetworkResourceType,
    ) {
        self.clocksyncer
            .update(delta_seconds, seconds_since_startup, net);

        for (_, mut connection) in net.connections() {
            while let Some(command) = connection.recv::<Timestamped<WorldType::CommandType>>() {
                self.timekeeping_simulations.receive_command(command);
            }
            while let Some(snapshot) = connection.recv::<Timestamped<WorldType::SnapshotType>>() {
                self.timekeeping_simulations.receive_snapshot(snapshot);
            }
        }

        trace!(
            "client server clock offset {}",
            self.clocksyncer
                .server_seconds_offset()
                .expect("Clock should be synced")
        );
        self.timekeeping_simulations.update(
            delta_seconds,
            self.clocksyncer
                .server_seconds_since_startup(seconds_since_startup)
                .expect("Clock should be synced")
                + self.timekeeping_simulations.config.lag_compensation_latency,
        );
    }

    pub fn is_ready(&self) -> bool {
        self.timekeeping_simulations.display_state.is_some()
    }
}

pub struct ClientWorldSimulations<WorldType: World> {
    /// The next server snapshot that needs applying after the current latest snapshot has been
    /// fully interpolated into.
    queued_snapshot: Option<Timestamped<WorldType::SnapshotType>>,

    /// The timestamp of the last queued snapshot from the server, so we can discard stale
    /// snapshots from the server when the arrive out of order. This persists even after the queued
    /// snapshot has been cleared after it has been applied to the world.
    last_queued_snapshot_timestamp: Option<Timestamp>,

    /// The timestamp of the last received snapshot from the server, regardless of whether it
    /// was discarded or accepted (since we only keep the latest snapshot). This is primarily
    /// for diagnostic purposes.
    last_received_snapshot_timestamp: Option<Timestamp>,

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
    /// single world state. The parameter is in the range `[0,1]` where 0 represents using only
    /// the `old_world`, and where 1 represents using only the `new_world`.
    old_new_interpolation_t: f64,

    /// The latest interpolated state between `old_world` and `new_world` just before and just
    /// after the current requested render timestamp.
    /// `states.get_old()` is the state just before the requested timestamp.
    /// `states.get_new()` is the state just after the requested timestamp.
    /// Old and new gets swapped every step.
    /// They are None until the first world simulation state that is based on a server snapshot is
    /// ready to be "published".
    states: OldNew<Option<Timestamped<WorldType::DisplayStateType>>>,

    /// The interpolation between `previous_state` and `current_state` for the requested render
    /// timestamp. This remains None until the client is initialised with the server's snapshots.
    display_state: Option<Tweened<WorldType::DisplayStateType>>,

    config: Config,
}

impl<WorldType: World> ClientWorldSimulations<WorldType> {
    pub fn new(config: Config, initial_timestamp: Timestamp) -> Self {
        let mut client_world_simulations = Self {
            queued_snapshot: None,
            last_queued_snapshot_timestamp: None,
            last_received_snapshot_timestamp: None,
            base_command_buffer: Default::default(),
            world_simulations: OldNew::new(),
            old_new_interpolation_t: 1.0,
            states: OldNew::new(),
            display_state: Default::default(),
            config,
        };
        let OldNewResult { old, new } = client_world_simulations.world_simulations.get_mut();
        old.reset_last_completed_timestamp(initial_timestamp);
        new.reset_last_completed_timestamp(initial_timestamp);
        client_world_simulations
    }

    fn receive_command(&mut self, command: Timestamped<WorldType::CommandType>) {
        debug!("Received command {:?}", command);
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

        self.last_received_snapshot_timestamp = Some(snapshot.timestamp());
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

impl<WorldType: World> Stepper for ClientWorldSimulations<WorldType> {
    fn step(&mut self) {
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
                "Old world current simulating timestamp: {:?}",
                old_world_simulation.simulating_timestamp()
            );
            trace!(
                "New world current simulating timestamp: {:?}",
                new_world_simulation.simulating_timestamp()
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

        assert!(is_interpolating || self.old_new_interpolation_t == 0.0,
            "Interpolation parameter t should be reset and not advanced if the new world has not caught up to the new world yet");

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

        let is_first_initialised_frame = self.last_queued_snapshot_timestamp.is_some()
            && self.states.get().new.is_none()
            && new_world_simulation.last_completed_timestamp()
                == old_world_simulation.last_completed_timestamp();
        let had_already_initialised = self.states.get().new.is_some();

        if is_first_initialised_frame {
            // The old world simulation contains the uninitialised state while the new
            // world simulation is based on the first snapshot from the server.
            // Therefore, we can skip the interpolation and jump straight to the new
            // world state.
            self.old_new_interpolation_t = 1.0;
        }

        if is_first_initialised_frame || had_already_initialised {
            // TODO: Optimizable - the states only need to be updated at the last step of the
            // current advance.
            trace!("Blending the old and new world states");
            self.states.swap();
            self.states.set_new(Some(
                Timestamped::<WorldType::DisplayStateType>::from_interpolation(
                    &old_world_simulation.display_state(),
                    &new_world_simulation.display_state(),
                    self.old_new_interpolation_t,
                ),
            ));
        }
    }
}

impl<WorldType: World> FixedTimestepper for ClientWorldSimulations<WorldType> {
    fn last_completed_timestamp(&self) -> Timestamp {
        self.world_simulations.get().old.last_completed_timestamp()
    }

    fn reset_last_completed_timestamp(&mut self, corrected_timestamp: Timestamp) {
        let OldNewResult {
            old: old_world_simulation,
            new: new_world_simulation,
        } = self.world_simulations.get_mut();

        if new_world_simulation.last_completed_timestamp()
            == old_world_simulation.last_completed_timestamp()
        {
            new_world_simulation.reset_last_completed_timestamp(corrected_timestamp);
        }

        old_world_simulation.reset_last_completed_timestamp(corrected_timestamp);
    }

    fn post_update(&mut self, timestep_overshoot_seconds: f64) {
        // We display an interpolation between the undershot and overshot states.
        trace!("Interpolating undershot/overshot states (aka tweening)");
        let OldNewResult {
            old: optional_undershot_state,
            new: optional_overshot_state,
        } = self.states.get();
        let tween_t = self
            .config
            .tweening_method
            .shape_interpolation_t(1.0 - timestep_overshoot_seconds / self.config.timestep_seconds);
        trace!("tween_t {}", tween_t);
        if let Some(undershot_state) = optional_undershot_state {
            if let Some(overshot_state) = optional_overshot_state {
                self.display_state = Some(Tweened::from_interpolation(
                    undershot_state,
                    overshot_state,
                    tween_t,
                ));
            }
        }

        trace!("Update the base command buffer's timestamp and accept-window");
        self.base_command_buffer
            .update_timestamp(self.last_completed_timestamp());
    }
}
