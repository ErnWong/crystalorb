//! The [`Client`][client] undergoes several stages of initialization before it is ready to accept
//! commands and be displayed to the player's screen. The enums in this module provides access to
//! different functionality depending on what stage the [`Client`][client] is at:
//!
//! # Stages
//!
//! ## Stage 1 - Syncing Clock Stage
//!
//! In this first stage, the [`Client`][client] tries to figure out the clock differences between
//! the local machine and the server machine. The [`Client`][client] collects a handful of "clock
//! offset" samples until it reaches the amount needed to make a good estimate. This timing
//! difference is necessary so that the [`Client`][client] can talk to the [`Server`][server] about
//! the timing of player commands and snapshots.
//!
//! You can observe the clock syncing progress by checking the [number of samples collected so
//! far](SyncingClock::sample_count) and comparing that number with the [number of samples that are
//! needed](SyncingClock::samples_needed).
//!
//! ## Stage 2 - Syncing Initial State Stage
//!
//! The second stage is where the [`Client`][client] waits for the first [`Server`][server]
//! [snapshot][snapshot] to arrive, fastforwarded, and generally be fully processed through the
//! client pipeline so that it can be shown on the screen, to flush out all the uninitialized
//! simulation state.
//!
//! ## Stage 3 - Ready Stage
//!
//! The third and final stage is where the [`Client`][client] is now ready for its [display
//! state](Ready::display_state) to be shown onto the screen, and where the [`Client`][client] is
//! ready to accept commands from the user.
//!
//! [client]: crate::client::Client
//! [server]: crate::server::Server
//! [snapshot]: crate::world::World::SnapshotType

use crate::{clocksync::ClockSyncer, network_resource::NetworkResource, world::World, Config};

mod syncing_clock;
pub use syncing_clock::SyncingClock;

mod syncing_initial_state;
pub use syncing_initial_state::SyncingInitialState;

mod ready;
pub use ready::Ready;

use super::ActiveClient;

/// The internal, owned stage. Hosts the client's states and structures depending on what stage the
/// client is at.
///
/// See the [module-level documentation](self) for more information.
#[derive(Debug)]
pub(crate) enum StageOwned<WorldType: World> {
    /// The [first stage][module-stage].
    ///
    /// See the relevant section in the [module-level documentation][module-stage] for more
    /// info.
    ///
    /// [module-stage]: self#stage-1---syncing-clock-stage
    SyncingClock(ClockSyncer),

    /// The [second stage][module-stage].
    ///
    /// See the relevant section in the [module-level documentation][module-stage] for more
    /// info.
    ///
    /// [module-stage]: self#stage-2---syncing-initial-state-stage
    SyncingInitialState(ActiveClient<WorldType>),

    /// The [third stage][module-stage].
    ///
    /// See the relevant section in the [module-level documentation][module-stage] for more
    /// info.
    ///
    /// [module-stage]: self#stage-3---ready-stage
    Ready(ActiveClient<WorldType>),
}

impl<WorldType: World> StageOwned<WorldType> {
    pub fn update<NetworkResourceType: NetworkResource<WorldType>>(
        &mut self,
        delta_seconds: f64,
        seconds_since_startup: f64,
        config: &Config,
        net: &mut NetworkResourceType,
    ) {
        let should_transition = match self {
            StageOwned::SyncingClock(clocksyncer) => {
                clocksyncer.update(delta_seconds, seconds_since_startup, net);
                clocksyncer.is_ready()
            }
            StageOwned::SyncingInitialState(client) => {
                client.update(delta_seconds, seconds_since_startup, net);
                client.is_ready()
            }
            StageOwned::Ready(client) => {
                client.update(delta_seconds, seconds_since_startup, net);
                false
            }
        };

        if should_transition {
            let config = config.clone();
            take_mut::take(self, |stage| match stage {
                StageOwned::SyncingClock(clocksyncer) => StageOwned::SyncingInitialState(
                    ActiveClient::new(seconds_since_startup, config, clocksyncer),
                ),
                StageOwned::SyncingInitialState(client) => StageOwned::Ready(client),
                StageOwned::Ready(_) => unreachable!(),
            });
        }
    }
}

impl<'a, WorldType: World> From<&'a StageOwned<WorldType>> for Stage<'a, WorldType> {
    fn from(stage: &'a StageOwned<WorldType>) -> Stage<'a, WorldType> {
        match stage {
            StageOwned::SyncingClock(clocksyncer) => Stage::SyncingClock(clocksyncer.into()),
            StageOwned::SyncingInitialState(active_client) => {
                Stage::SyncingInitialState(active_client.into())
            }
            StageOwned::Ready(active_client) => Stage::Ready(active_client.into()),
        }
    }
}

impl<'a, WorldType: World> From<&'a mut StageOwned<WorldType>> for StageMut<'a, WorldType> {
    fn from(stage: &'a mut StageOwned<WorldType>) -> StageMut<'a, WorldType> {
        match stage {
            StageOwned::SyncingClock(clocksyncer) => StageMut::SyncingClock(clocksyncer.into()),
            StageOwned::SyncingInitialState(active_client) => {
                StageMut::SyncingInitialState(active_client.into())
            }
            StageOwned::Ready(active_client) => StageMut::Ready(active_client.into()),
        }
    }
}

/// This is the immutable view of the client's [stage](self) that is returned by
/// [`Client::stage`][get-stage].
///
/// See [`Client::stage_mut`][get-stagemut] and [`StageMut`] for mutable access.
///
/// See the [module-level documentation](self) for more information.
///
/// [get-stage]: crate::client::Client::stage
/// [get-stagemut]: crate::client::Client::stage_mut
#[derive(Debug)]
pub enum Stage<'a, WorldType: World> {
    /// The [first stage][module-stage].
    ///
    /// See the relevant section in the [module-level documentation][module-stage] for more
    /// info.
    ///
    /// [module-stage]: self#stage-1---syncing-clock-stage
    SyncingClock(SyncingClock<&'a ClockSyncer>),

    /// The [second stage][module-stage].
    ///
    /// See the relevant section in the [module-level documentation][module-stage] for more
    /// info.
    ///
    /// [module-stage]: self#stage-2---syncing-initial-state-stage
    SyncingInitialState(SyncingInitialState<WorldType, &'a ActiveClient<WorldType>>),

    /// The [third stage][module-stage].
    ///
    /// See the relevant section in the [module-level documentation][module-stage] for more
    /// info.
    ///
    /// [module-stage]: self#stage-3---ready-stage
    Ready(Ready<WorldType, &'a ActiveClient<WorldType>>),
}

/// This is the mutable view of the client's [stage](self) that is returned by
/// [`Client::stage_mut`][get-stagemut].
///
/// See [`Client::stage`][get-stage] and [`Stage`] for immutable, shareable access.
///
/// See the [module-level documentation](self) for more information.
///
/// [get-stage]: crate::client::Client::stage
/// [get-stagemut]: crate::client::Client::stage_mut
#[derive(Debug)]
pub enum StageMut<'a, WorldType: World> {
    /// The [first stage][module-stage].
    ///
    /// See the relevant section in the [module-level documentation][module-stage] for more
    /// info.
    ///
    /// [module-stage]: self#stage-1---syncing-clock-stage
    SyncingClock(SyncingClock<&'a mut ClockSyncer>),

    /// The [second stage][module-stage].
    ///
    /// See the relevant section in the [module-level documentation][module-stage] for more
    /// info.
    ///
    /// [module-stage]: self#stage-2---syncing-initial-state-stage
    SyncingInitialState(SyncingInitialState<WorldType, &'a mut ActiveClient<WorldType>>),

    /// The [third stage][module-stage].
    ///
    /// See the relevant section in the [module-level documentation][module-stage] for more
    /// info.
    ///
    /// [module-stage]: self#stage-3---ready-stage
    Ready(Ready<WorldType, &'a mut ActiveClient<WorldType>>),
}
