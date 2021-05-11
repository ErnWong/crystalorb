use crate::clocksync::ClockSyncer;
use std::borrow::Borrow;

/// The client interface while the client is in the initial [clock syncing
/// stage](super#stage-1---syncing-clock-stage).
#[derive(Debug)]
pub struct SyncingClock<ClockSyncerRefType>(ClockSyncerRefType)
where
    ClockSyncerRefType: Borrow<ClockSyncer>;

impl<'a> From<&'a ClockSyncer> for SyncingClock<&'a ClockSyncer> {
    fn from(clocksyncer: &'a ClockSyncer) -> Self {
        Self(clocksyncer)
    }
}

impl<'a> From<&'a mut ClockSyncer> for SyncingClock<&'a mut ClockSyncer> {
    fn from(clocksyncer: &'a mut ClockSyncer) -> Self {
        Self(clocksyncer)
    }
}

impl<ClockSyncerRefType> SyncingClock<ClockSyncerRefType>
where
    ClockSyncerRefType: Borrow<ClockSyncer>,
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
