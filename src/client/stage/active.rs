use crate::{
    client::simulator::Simulator,
    clocksync::ClockSyncer,
    fixed_timestepper::{FixedTimestepper, TerminationCondition, TimeKeeper},
    network_resource::{Connection, NetworkResource},
    timestamp::{
        always_increasing::{AlwaysIncreasingFilter, FilterError},
        Timestamp, Timestamped,
    },
    world::{Tweened, World},
    Config,
};
use tracing::{info, trace, warn};

/// The internal CrystalOrb structure used to actively run the simulations, which is not
/// constructed until the [`ClockSyncer`] is ready.
#[derive(Debug)]
pub struct Active<SimulatorType: Simulator> {
    clocksyncer: ClockSyncer,

    snapshot_filter: AlwaysIncreasingFilter,

    timekeeping_simulations: TimeKeeper<SimulatorType, { TerminationCondition::FirstOvershoot }>,
}

impl<SimulatorType: Simulator> Active<SimulatorType> {
    pub fn new(seconds_since_startup: f64, config: Config, clocksyncer: ClockSyncer) -> Self {
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
            snapshot_filter: AlwaysIncreasingFilter::new(),
            timekeeping_simulations: TimeKeeper::new(
                SimulatorType::new(config.clone(), initial_timestamp),
                config,
            ),
        }
    }

    pub fn last_completed_timestamp(&self) -> Timestamp {
        self.timekeeping_simulations.last_completed_timestamp()
    }

    pub fn simulating_timestamp(&self) -> Timestamp {
        self.last_completed_timestamp() + 1
    }

    pub fn client_id(&self) -> usize {
        self.clocksyncer
            .client_id()
            .expect("Client should be connected by the time it is active")
    }

    pub fn buffered_commands(
        &self,
    ) -> impl Iterator<
        Item = (
            Timestamp,
            &Vec<<SimulatorType::WorldType as World>::CommandType>,
        ),
    > {
        self.timekeeping_simulations.buffered_commands()
    }

    pub fn display_state(
        &self,
    ) -> &Option<Tweened<<SimulatorType::WorldType as World>::DisplayStateType>> {
        self.timekeeping_simulations.display_state()
    }

    pub fn last_queued_snapshot_timestamp(&self) -> &Option<Timestamp> {
        self.timekeeping_simulations
            .last_queued_snapshot_timestamp()
    }

    pub fn last_received_snapshot_timestamp(&self) -> &Option<Timestamp> {
        self.timekeeping_simulations
            .last_received_snapshot_timestamp()
    }

    pub fn issue_command<NetworkResourceType: NetworkResource>(
        &mut self,
        command: <SimulatorType::WorldType as World>::CommandType,
        net: &mut NetworkResourceType,
    ) {
        let timestamped_command = Timestamped::new(command, self.simulating_timestamp());
        self.timekeeping_simulations
            .receive_command(&timestamped_command);
        net.broadcast_message(timestamped_command);
    }

    /// Perform the next update for the current rendering frame.
    pub fn update<NetworkResourceType: NetworkResource>(
        &mut self,
        delta_seconds: f64,
        seconds_since_startup: f64,
        net: &mut NetworkResourceType,
    ) {
        self.clocksyncer
            .update(delta_seconds, seconds_since_startup, net);

        for (_, mut connection) in net.connections() {
            while let Some(command) =
                connection.recv::<Timestamped<<SimulatorType::WorldType as World>::CommandType>>()
            {
                self.timekeeping_simulations.receive_command(&command);
            }
            while let Some(snapshot) =
                connection.recv::<Timestamped<<SimulatorType::WorldType as World>::SnapshotType>>()
            {
                self.receive_snapshot(snapshot);
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
                + self
                    .timekeeping_simulations
                    .config()
                    .lag_compensation_latency,
        );

        self.snapshot_filter.update(
            self.last_completed_timestamp(),
            self.last_completed_timestamp() - (self.config.lag_compensation_frame_count() * 2),
        );
    }

    /// Whether all the uninitialized state has been flushed out, and that the first display state
    /// is available to be shown to the client's screen.
    pub fn is_ready(&self) -> bool {
        self.display_state().is_some()
    }

    fn receive_snapshot(
        &mut self,
        snapshot: Timestamped<<SimulatorType::WorldType as World>::SnapshotType>,
    ) {
        match self.snapshot_filter.try_apply(snapshot) {
            Ok(newer_snapshot) => self
                .timekeeping_simulations
                .receive_snapshot(newer_snapshot),
            Err(FilterError::Stale) => {
                // Ignore stale snapshots.
                warn!("Received stale snapshot - ignoring.");
            }
            Err(FilterError::FromTheFuture) => {
                warn!("Received snapshot from the future! Ignoring snapshot.");
            }
        }
    }
}
