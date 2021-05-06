//! Contains traits for CrystalOrb to interface with your game physics world, along with internal
//! structures that interfaces directly with your world.

use crate::{
    command::{Command, CommandBuffer},
    fixed_timestepper::{FixedTimestepper, Stepper},
    timestamp::{Timestamp, Timestamped},
};
use serde::{de::DeserializeOwned, Serialize};
use std::{fmt::Debug, ops::Deref};
use tracing::trace;

/// The [`DisplayState`] represents the information about how to display the [`World`] at its
/// current state. For example, while a [`World`] might contain information about player's position
/// and velocities, some games may only need to know about the position to render it (unless
/// you're doing some fancy motion-blur). You can think of a [`DisplayState`] as the "output" of a
/// [`World`]. There is nothing stopping you from making the [`DisplayState`] the same structure as
/// the [`World`] if it makes more sense for your game, but most of the time, the [`World`]
/// structure may contain things that are inefficient to copy around (e.g. an entire physics engine)
pub trait DisplayState: Send + Sync + Clone + Debug {
    /// CrystalOrb needs to mix different [`DisplayState`]s from different [`World`]s together, as
    /// well as mix [`DisplayState`] from two adjacent timestamps. The
    /// [`from_interpolation`](DisplayState::from_interpolation) method tells CrystalOrb how to
    /// perform this "mix" operation. Here, the `t` parameter is the interpolation parameter that
    /// ranges between `0.0` and `1.0`, where `t = 0.0` represents the request to have 100% of
    /// `state1` and 0% of `state2`, and where `t = 1.0` represents the request to have 0% of
    /// `state1` and 100% of `state2`.
    ///
    /// A common operation to implement this function is through [linear
    /// interpolation](https://en.wikipedia.org/wiki/Linear_interpolation), which looks like this:
    ///
    /// ```text
    /// state1 * (1.0 - t) + state2 * t
    /// ```
    ///
    /// However, for things involving rotation, you may need to use [spherical linear
    /// interpolation](https://en.wikipedia.org/wiki/Slerp), or [circular
    /// statistics](https://en.wikipedia.org/wiki/Mean_of_circular_quantities), and perhaps you may
    /// need to convert between coordinate systems before/after performing the interpolation to get
    /// the right transformations about the correct pivot points.
    fn from_interpolation(state1: &Self, state2: &Self, t: f64) -> Self;
}

impl<T: DisplayState> DisplayState for Timestamped<T> {
    /// Interpolate between two timestamped display states. If the two timestamps are
    /// different, then the interpolation parameter `t` must be either `0.0` or `1.0`.
    ///
    /// # Panics
    ///
    /// Panics if the timestamps are different but the interpolation parameter is not `0.0` nor
    /// `1.0`, since timestamps are whole number values and cannot be continuously
    /// interpolated. Interpolating between two display states of different timestamps is known
    /// as "tweening" (i.e. animation in-betweening) and should be done using
    /// [`Tweened::from_interpolation`].
    fn from_interpolation(state1: &Self, state2: &Self, t: f64) -> Self {
        if t == 0.0 {
            state1.clone()
        } else if (t - 1.0).abs() < f64::EPSILON {
            state2.clone()
        } else {
            assert_eq!(state1.timestamp(), state2.timestamp(), "Can only interpolate between timestamped states of the same timestamp. If timestamps differ, you will need to use Tweened::from_interpolation to also interpolate the timestamp value into a float.");

            Self::new(
                DisplayState::from_interpolation(state1.inner(), state2.inner(), t),
                state1.timestamp(),
            )
        }
    }
}

/// This is the result when you interpolate/"blend"/"tween" between two [`DisplayState`]s of
/// adjacent timestamps (similar to ["Inbetweening"](https://en.wikipedia.org/wiki/Inbetweening) in
/// animation - the generation of intermediate frames). You get the [`DisplayState`] and a
/// floating-point, non-whole-number timestamp.
#[derive(Default, Debug, Clone, PartialEq)]
pub struct Tweened<T> {
    display_state: T,
    timestamp: f64,
}

impl<T: DisplayState> Tweened<T> {
    /// Get the resulting in-between [`DisplayState`].
    pub fn display_state(&self) -> &T {
        &self.display_state
    }

    /// Get the "logical timestamp" that [`Tweened::display_state`] corresponds with. For
    /// example, a `float_timestamp` of `123.4` represents the in-between frame that is 40% of
    /// the way between frame `123` and frame `124`.
    pub fn float_timestamp(&self) -> f64 {
        self.timestamp
    }
}

impl<T: DisplayState> Deref for Tweened<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.display_state
    }
}

impl<T: DisplayState> Tweened<T> {
    pub(crate) fn from_interpolation(
        state1: &Timestamped<T>,
        state2: &Timestamped<T>,
        t: f64,
    ) -> Self {
        // Note: timestamps are in modulo arithmetic, so we need to work using the wrapped
        // difference value.
        let timestamp_difference: i16 = (state2.timestamp() - state1.timestamp()).into();
        let timestamp_offset: f64 = t * (timestamp_difference as f64);
        let timestamp_interpolated = i16::from(state1.timestamp()) as f64 + timestamp_offset;
        Self {
            display_state: T::from_interpolation(state1.inner(), state2.inner(), t),
            timestamp: timestamp_interpolated,
        }
    }
}

impl<T: DisplayState> From<Timestamped<T>> for Tweened<T> {
    fn from(timestamped: Timestamped<T>) -> Self {
        Self {
            display_state: timestamped.inner().clone(),
            timestamp: i16::from(timestamped.timestamp()) as f64,
        }
    }
}

/// Structures that implement the [`World`] trait are structures that are responsible for storing
/// *and* simulating the game physics. The [`World`] is a simulation that is updated using its
/// [`Stepper::step`] implementation. Players and any game logic outside of the physics simulation
/// can interact with the physics simulation by applying [commands](Command) to the [`World`] (for
/// example, a command to tell player 2's rigid body to jump, or a command to spawn a new player
/// rigid body).
///
/// CrystalOrb needs two additional functionality for your world:
/// (1) the ability to
/// [create](World::snapshot) and [apply](World::apply_snapshot) [snapshots](World::SnapshotType)
/// so that CrystalOrb can synchronize the states between the client and the server, and
/// (2) the ability to [output](World::display_state) and [mix](DisplayState::from_interpolation)
/// between the "display state" of the world.
///
/// # Conceptual examples with various physics engines
///
/// If you are using the [rapier physics
/// engine](https://www.rapier.rs), then this [`World`] structure would contain things like the
/// `PhysicsPipeline`, `BroadPhase`, `NarrowPhase`, `RigidBodySet`, `ColliderSet`, `JointSet`,
/// `CCDSolver`, and any other pieces of game-specific state you need. The `Stepper::step` implementation
/// for such a world would typically invoke `PhysicsPipeline::step`, as well as any other
/// game-specific non-physics logic.
///
/// If you are using the [nphysics physics
/// engine](https://www.nphysics.org), then this [`World`] structure would contain things like the
/// `DefaultMechanicalWorld`, `DefaultGeometricalWorld`, `DefaultBodySet`, `DefaultColliderSet`,
/// `DefaultJointConstraintSet`, and `DefaultForceGeneratorSet`, as well as any other pieces of
/// game-specific state you need. The `Stepper::step` implementation for such a world would
/// typically invoke `DefaultMechanicalWorld::step`, and any other game-specific update logic.
pub trait World: Stepper + Default + Send + Sync + 'static {
    /// The command that can be used by the game and the player to interact with the physics
    /// simulation. Typically, this is an enum of some kind, but it is up to you.
    type CommandType: Command;

    /// The subset of state information about the world that can be used to fully recreate the
    /// world. Needs to be serializable so that it can be sent from the server to the client. There
    /// is nothing stopping you from making the [`SnapshotType`](World::SnapshotType) the same type
    /// as your [`World`] if you are ok with serializing the entire physics engine state. If you
    /// think sending your entire [`World`] is a bit too heavy-weighted, you can hand-craft and
    /// optimise your own `SnapshotType` structure to be as light-weight as possible.
    type SnapshotType: Debug + Clone + Serialize + DeserializeOwned + Send + Sync + 'static;

    /// The subset of state information about the world that is to be displayed/rendered. This is
    /// used by the client to create the perfect blend of state information for the current
    /// rendering frame. You could make this [`DisplayState`](World::DisplayStateType) the same
    /// structure as your [`World`] if you don't mind CrystalOrb making lots of copies of your
    /// entire [`World`] structure and performing interpolation on all your state variables.
    type DisplayStateType: DisplayState;

    /// Each [`Command`] that the server receives from the client needs to be validated before
    /// being applied as well as relayed to other clients. You can use this method to check that
    /// the client has sufficient permission to issue such command. For example, you can prevent
    /// other clients from moving players that they don't own. Clients can determine their own
    /// `client_id` using the
    /// [`ReadyClient::client_id`](crate::client::ReadyClient::client_id) method.
    fn command_is_valid(command: &Self::CommandType, client_id: usize) -> bool;

    /// This describes how a [`Command`] affects the [`World`]. Use this method to update your
    /// state. For example, you may want to apply forces/impulses to your rigid bodies, or simply
    /// toggle some of your state variables that will affect how your [`Stepper::step`] will run.
    fn apply_command(&mut self, command: &Self::CommandType);

    /// Apply a [snapshot](World::SnapshotType) generated from another [`World`]'s
    /// [`World::snapshot`] method, so that this [`World`] will behave identically to that other
    /// [`World`]. See [`World::SnapshotType`] for more information.
    ///
    /// Typically, the server generates snapshots that clients need to regularly apply.
    fn apply_snapshot(&mut self, snapshot: Self::SnapshotType);

    /// Generate the subset of the world state information that is needed to recreate this
    /// [`World`] and have it behave identically to this current [`World`]. See
    /// [`World::SnapshotType`] for more information.
    ///
    /// Typically, the server generates snapshots that clients need to regularly apply.
    fn snapshot(&self) -> Self::SnapshotType;

    /// Generate the subset of the world state information that is needed to render the world.
    /// See [`World::DisplayStateType`] and [`DisplayState`].
    fn display_state(&self) -> Self::DisplayStateType;
}

/// Whether the [`WorldSimulation`] needs to wait for a snapshot to be applied before its display
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
pub(crate) struct WorldSimulation<WorldType: World, const INITIALIZATION_TYPE: InitializationType> {
    world: WorldType,
    command_buffer: CommandBuffer<WorldType::CommandType>,
    has_initialized: bool,
}

impl<WorldType: World, const INITIALIZATION_TYPE: InitializationType> Default
    for WorldSimulation<WorldType, INITIALIZATION_TYPE>
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
    WorldSimulation<WorldType, INITIALIZATION_TYPE>
{
    /// Create a new [`WorldSimulation`].
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
    pub fn schedule_command(&mut self, command: Timestamped<WorldType::CommandType>) {
        self.command_buffer.insert(command);
    }

    /// Tries to "fast-forward" the world simulation so that the timestamp of the last completed
    /// frame matches the target timestamp. If the maximum number of steps is reached, the
    /// `last_completed_timestamp()` will not match the target timestamp yet, since no frames are
    /// to be skipped over.
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
    /// initialized with a snapshot yet, and [`INITIALIZATION_TYPE`](WorldSimulation) is
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
    for WorldSimulation<WorldType, INITIALIZATION_TYPE>
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
    for WorldSimulation<WorldType, INITIALIZATION_TYPE>
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

#[cfg(test)]
mod test {
    use super::*;

    #[derive(Clone, Default, Debug, PartialEq)]
    struct MockDisplayState(f64);
    impl DisplayState for MockDisplayState {
        fn from_interpolation(state1: &Self, state2: &Self, t: f64) -> Self {
            Self(state1.0 * t + state2.0 * (1.0 - t))
        }
    }

    #[test]
    fn when_interpolating_displaystate_with_t_0_then_state1_is_returned() {
        // GIVEN
        let state1 = Timestamped::new(MockDisplayState(4.0), Timestamp::default() + 2);
        let state2 = Timestamped::new(MockDisplayState(8.0), Timestamp::default() + 5);

        // WHEN
        let interpolated = DisplayState::from_interpolation(&state1, &state2, 0.0);

        // THEN
        assert_eq!(state1, interpolated);
    }

    #[test]
    fn when_interpolating_displaystate_with_t_1_then_state2_is_returned() {
        // GIVEN
        let state1 = Timestamped::new(MockDisplayState(4.0), Timestamp::default() + 2);
        let state2 = Timestamped::new(MockDisplayState(8.0), Timestamp::default() + 5);

        // WHEN
        let interpolated = DisplayState::from_interpolation(&state1, &state2, 1.0);

        // THEN
        assert_eq!(state2, interpolated);
    }
}
