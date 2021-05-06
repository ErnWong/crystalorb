//! This module houses the structures that are specific to the management of a CrystalOrb game
//! client.
//!
//! The [`Client`] structure is what you create, store, and update, analogous to the
//! [`Server`](crate::server::Server) structure. The other structures in this module are what you
//! access indirectly through your [`Client`] to get additional functionality, depending on what
//! loading stage the [`Client`] is at (via [`Client::stage`] or [`Client::stage_mut`]).
//!
//! For the majority of the time, you would probably want to access the [`ReadyClient`] in order
//! to:
//!
//! - [issue commands](ReadyClient::issue_command)
//!     ```
//!     use crystalorb::{Config, client::{Client, ClientStage}};
//!     use crystalorb_demo::{DemoWorld, DemoCommand, PlayerSide, PlayerCommand};
//!     use crystalorb_mock_network::MockNetwork;
//!
//!     // Using a mock network as an example...
//!     let (_, (mut network, _)) =
//!         MockNetwork::new_mock_network::<DemoWorld>();
//!
//!     let mut client = Client::<DemoWorld>::new(Config::new());
//!
//!     // ...later on in your update loop, in response to a player input...
//!
//!     if let ClientStage::Ready(ready_client) = client.stage_mut() {
//!         let command = DemoCommand::new(PlayerSide::Left, PlayerCommand::Jump, true);
//!         ready_client.issue_command(command, &mut network);
//!     }
//!     ```
//! - and [get the display
//! state](ReadyClient::display_state) to render on the screen.
//!     ```
//!     use crystalorb::{
//!         Config,
//!         client::{Client, ClientStage},
//!         world::DisplayState,
//!     };
//!     use crystalorb_demo::{DemoWorld, DemoDisplayState};
//!
//!     fn render(display_state: &DemoDisplayState) {
//!         // [Your game rendering code]
//!         println!("{:?}", display_state);
//!     }
//!
//!     let client = Client::<DemoWorld>::new(Config::new());
//!
//!     // ...later on in your update loop...
//!
//!     if let ClientStage::Ready(ready_client) = client.stage() {
//!         render(ready_client.display_state().display_state());
//!     }
//!     ```

use crate::{
    clocksync::ClockSyncer,
    command::CommandBuffer,
    fixed_timestepper::{FixedTimestepper, Stepper, TerminationCondition, TimeKeeper},
    network_resource::{Connection, NetworkResource},
    old_new::{OldNew, OldNewResult},
    timestamp::{Timestamp, Timestamped},
    world::{DisplayState, InitializationType, Tweened, World, WorldSimulation},
    Config,
};
use std::fmt::{Display, Formatter};
use tracing::{debug, info, trace, warn};

/// This is the top-level structure of CrystalOrb for your game client, analogous to the
/// [`Server`](crate::server::Server) for game servers. You create, store, and update this client
/// instance to run your game on the client side.
#[derive(Debug)]
pub struct Client<WorldType: World> {
    config: Config,
    stage: ClientStage<WorldType>,
}

impl<WorldType: World> Client<WorldType> {
    /// Constructs a new [`Client`].
    ///
    /// # Examples
    ///
    /// Using the default configuration parameters:
    ///
    /// ```
    /// use crystalorb::{Config, client::Client};
    /// use crystalorb_demo::DemoWorld;
    ///
    /// let client = Client::<DemoWorld>::new(Config::new());
    /// ```
    ///
    /// Overriding some configuration parameter:
    ///
    /// ```
    /// use crystalorb::{Config, client::Client};
    /// use crystalorb_demo::DemoWorld;
    ///
    /// let client = Client::<DemoWorld>::new(Config {
    ///     lag_compensation_latency: 0.5,
    ///     ..Config::new()
    /// });
    /// ```
    pub fn new(config: Config) -> Self {
        Self {
            config: config.clone(),
            stage: ClientStage::SyncingClock(SyncingClockClient(ClockSyncer::new(config))),
        }
    }

    /// Perform the next update for the current rendering frame. You would typically call this in
    /// your game engine's update loop of some kind.
    ///
    /// # Examples
    ///
    /// Here is how one might call [`Client::update`] outside of any game engine:
    ///
    /// ```
    /// use crystalorb::{Config, client::Client};
    /// use crystalorb_demo::DemoWorld;
    /// use crystalorb_mock_network::MockNetwork;
    /// use std::time::Instant;
    ///
    /// // Using a mock network as an example...
    /// let (_, (mut network, _)) =
    ///    MockNetwork::new_mock_network::<DemoWorld>();
    ///
    /// let mut client = Client::<DemoWorld>::new(Config::new());
    /// let startup_time = Instant::now();
    /// let mut previous_time = Instant::now();
    ///
    /// loop {
    ///     let current_time = Instant::now();
    ///     let delta_seconds = current_time.duration_since(previous_time).as_secs_f64();
    ///     let seconds_since_startup = current_time.duration_since(startup_time).as_secs_f64();
    ///
    ///     client.update(delta_seconds, seconds_since_startup, &mut network);
    ///
    ///     // ...Other update code omitted...
    ///     # break;
    /// }
    /// ```
    pub fn update<NetworkResourceType: NetworkResource>(
        &mut self,
        delta_seconds: f64,
        seconds_since_startup: f64,
        net: &mut NetworkResourceType,
    ) {
        let positive_delta_seconds = delta_seconds.max(0.0);
        #[allow(clippy::float_cmp)]
        if delta_seconds != positive_delta_seconds {
            warn!(
                "Attempted to update client with a negative delta_seconds {}. Clamping it to zero.",
                delta_seconds
            );
        }
        let should_transition = match &mut self.stage {
            ClientStage::SyncingClock(SyncingClockClient(clocksyncer)) => {
                clocksyncer.update(positive_delta_seconds, seconds_since_startup, net);
                clocksyncer.is_ready()
            }
            ClientStage::SyncingInitialState(SyncingInitialStateClient(client)) => {
                client.update(positive_delta_seconds, seconds_since_startup, net);
                client.is_ready()
            }
            ClientStage::Ready(ReadyClient(client)) => {
                client.update(positive_delta_seconds, seconds_since_startup, net);
                false
            }
        };
        if should_transition {
            let config = self.config.clone();
            take_mut::take(&mut self.stage, |stage| match stage {
                ClientStage::SyncingClock(SyncingClockClient(clocksyncer)) => {
                    ClientStage::SyncingInitialState(SyncingInitialStateClient(ActiveClient::new(
                        seconds_since_startup,
                        config,
                        clocksyncer,
                    )))
                }
                ClientStage::SyncingInitialState(SyncingInitialStateClient(client)) => {
                    ClientStage::Ready(ReadyClient(client))
                }
                ClientStage::Ready(_) => unreachable!(),
            });
        }
    }

    /// Get the current stage of the [`Client`], which provides access to extra functionality
    /// depending on what stage it is currently in.
    ///
    /// # Example
    ///
    /// To check on the client's progress at connecting with the server, you can check the number
    /// of clocksync samples during the client's [`SyncingClock`](ClientStage::SyncingClock)
    /// stage:
    ///
    /// ```
    /// use crystalorb::{Config, client::{Client, ClientStage}};
    /// use crystalorb_demo::DemoWorld;
    ///
    /// let client = Client::<DemoWorld>::new(Config::new());
    ///
    /// // ...Later on...
    ///
    /// if let ClientStage::SyncingClock(syncing_clock_client) = client.stage() {
    ///     println!(
    ///         "Connection progress: {}%",
    ///         syncing_clock_client.sample_count() as f64
    ///             / syncing_clock_client.samples_needed() as f64 * 100.0
    ///     );
    /// }
    /// ```
    pub fn stage(&self) -> &ClientStage<WorldType> {
        &self.stage
    }

    /// Get the current stage f the [`Client`], which provides access to extra functionality
    /// depending on what stage it is currently in.
    ///
    /// # Example
    ///
    /// To issue a command, you'll need to access the client in its [`Ready`](ClientStage::Ready)
    /// stage:
    ///
    /// ```
    /// use crystalorb::{Config, client::{Client, ClientStage}};
    /// use crystalorb_demo::{DemoWorld, DemoCommand, PlayerSide, PlayerCommand};
    /// use crystalorb_mock_network::MockNetwork;
    ///
    /// // Using a mock network as an example...
    /// let (_, (mut network, _)) =
    ///    MockNetwork::new_mock_network::<DemoWorld>();
    ///
    /// let mut client = Client::<DemoWorld>::new(Config::new());
    ///
    /// // ...Later on...
    ///
    /// if let ClientStage::Ready(ready_client) = client.stage_mut() {
    ///     let command = DemoCommand::new(PlayerSide::Left, PlayerCommand::Jump, true);
    ///     ready_client.issue_command(command, &mut network);
    /// }
    /// ```
    pub fn stage_mut(&mut self) -> &mut ClientStage<WorldType> {
        &mut self.stage
    }
}

/// The [`Client`] undergoes several stages of initialization before it is ready to accept commands
/// and be displayed to the player's screen. This enum provides access to different functionality
/// depending on what stage the [`Client`] is at.
#[derive(Debug)]
pub enum ClientStage<WorldType: World> {
    /// In this first stage, the [`Client`] tries to figure out the clock differences between the
    /// local machine and the server machine. The [`Client`] collects a handful of "clock offset"
    /// samples until it reaches the amount needed to make a good estimate. This timing difference
    /// is necessary so that the [`Client`] can talk to the [`Server`](crate::server::Server) about
    /// the timing of player commands and snapshots.
    ///
    /// You can observe the clock syncing progress by checking the [number of samples collected so
    /// far](SyncingClockClient::sample_count) and comparing that number with the [number of
    /// samples that are needed](SyncingClockClient::samples_needed).
    SyncingClock(SyncingClockClient),

    /// The second stage is where the [`Client`] waits for the first
    /// [`Server`](crate::server::Server) [snapshot](World::SnapshotType) to arrive, fastforwarded,
    /// and generally be fully processed through the client pipeline so that it can be shown on the
    /// screen, to flush out all the uninitialized simulation state.
    SyncingInitialState(SyncingInitialStateClient<WorldType>),

    /// The third and final stage is where the [`Client`] is now ready for its [display
    /// state](ReadyClient::display_state) to be shown onto the screen, and where the [`Client`] is
    /// ready to accept commands from the user.
    Ready(ReadyClient<WorldType>),
}

/// The client interface while the client is in the initial clock syncing stage.
#[derive(Debug)]
pub struct SyncingClockClient(ClockSyncer);

impl SyncingClockClient {
    /// The number of clock offset samples collected so far.
    pub fn sample_count(&self) -> usize {
        self.0.sample_count()
    }

    /// The number of clock offset samples needed to make a good estimate on the timing differences
    /// between the client and the server.
    pub fn samples_needed(&self) -> usize {
        self.0.samples_needed()
    }
}

/// The client interface while the client is in the initial state syncing stage.
#[derive(Debug)]
pub struct SyncingInitialStateClient<WorldType: World>(ActiveClient<WorldType>);

impl<WorldType: World> SyncingInitialStateClient<WorldType> {
    /// The timestamp of the most recent frame that has completed its simulation.
    /// This is typically one less than [`SyncingInitialStateClient::simulating_timestamp`].
    pub fn last_completed_timestamp(&self) -> Timestamp {
        self.0.last_completed_timestamp()
    }

    /// The timestamp of the frame that is *in the process* of being simulated.
    /// This is typically one more than [`SyncingInitialStateClient::simulating_timestamp`].
    pub fn simulating_timestamp(&self) -> Timestamp {
        self.0.simulating_timestamp()
    }
}

/// The client interface once the client is in the "ready" stage.
#[derive(Debug)]
pub struct ReadyClient<WorldType: World>(ActiveClient<WorldType>);

impl<WorldType: World> ReadyClient<WorldType> {
    /// The timestamp of the most recent frame that has completed its simulation.
    /// This is typically one less than [`ReadyClient::simulating_timestamp`].
    pub fn last_completed_timestamp(&self) -> Timestamp {
        self.0.last_completed_timestamp()
    }

    /// The timestamp of the frame that is *in the process* of being simulated.
    /// This is typically one more than [`ReadyClient::simulating_timestamp`].
    ///
    /// This is also the timestamp that gets attached to the command when you call
    /// [`ReadyClient::issue_command`].
    pub fn simulating_timestamp(&self) -> Timestamp {
        self.0.simulating_timestamp()
    }

    /// A number that is used to identify the client among all the clients connected to the server.
    /// This number may be useful, for example, to identify which piece of world state belongs to
    /// which player.
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
        let timestamped_command = Timestamped::new(command, self.simulating_timestamp());
        self.0
            .timekeeping_simulations
            .receive_command(&timestamped_command);
        net.broadcast_message(timestamped_command);
    }

    /// Iterate through the commands that are being kept around. This is intended to be for
    /// diagnostic purposes.
    pub fn buffered_commands(
        &self,
    ) -> impl Iterator<Item = (Timestamp, &Vec<WorldType::CommandType>)> {
        self.0.timekeeping_simulations.base_command_buffer.iter()
    }

    /// Get the current display state that can be used to render the client's screen.
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
    ///
    /// This value gets updated if it gets too old, even if there hasn't been any newer snapshot
    /// received. This is because we need to compare newly-received snapshots with this value, but
    /// we can't compare Timestamps if they are outside the
    /// [comparable range](Timestamp::comparable_range_with_midpoint).
    ///
    /// None if no snapshots have been received yet.
    pub fn last_queued_snapshot_timestamp(&self) -> &Option<Timestamp> {
        &self
            .0
            .timekeeping_simulations
            .last_queued_snapshot_timestamp
    }

    /// The timestamp of the most recently received snapshot, regardless of whether it got queued
    /// or discarded.
    ///
    /// Unlike [`ReadyClient::last_queued_snapshot_timestamp`], this does not get updated when it
    /// becomes too old to be compared with the current timestamp. This is primarily used for
    /// diagnostic purposes.
    ///
    /// None if no spashots have been received yet.
    pub fn last_received_snapshot_timestamp(&self) -> &Option<Timestamp> {
        &self
            .0
            .timekeeping_simulations
            .last_received_snapshot_timestamp
    }

    /// Useful diagnostic to see what stage of the server reconciliation process that the
    /// client is currently at. For more information, refer to [`ReconciliationStatus`].
    pub fn reconciliation_status(&self) -> ReconciliationStatus {
        self.0
            .timekeeping_simulations
            .infer_current_reconciliation_status()
    }
}

#[derive(Debug)]
struct ActiveClient<WorldType: World> {
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
                self.timekeeping_simulations.receive_command(&command);
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

    /// Whether all the uninitialized state has been flushed out, and that the first display state
    /// is available to be shown to the client's screen.
    pub fn is_ready(&self) -> bool {
        self.timekeeping_simulations.display_state.is_some()
    }
}

/// The client needs to perform different behaviours at different times. For example, the client
/// cannot reconcile with the server before receing a snapshot from the server. The client cannot
/// blend the snapshot into the current world state until the snapshot state has been fastforwarded
/// to the correct timestamp (since server snapshots are always behind the client's state).
#[derive(Debug)]
pub enum ReconciliationStatus {
    /// This is the status when the previous snapshot has been fully blended in, and the client is
    /// now waiting for the next snapshot to be applied.
    AwaitingSnapshot,

    /// This is the status when a snapshot is taken from the snapshot queue and the client is now
    /// in the process of bringing that snapshot state on par with the client's existing timestamp.
    Fastforwarding(FastforwardingHealth),

    /// This is the status when the snapshot timestamp now matches the client's timestamp, and the
    /// client is now in the process of blending the snapshot into the client's display state.
    /// The `f64` value describes the current interpolation parameter used for blending the old and
    /// new display states together, ranging from `0.0` (meaning use the old state) to `1.0`
    /// (meaning use the new state).
    Blending(f64),
}

/// Fastforwarding the server's snapshot to the current timestamp can take multiple update cycles.
/// During this time, different external situations can cause the fastfowarding process to abort in
/// an unexpected way. This enum describes these possible situations.
#[derive(Debug)]
pub enum FastforwardingHealth {
    /// Exactly as the name implies. Fastforwarding continues as normal.
    Healthy,

    /// If a new server snapshot arrives before the current server snapshot has finished
    /// fastfowarding, and for some reason this server snapshot has a timestamp newer than the
    /// current fastforwarded snapshot timestamp, then we mark the current fastforwarded snapshot
    /// as obsolete as it is even further behind than the latest snapshot. This newer snapshot then
    /// replaces the current new-world state, and the fast-forwarding continues.
    ///
    /// Essentially, this is like taking a shortcut, and is important whenever the client performs
    /// a timeskip. The new world timestamp could suddenly become very far behind, and we don't
    /// want the client to become stuck trying to fastforward this very "old" server snapshot.
    Obsolete,

    /// When the client perform timeskips, weird things can happen to existing timestamps and the
    /// current new world timestamp that we are trying to fastforward may end up *ahead* of the
    /// current timestamp.
    Overshot,
}

impl Display for ReconciliationStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ReconciliationStatus::AwaitingSnapshot => write!(f, "Awaiting Snapshot"),
            ReconciliationStatus::Fastforwarding(health) => {
                write!(f, "Fastforwarding {}", health)
            }
            ReconciliationStatus::Blending(t) => write!(f, "Blending @ {}%", (t * 100.0) as i32),
        }
    }
}

impl Display for FastforwardingHealth {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            FastforwardingHealth::Healthy => write!(f, ""),
            FastforwardingHealth::Obsolete => write!(f, "Obsolete"),
            FastforwardingHealth::Overshot => write!(f, "Overshot"),
        }
    }
}

#[derive(Debug)]
struct ClientWorldSimulations<WorldType: World> {
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
    world_simulations:
        OldNew<WorldSimulation<WorldType, { InitializationType::NeedsInitialization }>>,

    /// The interpolation paramater to blend the `old_world` and `new_world` together into a
    /// single world state. The parameter is in the range `[0,1]` where 0 represents using only
    /// the `old_world`, and where 1 represents using only the `new_world`.
    blend_old_new_interpolation_t: f64,

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
            base_command_buffer: CommandBuffer::new(),
            world_simulations: OldNew::new(),
            blend_old_new_interpolation_t: 1.0,
            states: OldNew::new(),
            display_state: None,
            config,
        };
        let OldNewResult { old, new } = client_world_simulations.world_simulations.get_mut();
        old.reset_last_completed_timestamp(initial_timestamp);
        new.reset_last_completed_timestamp(initial_timestamp);
        client_world_simulations
    }

    pub fn infer_current_reconciliation_status(&self) -> ReconciliationStatus {
        let OldNewResult {
            old: old_world_simulation,
            new: new_world_simulation,
        } = self.world_simulations.get();

        if new_world_simulation.last_completed_timestamp()
            == old_world_simulation.last_completed_timestamp()
        {
            assert_eq!(
                new_world_simulation.last_completed_timestamp(),
                old_world_simulation.last_completed_timestamp()
            );
            if self.blend_old_new_interpolation_t < 1.0 {
                ReconciliationStatus::Blending(self.blend_old_new_interpolation_t)
            } else {
                assert!(self.blend_old_new_interpolation_t >= 1.0);
                ReconciliationStatus::AwaitingSnapshot
            }
        } else {
            assert!(
                self.blend_old_new_interpolation_t <= 0.0,
                "Interpolation t advances only if timestamps are equal, and once they are equal, they remain equal even in timeskips."
            );

            let is_snapshot_newer = self.queued_snapshot.as_ref().map_or(false, |snapshot| {
                snapshot.timestamp() > new_world_simulation.last_completed_timestamp()
            });

            let fastforward_status = if new_world_simulation.last_completed_timestamp()
                > old_world_simulation.last_completed_timestamp()
            {
                FastforwardingHealth::Overshot
            } else if is_snapshot_newer {
                FastforwardingHealth::Obsolete
            } else {
                assert!(
                    new_world_simulation.last_completed_timestamp()
                        < old_world_simulation.last_completed_timestamp()
                );
                FastforwardingHealth::Healthy
            };
            ReconciliationStatus::Fastforwarding(fastforward_status)
        }
    }

    fn receive_command(&mut self, command: &Timestamped<WorldType::CommandType>) {
        debug!("Received command {:?}", command);
        let OldNewResult { old, new } = self.world_simulations.get_mut();
        self.base_command_buffer.insert(command);
        old.schedule_command(command);
        new.schedule_command(command);
    }

    fn receive_snapshot(&mut self, snapshot: Timestamped<WorldType::SnapshotType>) {
        trace!(
            "Received snapshot: {:?} frames behind",
            self.last_completed_timestamp() - snapshot.timestamp()
        );

        self.last_received_snapshot_timestamp = Some(snapshot.timestamp());

        if snapshot.timestamp() > self.last_completed_timestamp() {
            warn!("Received snapshot from the future! Ignoring snapshot.");
            return;
        }
        match &self.last_queued_snapshot_timestamp {
            None => self.queued_snapshot = Some(snapshot),
            Some(last_timestamp) => {
                // Ignore stale snapshots.
                if snapshot.timestamp() > *last_timestamp {
                    self.queued_snapshot = Some(snapshot);
                } else {
                    warn!("Received stale snapshot - ignoring.");
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
        fn load_snapshot<WorldType: World>(
            this: &mut ClientWorldSimulations<WorldType>,
            snapshot: &Timestamped<WorldType::SnapshotType>,
        ) {
            trace!("Loading new snapshot from server");

            let OldNewResult {
                old: old_world_simulation,
                new: new_world_simulation,
            } = this.world_simulations.get_mut();

            // We can now safely discard commands from the buffer that are older than this
            // server snapshot.
            //
            // Off-by-one check:
            //
            // snapshot has completed the frame at t=snapshot.timestamp(), and therefore
            // has already applied commands that are scheduled for t=snapshot.timestamp().
            this.base_command_buffer.drain_up_to(snapshot.timestamp());

            new_world_simulation
                .apply_completed_snapshot(&snapshot, this.base_command_buffer.clone());

            if new_world_simulation.last_completed_timestamp()
                > old_world_simulation.last_completed_timestamp()
            {
                // The server should always be behind the client, even excluding the
                // network latency. The client may momentarily fall behind due to, e.g.,
                // browser tab sleeping, but once the browser tab wakes up, the client
                // should automatically compensate, and if necessary, time skip to the
                // correct timestamp to be ahead of the server. If even then the server
                // continues to be ahead, then it might suggest that the client and the
                // server's clocks are running at different rates, and some additional time
                // syncing mechanism is needed.
                warn!("Server's snapshot is newer than client!");
            }

            // We reset the old/new interpolation factor and begin slowly blending in from
            // the old world to the new world once the new world has caught up (aka
            // "fast-forwarded") to the old world's timestamp.
            this.blend_old_new_interpolation_t = 0.0;
        }

        fn simulate_next_frame<WorldType: World>(this: &mut ClientWorldSimulations<WorldType>) {
            trace!("Stepping old world by one frame");
            let OldNewResult {
                old: old_world_simulation,
                new: new_world_simulation,
            } = this.world_simulations.get_mut();
            old_world_simulation.step();

            trace!(
                "Fastforwarding new world from timestamp {:?} to current timestamp {:?}",
                new_world_simulation.last_completed_timestamp(),
                old_world_simulation.last_completed_timestamp()
            );
            new_world_simulation.try_completing_simulations_up_to(
                old_world_simulation.last_completed_timestamp(),
                this.config.fastforward_max_per_step,
            );
        }

        fn publish_old_state<WorldType: World>(this: &mut ClientWorldSimulations<WorldType>) {
            this.states.swap();
            this.states
                .set_new(this.world_simulations.get().old.display_state());
        }

        fn publish_blended_state<WorldType: World>(this: &mut ClientWorldSimulations<WorldType>) {
            let OldNewResult {
                old: old_world_simulation,
                new: new_world_simulation,
            } = this.world_simulations.get_mut();

            assert_eq!(
                old_world_simulation.last_completed_timestamp(),
                new_world_simulation.last_completed_timestamp()
            );

            trace!("Blending the old and new world states");
            let state_to_publish = match (
                old_world_simulation.display_state(),
                new_world_simulation.display_state(),
            ) {
                (Some(old), Some(new)) => Some(
                    Timestamped::<WorldType::DisplayStateType>::from_interpolation(
                        &old,
                        &new,
                        this.blend_old_new_interpolation_t,
                    ),
                ),
                (None, Some(new)) => Some(new),
                (Some(_), None) => {
                    unreachable!("New world is always initialized before old world does")
                }
                (None, None) => None,
            };

            this.states.swap();
            this.states.set_new(state_to_publish);
        }

        trace!("Step...");

        match self.infer_current_reconciliation_status() {
            ReconciliationStatus::Blending(_) => {
                self.blend_old_new_interpolation_t += self.config.blend_progress_per_frame();
                self.blend_old_new_interpolation_t =
                    self.blend_old_new_interpolation_t.clamp(0.0, 1.0);
                simulate_next_frame(self);
                publish_blended_state(self);

                assert!(
                    matches!(
                        self.infer_current_reconciliation_status(),
                        ReconciliationStatus::Blending(_) | ReconciliationStatus::AwaitingSnapshot,
                    ),
                    "Unexpected status change into: {:?}",
                    self.infer_current_reconciliation_status()
                );
            }

            ReconciliationStatus::AwaitingSnapshot => {
                if let Some(snapshot) = self.queued_snapshot.take() {
                    self.world_simulations.swap();
                    load_snapshot(self, &snapshot);
                    simulate_next_frame(self);
                    publish_old_state(self);

                    assert!(
                        matches!(
                            self.infer_current_reconciliation_status(),
                            ReconciliationStatus::Fastforwarding(
                                FastforwardingHealth::Healthy | FastforwardingHealth::Overshot
                            )
                        ) || matches!(
                            self.infer_current_reconciliation_status(),
                            ReconciliationStatus::Blending(t) if t == 0.0
                        ),
                        "Unexpected status change into: {:?}",
                        self.infer_current_reconciliation_status()
                    );
                } else {
                    simulate_next_frame(self);
                    publish_blended_state(self);

                    assert!(
                        matches!(
                            self.infer_current_reconciliation_status(),
                            ReconciliationStatus::AwaitingSnapshot
                        ),
                        "Unexpected status change into: {:?}",
                        self.infer_current_reconciliation_status()
                    );
                }
            }

            ReconciliationStatus::Fastforwarding(FastforwardingHealth::Obsolete) => {
                let snapshot = self
                    .queued_snapshot
                    .take()
                    .expect("New world can only be obsolete if there exists a newer snapshot");

                warn!("Abandoning previous snapshot for newer shapshot! Couldn't fastforward the previous snapshot in time.");

                load_snapshot(self, &snapshot);
                simulate_next_frame(self);
                publish_old_state(self);

                assert!(
                    matches!(
                        self.infer_current_reconciliation_status(),
                        ReconciliationStatus::Fastforwarding(FastforwardingHealth::Healthy)
                    ),
                    "Unexpected status change into: {:?}",
                    self.infer_current_reconciliation_status()
                );
            }

            ReconciliationStatus::Fastforwarding(FastforwardingHealth::Healthy) => {
                simulate_next_frame(self);
                publish_old_state(self);

                assert!(
                    matches!(
                        self.infer_current_reconciliation_status(),
                        ReconciliationStatus::Fastforwarding(FastforwardingHealth::Healthy)
                    ) || matches!(
                        self.infer_current_reconciliation_status(),
                        ReconciliationStatus::Blending(t) if t == 0.0
                    ),
                    "Unexpected status change into: {:?}",
                    self.infer_current_reconciliation_status()
                );
            }

            ReconciliationStatus::Fastforwarding(FastforwardingHealth::Overshot) => {
                warn!("New world fastforwarded beyond old world - This should only happen when the client had timeskipped.");

                let OldNewResult {
                    old: old_world_simulation,
                    new: new_world_simulation,
                } = self.world_simulations.get_mut();

                new_world_simulation.reset_last_completed_timestamp(
                    old_world_simulation.last_completed_timestamp(),
                );
                simulate_next_frame(self);
                publish_blended_state(self);

                assert!(
                    matches!(
                        self.infer_current_reconciliation_status(),
                        ReconciliationStatus::Blending(t) if t == 0.0
                    ),
                    "Unexpected status change into: {:?}",
                    self.infer_current_reconciliation_status()
                );
            }
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

        let old_timestamp = old_world_simulation.last_completed_timestamp();

        if new_world_simulation.last_completed_timestamp()
            == old_world_simulation.last_completed_timestamp()
        {
            new_world_simulation.reset_last_completed_timestamp(corrected_timestamp);
        }

        old_world_simulation.reset_last_completed_timestamp(corrected_timestamp);

        // Note: If timeskip was so large that timestamp has wrapped around to the past,
        // then we need to clear all the commands in the base command buffer so that any
        // pending commands to get replayed unexpectedly in the future at the wrong time.
        if corrected_timestamp < old_timestamp {
            self.base_command_buffer.drain_all();
        }
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

        trace!("Reset last completed timestamp if outside comparable range");
        if let Some(last_timestamp) = self.last_queued_snapshot_timestamp.as_ref().copied() {
            let comparable_range =
                Timestamp::comparable_range_with_midpoint(self.last_completed_timestamp());
            if !comparable_range.contains(&last_timestamp)
                || last_timestamp > self.last_completed_timestamp()
            {
                warn!("Last queued snapshot timestamp too old to compare. Resetting timestamp book-keeping");
                // This is done to prevent new snapshots from being discarded due
                // to a very old snapshot that was last applied. This can happen
                // after a very long pause (e.g. browser tab sleeping).
                // We don't jump to `comparable_range.start` since it will be invalid immediately
                // in the next iteration.
                self.last_queued_snapshot_timestamp = Some(
                    self.last_completed_timestamp()
                        - (self.config.lag_compensation_frame_count() * 2),
                );
            }
        }
    }
}
