use crate::{fixed_timestepper::Stepper, Config};

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
    /// Configuration parameters that tweak how CrystalOrb works.
    type ConfigType: Config;

    /// Each [`Command`] that the server receives from the client needs to be validated before
    /// being applied as well as relayed to other clients. You can use this method to check that
    /// the client has sufficient permission to issue such command. For example, you can prevent
    /// other clients from moving players that they don't own. Clients can determine their own
    /// `client_id` using the
    /// [`Ready::client_id`](crate::client::stage::Ready::client_id) method from the
    /// [`Ready`](crate::client::stage::Stage::Ready) stage.
    fn command_is_valid<ClientId>(
        command: &<Self::ConfigType as Config>::CommandType,
        client_id: ClientId,
    ) -> bool;

    /// This describes how a [`Command`] affects the [`World`]. Use this method to update your
    /// state. For example, you may want to apply forces/impulses to your rigid bodies, or simply
    /// toggle some of your state variables that will affect how your [`Stepper::step`] will run.
    fn apply_command(&mut self, command: &<Self::ConfigType as Config>::CommandType);

    /// Apply a [snapshot](World::SnapshotType) generated from another [`World`]'s
    /// [`World::snapshot`] method, so that this [`World`] will behave identically to that other
    /// [`World`]. See [`World::SnapshotType`] for more information.
    ///
    /// Typically, the server generates snapshots that clients need to regularly apply.
    fn apply_snapshot(&mut self, snapshot: <Self::ConfigType as Config>::SnapshotType);

    /// Generate the subset of the world state information that is needed to recreate this
    /// [`World`] and have it behave identically to this current [`World`]. See
    /// [`World::SnapshotType`] for more information.
    ///
    /// Typically, the server generates snapshots that clients need to regularly apply.
    fn snapshot(&self) -> <Self::ConfigType as Config>::SnapshotType;

    /// Generate the subset of the world state information that is needed to render the world.
    /// See [`World::DisplayStateType`] and [`DisplayState`].
    fn display_state(&self) -> <Self::ConfigType as Config>::DisplayStateType;
}
