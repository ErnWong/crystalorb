use crate::{
    client::{simulator::Simulator, stage::Active},
    timestamp::Timestamp,
};
use std::{borrow::Borrow, marker::PhantomData};

/// The client interface while the client is in the initial [state syncing
/// stage](super#stage-2---syncing-initial-state-stage).
#[derive(Debug)]
pub struct SyncingInitialState<SimulatorType, RefType>(RefType, PhantomData<SimulatorType>)
where
    SimulatorType: Simulator,
    RefType: Borrow<Active<SimulatorType>>;

impl<'a, SimulatorType: Simulator> From<&'a Active<SimulatorType>>
    for SyncingInitialState<SimulatorType, &'a Active<SimulatorType>>
{
    fn from(active_client: &'a Active<SimulatorType>) -> Self {
        Self(active_client, PhantomData)
    }
}

impl<'a, SimulatorType: Simulator> From<&'a mut Active<SimulatorType>>
    for SyncingInitialState<SimulatorType, &'a mut Active<SimulatorType>>
{
    fn from(active_client: &'a mut Active<SimulatorType>) -> Self {
        Self(active_client, PhantomData)
    }
}

impl<SimulatorType, RefType> SyncingInitialState<SimulatorType, RefType>
where
    SimulatorType: Simulator,
    RefType: Borrow<Active<SimulatorType>>,
{
    /// The timestamp of the most recent frame that has completed its simulation.
    /// This is typically one less than [`SyncingInitialState::simulating_timestamp`].
    pub fn last_completed_timestamp(&self) -> Timestamp {
        self.0.borrow().last_completed_timestamp()
    }

    /// The timestamp of the frame that is *in the process* of being simulated.
    /// This is typically one more than [`SyncingInitialState::simulating_timestamp`].
    pub fn simulating_timestamp(&self) -> Timestamp {
        self.0.borrow().simulating_timestamp()
    }
}
