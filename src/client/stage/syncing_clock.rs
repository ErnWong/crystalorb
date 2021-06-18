use crate::{clocksync::ClockSyncer, Config};
use std::{borrow::Borrow, marker::PhantomData};

/// The client interface while the client is in the initial [clock syncing
/// stage](super#stage-1---syncing-clock-stage).
#[derive(Debug)]
pub struct SyncingClock<ClockSyncerRefType, ConfigType>(
    ClockSyncerRefType,
    PhantomData<ConfigType>,
)
where
    ClockSyncerRefType: Borrow<ClockSyncer<ConfigType>>,
    ConfigType: Config;

impl<'a, ConfigType: Config> From<&'a ClockSyncer<ConfigType>>
    for SyncingClock<&'a ClockSyncer<ConfigType>, ConfigType>
{
    fn from(clocksyncer: &'a ClockSyncer<ConfigType>) -> Self {
        Self(clocksyncer, PhantomData)
    }
}

impl<'a, ConfigType: Config> From<&'a mut ClockSyncer<ConfigType>>
    for SyncingClock<&'a mut ClockSyncer<ConfigType>, ConfigType>
{
    fn from(clocksyncer: &'a mut ClockSyncer<ConfigType>) -> Self {
        Self(clocksyncer, PhantomData)
    }
}

impl<ClockSyncerRefType, ConfigType> SyncingClock<ClockSyncerRefType, ConfigType>
where
    ClockSyncerRefType: Borrow<ClockSyncer<ConfigType>>,
    ConfigType: Config,
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
