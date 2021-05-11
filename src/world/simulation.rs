use crate::{
    command::CommandBuffer,
    fixed_timestepper::{FixedTimestepper, Stepper},
    timestamp::{Timestamp, Timestamped},
    world::World,
};
use std::fmt::Debug;
use tracing::trace;

/// Whether the [`Simulation`] needs to wait for a snapshot to be applied before its display
/// state can be considered usable.
#[derive(PartialEq, Eq)]
pub(crate) enum InitializationType {
    /// Implies that the world can be used as is without first being initialized with any snapshot.
    /// A good example would be the server's world simulations.
    PreInitialized,

    /// Implies that the world starts off in an "invalid" state, and needs to be initialized with a
    /// snapshot before its resulting display state is "valid" and usable. A good example would be
    /// the clients' world simulations.
    NeedsInitialization,
}

/// A "wrapper" for your [`World`] that is equipped with a `CommandBuffer`. This is responsible
/// for applying the correct commands at the correct time while running your [`World`] simulation.
#[derive(Debug)]
pub(crate) struct Simulation<WorldType: World, const INITIALIZATION_TYPE: InitializationType> {
    world: WorldType,
    command_buffer: CommandBuffer<WorldType::CommandType>,
    has_initialized: bool,
}

impl<WorldType: World, const INITIALIZATION_TYPE: InitializationType> Default
    for Simulation<WorldType, INITIALIZATION_TYPE>
{
    fn default() -> Self {
        Self {
            world: Default::default(),
            command_buffer: CommandBuffer::new(),
            has_initialized: match INITIALIZATION_TYPE {
                InitializationType::NeedsInitialization => false,
                InitializationType::PreInitialized => true,
            },
        }
    }
}

impl<WorldType: World, const INITIALIZATION_TYPE: InitializationType>
    Simulation<WorldType, INITIALIZATION_TYPE>
{
    /// Create a new [`Simulation`].
    pub fn new() -> Self {
        Self::default()
    }

    /// The timestamp for the next frame that is either about to be simulated, of is currently
    /// being simulated. This timestamp is useful for issuing commands to be applied to the
    /// world as soon as possible.
    pub fn simulating_timestamp(&self) -> Timestamp {
        self.last_completed_timestamp() + 1
    }

    /// Schedule a command to be applied to the world at the beginning of the frame with the
    /// given timestamp.
    pub fn schedule_command(&mut self, command: &Timestamped<WorldType::CommandType>) {
        self.command_buffer.insert(command);
    }

    /// Tries to "fast-forward" the world simulation so that the timestamp of the last completed
    /// frame matches the target timestamp. If the maximum number of steps is reached, the
    /// `last_completed_timestamp()` will not match the target timestamp yet, since no frames are
    /// to be skipped over.
    pub fn try_completing_simulations_up_to(
        &mut self,
        target_completed_timestamp: Timestamp,
        max_steps: usize,
    ) {
        for _ in 0..max_steps {
            if self.last_completed_timestamp() >= target_completed_timestamp {
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
        completed_snapshot: &Timestamped<WorldType::SnapshotType>,
        rewound_command_buffer: CommandBuffer<WorldType::CommandType>,
    ) {
        self.world
            .apply_snapshot(completed_snapshot.inner().clone());
        self.command_buffer = rewound_command_buffer;
        self.command_buffer
            .update_timestamp(completed_snapshot.timestamp());
        self.has_initialized = true;
    }

    /// Generates a timestamped snapshot of the world for the latest frame that has completed
    /// its simulation.
    pub fn last_completed_snapshot(&self) -> Timestamped<WorldType::SnapshotType> {
        Timestamped::new(self.world.snapshot(), self.last_completed_timestamp())
    }

    /// Get the current display state of the world. Returns `None` if the world has not been
    /// initialized with a snapshot yet, and [`INITIALIZATION_TYPE`](Simulation) is
    /// [`InitializationType::NeedsInitialization`].
    pub fn display_state(&self) -> Option<Timestamped<WorldType::DisplayStateType>> {
        if self.has_initialized {
            Some(Timestamped::new(
                self.world.display_state(),
                self.last_completed_timestamp(),
            ))
        } else {
            None
        }
    }

    /// Get a list of commands currently buffered. This is primarily for diagnostic purposes.
    pub fn buffered_commands(
        &self,
    ) -> impl Iterator<Item = (Timestamp, &Vec<WorldType::CommandType>)> {
        self.command_buffer.iter()
    }
}

impl<WorldType: World, const INITIALIZATION_TYPE: InitializationType> Stepper
    for Simulation<WorldType, INITIALIZATION_TYPE>
{
    fn step(&mut self) {
        trace!(
            "World simulation step (simulation timestamp: {:?})",
            self.simulating_timestamp()
        );

        // We first drain up to and including the commands that are scheduled for this
        // timestamp that we are trying to simulate.
        let commands = self.command_buffer.drain_up_to(self.simulating_timestamp());
        for command in commands {
            trace!("Applying command {:?}", &command);
            self.world.apply_command(&command);
        }

        // We then advance the world simulation by one frame.
        self.world.step();

        // The simulation for this frame has been completed, so we can now increment the
        // timestamp.
        self.command_buffer
            .update_timestamp(self.last_completed_timestamp() + 1);
    }
}

impl<WorldType: World, const INITIALIZATION_TYPE: InitializationType> FixedTimestepper
    for Simulation<WorldType, INITIALIZATION_TYPE>
{
    /// The timestamp for the frame that has completed simulation, and whose resulting state is
    /// now available.
    fn last_completed_timestamp(&self) -> Timestamp {
        self.command_buffer.timestamp()
    }

    /// Useful for initializing the timestamp, syncing the timestamp, and, when necessary,
    /// (i.e. when the world is drifting too far behind and can't catch up), jump to a
    /// corrected timestamp in the future without running any simulation. Note that any
    /// leftover stale commands won't be dropped - they will still be applied in the next
    /// frame, unless they are too stale that the must be dropped to maintain the transitivity
    /// laws.
    fn reset_last_completed_timestamp(&mut self, timestamp: Timestamp) {
        let old_timestamp = self.last_completed_timestamp();
        self.command_buffer.update_timestamp(timestamp);

        // Note: If timeskip was so large that timestamp has wrapped around to the past, then we
        // need to apply all the commands in the command buffer so that any pending commands to get
        // replayed unexpectedly in the future at the wrong time.
        if timestamp < old_timestamp {
            let commands = self.command_buffer.drain_all();
            for command in commands {
                trace!("Applying stale command after large timeskip {:?}", &command);
                self.world.apply_command(&command);
            }
        }
    }
}
