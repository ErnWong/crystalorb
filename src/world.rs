//! Contains traits for CrystalOrb to interface with your game physics world, along with internal
//! structures that interfaces directly with your world.

mod display_state;
pub use display_state::{DisplayState, Tweened};

mod world_trait;
pub use world_trait::World;

mod simulation;
pub(crate) use simulation::{InitializationType, Simulation};
