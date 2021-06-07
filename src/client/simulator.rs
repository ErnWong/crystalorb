use crate::{
    fixed_timestepper::FixedTimestepper,
    timestamp::{Timestamp, Timestamped},
    world::World,
    Config,
};

mod reconciliating;
pub use reconciliating::{
    FastforwardingHealth, Reconciliating, ReconciliationInfo, ReconciliationStatus,
};

mod delayed;
pub use delayed::Delayed;

pub trait Simulator: FixedTimestepper {
    type WorldType: World;
    type DisplayStateType<'a>;

    fn new(config: Config, initial_timestamp: Timestamp) -> Self;

    fn display_state<'a>(&'a self) -> Self::DisplayStateType<'a>;

    fn receive_command(&mut self, command: &Timestamped<<Self::WorldType as World>::CommandType>);

    fn receive_snapshot(&mut self, snapshot: Timestamped<<Self::WorldType as World>::SnapshotType>);
}
