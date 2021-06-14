use crate::{clocksync::ClockSyncer, network_resource::NetworkResource};
use std::{borrow::Borrow, marker::PhantomData};

/// The client interface while the client is in the initial [clock syncing
/// stage](super#stage-1---syncing-clock-stage).
#[derive(Debug)]
pub struct SyncingClock<ClockSyncerRefType, NetworkResourceType>(
    ClockSyncerRefType,
    PhantomData<NetworkResourceType>,
)
where
    ClockSyncerRefType: Borrow<ClockSyncer<NetworkResourceType>>,
    NetworkResourceType: NetworkResource;

impl<'a, NetworkResourceType: NetworkResource> From<&'a ClockSyncer<NetworkResourceType>>
    for SyncingClock<&'a ClockSyncer<NetworkResourceType>, NetworkResourceType>
{
    fn from(clocksyncer: &'a ClockSyncer<NetworkResourceType>) -> Self {
        Self(clocksyncer, PhantomData)
    }
}

impl<'a, NetworkResourceType: NetworkResource> From<&'a mut ClockSyncer<NetworkResourceType>>
    for SyncingClock<&'a mut ClockSyncer<NetworkResourceType>, NetworkResourceType>
{
    fn from(clocksyncer: &'a mut ClockSyncer<NetworkResourceType>) -> Self {
        Self(clocksyncer, PhantomData)
    }
}

impl<ClockSyncerRefType, NetworkResourceType> SyncingClock<ClockSyncerRefType, NetworkResourceType>
where
    ClockSyncerRefType: Borrow<ClockSyncer<NetworkResourceType>>,
    NetworkResourceType: NetworkResource,
{
    /// The number of clock offset samples collected so far.
    pub fn sample_count(&self) -> usize {
        self.0.borrow().sample_count()
    }

    /// The number of clock offset samples needed to make a good estimate on the timing differences
    /// between the client and the server.
    pub fn samples_needed(&self) -> usize {
        self.0.borrow().samples_needed()
    }
}
