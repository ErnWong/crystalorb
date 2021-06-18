use crate::{timestamp::Timestamp, world::World};
use std::{borrow::Borrow, marker::PhantomData};

use super::ActiveClient;

/// The client interface while the client is in the initial [state syncing
/// stage](super#stage-2---syncing-initial-state-stage).
#[derive(Debug)]
pub struct SyncingInitialState<WorldType, ActiveClientRefType>(
    ActiveClientRefType,
    PhantomData<WorldType>,
)
where
    ActiveClientRefType: Borrow<ActiveClient<WorldType>>,
    WorldType: World;

impl<'a, WorldType: World> From<&'a ActiveClient<WorldType>>
    for SyncingInitialState<WorldType, &'a ActiveClient<WorldType>>
{
    fn from(active_client: &'a ActiveClient<WorldType>) -> Self {
        Self(active_client, PhantomData)
    }
}

impl<'a, WorldType: World> From<&'a mut ActiveClient<WorldType>>
    for SyncingInitialState<WorldType, &'a mut ActiveClient<WorldType>>
{
    fn from(active_client: &'a mut ActiveClient<WorldType>) -> Self {
        Self(active_client, PhantomData)
    }
}

impl<WorldType, ActiveClientRefType> SyncingInitialState<WorldType, ActiveClientRefType>
where
    ActiveClientRefType: Borrow<ActiveClient<WorldType>>,
    WorldType: World,
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
