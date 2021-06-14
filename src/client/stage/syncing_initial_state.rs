use crate::{network_resource::NetworkResource, timestamp::Timestamp, world::World};
use std::{borrow::Borrow, marker::PhantomData};

use super::ActiveClient;

/// The client interface while the client is in the initial [state syncing
/// stage](super#stage-2---syncing-initial-state-stage).
#[derive(Debug)]
pub struct SyncingInitialState<WorldType, NetworkResourceType, ActiveClientRefType>(
    ActiveClientRefType,
    PhantomData<WorldType>,
    PhantomData<NetworkResourceType>,
)
where
    ActiveClientRefType: Borrow<ActiveClient<WorldType, NetworkResourceType>>,
    WorldType: World,
    NetworkResourceType: NetworkResource;

impl<'a, WorldType: World, NetworkResourceType: NetworkResource>
    From<&'a ActiveClient<WorldType, NetworkResourceType>>
    for SyncingInitialState<
        WorldType,
        NetworkResourceType,
        &'a ActiveClient<WorldType, NetworkResourceType>,
    >
{
    fn from(active_client: &'a ActiveClient<WorldType, NetworkResourceType>) -> Self {
        Self(active_client, PhantomData, PhantomData)
    }
}

impl<'a, WorldType: World, NetworkResourceType: NetworkResource>
    From<&'a mut ActiveClient<WorldType, NetworkResourceType>>
    for SyncingInitialState<
        WorldType,
        NetworkResourceType,
        &'a mut ActiveClient<WorldType, NetworkResourceType>,
    >
{
    fn from(active_client: &'a mut ActiveClient<WorldType, NetworkResourceType>) -> Self {
        Self(active_client, PhantomData, PhantomData)
    }
}

impl<WorldType, NetworkResourceType, ActiveClientRefType>
    SyncingInitialState<WorldType, NetworkResourceType, ActiveClientRefType>
where
    ActiveClientRefType: Borrow<ActiveClient<WorldType, NetworkResourceType>>,
    WorldType: World,
    NetworkResourceType: NetworkResource,
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
