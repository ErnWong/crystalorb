use crate::{
    command::{Command, CommandBuffer},
    fixed_timestepper::Stepper,
    timestamp::{Timestamp, Timestamped},
};
use std::fmt::Debug;
use turbulence::message_channels::ChannelMessage;

pub trait DisplayState: Default + Send + Sync + Clone {
    fn from_interpolation(state1: &Self, state2: &Self, t: f32) -> Self;
}

pub trait World: Stepper + Default + Send + Sync + 'static {
    type CommandType: Command;
    type SnapshotType: ChannelMessage + Clone + Debug;
    type DisplayStateType: DisplayState;

    fn command_is_valid(command: &Self::CommandType, client_id: usize) -> bool;
    fn apply_command(&mut self, command: &Self::CommandType);
    fn apply_snapshot(&mut self, snapshot: Self::SnapshotType);
    fn snapshot(&self) -> Self::SnapshotType;
    fn display_state(&self) -> Self::DisplayStateType;
}

pub struct WorldSimulation<WorldType: World> {
    world: WorldType,
    last_completed_timestamp: Timestamp,
    command_buffer: CommandBuffer<WorldType::CommandType>,
}

impl<WorldType: World> Default for WorldSimulation<WorldType> {
    fn default() -> Self {
        Self {
            world: Default::default(),
            last_completed_timestamp: Default::default(),
            command_buffer: Default::default(),
        }
    }
}

impl<WorldType: World> WorldSimulation<WorldType> {
    pub fn new() -> Self {
        Self::default()
    }

    /// The timestamp for the frame that has completed simulation, and whose resulting state is
    /// now available.
    pub fn last_completed_timestamp(&self) -> Timestamp {
        self.last_completed_timestamp
    }

    /// Useful for initializing the timestamp, syncing the timestamp, and, when necessary,
    /// (i.e. when the world is drifting too far behind and can't catch up), jump to a
    /// corrected timestamp in the future without running any simulation. Note that any
    /// leftover stale commands won't be dropped - they will still be applied in the next
    /// frame.
    pub fn reset_last_completed_timestamp(&mut self, timestamp: Timestamp) {
        self.last_completed_timestamp = timestamp;
    }

    /// The timestamp for the next frame that is either about to be simulated, of is currently
    /// being simulated. This timestamp is useful for issuing commands to be applied to the
    /// world as soon as possible.
    pub fn simulating_timestamp(&self) -> Timestamp {
        self.last_completed_timestamp() + 1
    }

    /// Schedule a command to be applied to the world at the beginning of the frame with the
    /// given timestamp.
    pub fn schedule_command(&mut self, command: Timestamped<WorldType::CommandType>) {
        self.command_buffer.insert(command);
    }

    /// Tries to "fast-forward" the world simulation so that the timestamp of the last
    /// completed frame matches the target timestamp. If the maximum number of steps is
    /// reached, the last_completed_timestamp() will not match the target timestamp yet, since
    /// no frames are to be skipped over.
    pub fn try_completing_simulations_up_to(
        &mut self,
        target_completed_timestamp: &Timestamp,
        max_steps: usize,
    ) {
        for _ in 0..max_steps {
            if self.last_completed_timestamp() >= *target_completed_timestamp {
                break;
            }
            self.step();
        }
    }

    /// Apply a snapshot of the world that has completed its simulation for the given
    /// timestamped frame. Since the snapshots are usually at a different timestamp, (usually
    /// in the past), the internal command buffer needs to be replenished/updated with commands
    /// that it had already discarded.
    /// Note: We had to discard commands once we have applied them because commands can arrive
    /// out of order, and we do not want to skip past any stale commands.
    pub fn apply_completed_snapshot(
        &mut self,
        completed_snapshot: Timestamped<WorldType::SnapshotType>,
        rewound_command_buffer: CommandBuffer<WorldType::CommandType>,
    ) {
        self.reset_last_completed_timestamp(completed_snapshot.timestamp());
        self.world
            .apply_snapshot(completed_snapshot.inner().clone());
        self.command_buffer = rewound_command_buffer;
    }

    /// Generates a timestamped snapshot of the world for the latest frame that has completed
    /// its simulation.
    pub fn last_completed_snapshot(&self) -> Timestamped<WorldType::SnapshotType> {
        Timestamped::new(self.world.snapshot(), self.last_completed_timestamp())
    }

    pub fn display_state(&self) -> WorldType::DisplayStateType {
        self.world.display_state()
    }
}

impl<WorldType: World> Stepper for WorldSimulation<WorldType> {
    fn step(&mut self) -> f32 {
        // We first drain up to and including the commands that are scheduled for this
        // timestamp that we are trying to simulate.
        let commands = self.command_buffer.drain_up_to(self.simulating_timestamp());
        for command in commands {
            self.world.apply_command(&command);
        }

        // We then advance the world simulation by one frame.
        let delta_seconds = self.world.step();

        // The simulation for this frame has been completed.
        self.last_completed_timestamp.increment();

        delta_seconds
    }
}
