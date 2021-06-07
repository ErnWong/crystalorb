use super::Simulator;
use crate::{
    command::CommandBuffer,
    fixed_timestepper::{FixedTimestepper, Stepper},
    timestamp::{Timestamp, Timestamped},
    world::{InitializationType, Simulation, World},
    Config,
};
use tracing::trace;

#[derive(Debug)]
pub struct Delayed<WorldType: World> {
    queued_snapshot: Option<Timestamped<WorldType::SnapshotType>>,
    base_command_buffer: CommandBuffer<WorldType::CommandType>,
    world_simulation: Simulation<WorldType, { InitializationType::NeedsInitialization }>,
    config: Config,
}

impl<WorldType: World> Simulator for Delayed<WorldType> {
    type WorldType = WorldType;
    type DisplayStateType<'a> = Option<Timestamped<WorldType::DisplayStateType>>;

    fn new(config: Config, initial_timestamp: Timestamp) -> Self {
        let world_simulation = Simulation::new();
        world_simulation.reset_last_completed_timestamp(initial_timestamp);
        Self {
            queued_snapshot: None,
            base_command_buffer: CommandBuffer::new(),
            world_simulation,
            config,
        }
    }

    fn display_state(&self) -> Option<Timestamped<WorldType::DisplayStateType>> {
        // TODO: Tweening?
        self.world_simulation.display_state()
    }

    fn receive_command(&mut self, command: &Timestamped<WorldType::CommandType>) {
        self.base_command_buffer.insert(command);
        self.world_simulation.schedule_command(command);
    }

    fn receive_snapshot(&mut self, snapshot: Timestamped<WorldType::SnapshotType>) {
        self.queued_snapshot = Some(snapshot);
    }
}

impl<WorldType: World> Stepper for Delayed<WorldType> {
    fn step(&mut self) {
        let target_completed_timestamp = self.world_simulation.simulating_timestamp();

        if let Some(snapshot) = self.queued_snapshot.take() {
            self.base_command_buffer.drain_up_to(snapshot.timestamp);
            self.world_simulation
                .apply_completed_snapshot(&snapshot, self.base_command_buffer.clone());
        }

        self.world_simulation.try_completing_simulations_up_to(
            target_completed_timestamp,
            self.config.fastforward_max_per_step,
        );
    }
}

impl<WorldType: World> FixedTimestepper for Delayed<WorldType> {
    fn last_completed_timestamp(&self) -> Timestamp {
        self.world_simulation.last_completed_timestamp() + self.config.delayed_frame_count()
    }

    fn reset_last_completed_timestamp(&mut self, corrected_timestamp: Timestamp) {
        self.world_simulation.reset_last_completed_timestamp(
            corrected_timestamp - self.config.delayed_frame_count(),
        );
    }

    fn post_update(&mut self, timestep_overshoot_seconds: f64) {
        trace!("Update the base command buffer's timestamp and accept-window");
        self.base_command_buffer
            .update_timestamp(self.last_completed_timestamp());
    }
}
