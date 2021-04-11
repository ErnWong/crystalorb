use crate::{
    channels::ClockSyncMessage,
    fixed_timestepper,
    fixed_timestepper::{FixedTimestepper, Stepper},
    network_resource::{Connection, ConnectionHandleType, NetworkResource},
    timestamp::{Timestamp, Timestamped},
    world::{World, WorldSimulation},
    Config,
};
use tracing::{error, info, trace, warn};

pub struct Server<WorldType: World> {
    world_simulation: WorldSimulation<WorldType>,
    timestep_overshoot_seconds: f32,
    seconds_since_last_snapshot: f32,
    config: Config,
}

impl<WorldType: World> Server<WorldType> {
    pub fn new(config: Config) -> Self {
        Self {
            world_simulation: Default::default(),
            timestep_overshoot_seconds: 0.0,
            seconds_since_last_snapshot: 0.0,
            config,
        }
    }

    pub fn last_completed_timestamp(&self) -> Timestamp {
        self.world_simulation.last_completed_timestamp()
    }

    pub fn simulating_timestamp(&self) -> Timestamp {
        self.world_simulation.simulating_timestamp()
    }

    /// The timestamp that clients are supposed to be at (which should always be ahead of the
    /// server to compensate for the latency between the server and the clients).
    pub fn estimated_client_timestamp(&self) -> Timestamp {
        self.last_completed_timestamp() + self.config.lag_compensation_frame_count()
    }

    /// Positive refers that our world is ahead of the timestamp it is supposed to be, and
    /// negative refers that our world needs to catchup in the next frame.
    pub fn timestamp_drift(&self, seconds_since_startup: f64) -> Timestamp {
        self.estimated_client_timestamp()
            - Timestamp::from_seconds(seconds_since_startup, self.config.timestep_seconds)
    }

    /// Positive refers that our world is ahead of the timestamp it is supposed to be, and
    /// negative refers that our world needs to catchup in the next frame.
    pub fn timestamp_drift_seconds(&self, seconds_since_startup: f64) -> f32 {
        self.timestamp_drift(seconds_since_startup)
            .as_seconds(self.config.timestep_seconds)
    }

    pub fn update_timestamp(&mut self, seconds_since_startup: f64) {
        let timestamp =
            Timestamp::from_seconds(seconds_since_startup, self.config.timestep_seconds)
                - self.config.lag_compensation_frame_count();
        info!(
            "Updating server world timestamp to {:?} (note: lag compensation frame count = {})",
            timestamp,
            self.config.lag_compensation_frame_count()
        );
        self.world_simulation
            .reset_last_completed_timestamp(timestamp);
    }

    fn apply_validated_command<NetworkResourceType: NetworkResource>(
        &mut self,
        command: Timestamped<WorldType::CommandType>,
        command_source: Option<ConnectionHandleType>,
        net: &mut NetworkResourceType,
    ) {
        info!("Received command from {:?} - {:?}", command_source, command);

        // Apply this command to our world later on.
        self.world_simulation.schedule_command(command.clone());

        // Relay command to every other client.
        for (handle, mut connection) in net.connections() {
            // Don't send it back to the same client if there is one.
            if let Some(source_handle) = command_source {
                if handle == source_handle {
                    continue;
                }
            }
            let result = connection.send(command.clone());
            connection.flush::<Timestamped<WorldType::CommandType>>();
            if let Some(message) = result {
                error!("Failed to relay command to [{}]: {:?}", handle, message);
            }
        }
    }

    fn receive_command<NetworkResourceType: NetworkResource>(
        &mut self,
        command: Timestamped<WorldType::CommandType>,
        command_source: ConnectionHandleType,
        net: &mut NetworkResourceType,
    ) {
        if WorldType::command_is_valid(command.inner(), command_source)
        // TODO: Is it valid to validate the timestamps?
        // && command.timestamp() >= self.world_simulation.last_completed_timestamp()
        // && command.timestamp() <= self.estimated_client_timestamp()
        {
            self.apply_validated_command(command, Some(command_source), net);
        }
    }

    /// Issue a command from the server to the world. The command will be scheduled to the
    /// estimated client's current timestamp.
    pub fn issue_command<NetworkResourceType: NetworkResource>(
        &mut self,
        command: WorldType::CommandType,
        net: &mut NetworkResourceType,
    ) {
        self.apply_validated_command(
            Timestamped::new(command, self.estimated_client_timestamp()),
            None,
            net,
        );
    }

    pub fn display_state(&self) -> WorldType::DisplayStateType {
        self.world_simulation.display_state()
    }

    pub fn update<NetworkResourceType: NetworkResource>(
        &mut self,
        delta_seconds: f32,
        seconds_since_startup: f64,
        net: &mut NetworkResourceType,
    ) {
        let mut new_commands = Vec::new();
        let mut clock_syncs = Vec::new();
        for (handle, mut connection) in net.connections() {
            while let Some(command) = connection.recv::<Timestamped<WorldType::CommandType>>() {
                new_commands.push((command, handle));
            }
            while let Some(mut clock_sync_message) = connection.recv::<ClockSyncMessage>() {
                trace!("Replying to clock sync message. client_id: {}", handle);
                clock_sync_message.server_seconds_since_startup = seconds_since_startup;
                clock_sync_message.client_id = handle;
                clock_syncs.push((handle, clock_sync_message));
            }
        }
        for (command, command_source) in new_commands {
            self.receive_command(command.clone(), command_source, &mut *net);
        }
        for (handle, clock_sync_message) in clock_syncs {
            net.send_message(handle, clock_sync_message).unwrap();
        }

        // Compensate for any drift.
        // TODO: Remove duplicate code between client and server.
        let next_delta_seconds = (delta_seconds
            - self.timestamp_drift_seconds(seconds_since_startup))
        .clamp(0.0, self.config.update_delta_seconds_max);

        self.advance(next_delta_seconds);

        // If drift is too large and we still couldn't keep up, do a time skip.
        trace!(
            "Timestamp drift: {:?}",
            self.timestamp_drift_seconds(seconds_since_startup)
        );
        if -self.timestamp_drift_seconds(seconds_since_startup)
            > self.config.timestamp_skip_threshold_seconds
        {
            let corrected_timestamp = self.world_simulation.last_completed_timestamp()
                - self.timestamp_drift(seconds_since_startup);
            warn!(
                "Server is too far behind. Skipping timestamp from {:?} to {:?}",
                self.world_simulation.last_completed_timestamp(),
                corrected_timestamp
            );
            self.world_simulation
                .reset_last_completed_timestamp(corrected_timestamp);
        }

        self.seconds_since_last_snapshot += delta_seconds;
        if self.seconds_since_last_snapshot > self.config.snapshot_send_period {
            trace!(
                "Broadcasting snapshot at timestamp: {:?} (note: drift error: {})",
                self.world_simulation.last_completed_timestamp(),
                self.timestamp_drift_seconds(seconds_since_startup),
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
