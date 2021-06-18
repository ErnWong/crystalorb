#![feature(extended_key_value_attributes)]
#![feature(const_fn_floating_point_arithmetic)]
#![feature(map_first_last)]
#![feature(const_generics)]
#![feature(generic_associated_types)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]
#![warn(clippy::pedantic, clippy::cargo, clippy::unwrap_used)]
#![allow(
    clippy::must_use_candidate,
    clippy::too_many_lines,
    clippy::module_name_repetitions,
    clippy::cast_lossless, // TODO: Fix these. See issue #1
    clippy::cast_sign_loss, // TODO: Fix these. See issue #1
    clippy::cast_possible_truncation, // TODO: Fix these. See issue #1
    clippy::cast_precision_loss, // TODO: Fix these. See issue #1
)]
#![doc = include_str!("../README.markdown")]

use crate::{command::Command, network_resource::NetworkResource, world::DisplayState};
use serde::{de::DeserializeOwned, Serialize};
use std::fmt::Debug;

pub mod client;
pub mod clocksync;
pub mod command;
pub mod fixed_timestepper;
pub mod network_resource;
pub(crate) mod old_new;
pub mod server;
pub mod timestamp;
pub mod world;

/// Represent how to calculate the current client display state based on the simulated undershot
/// and overshot frames (since the two frames closest to the current time may not exactly line up
/// with the current time).
#[derive(Clone, Debug)]
pub enum TweeningMethod {
    /// Use the undershot frame if the simulated frames don't exactly line up to the current time.
    /// This is equivalent to linearly interpolating between the undershot and overshot frame, but
    /// using an interpolation paramater `t.floor()`.
    MostRecentlyPassed,

    /// Use the undershot frame or the overshot frame, whichever is closest to the current time.
    /// This is equivalent to interpolating between the undershot and overshot frame, but
    /// using an interpolation paramater `t.round()`.
    Nearest,

    /// Use the display state's interpolation function to find a suitable in-between display state
    /// between the undershot and overshot frame for the current time.
    Interpolated,
}

impl TweeningMethod {
    /// Depending on the tweening method, conditionally snap the interpolation parameter to 0.0 or
    /// 1.0.
    ///
    /// # Panics
    ///
    /// Asserts that `t` is within `0.0..=1.0`.
    pub fn shape_interpolation_t(&self, t: f64) -> f64 {
        assert!((0.0..=1.0).contains(&t));
        match self {
            TweeningMethod::MostRecentlyPassed => t.floor(),
            TweeningMethod::Nearest => t.round(),
            TweeningMethod::Interpolated => t,
        }
    }
}

/// Configuration parameters that tweak how CrystalOrb works.
///
/// For starters, you can just pass in the default values when you are creating the
/// [`client::Client`] and [`server::Server`] instances.
///
/// # Example
///
/// ```
/// use crystalorb::{Config, client::Client};
/// use crystalorb_demo::DemoWorld;
///
/// let client = Client::<DemoWorld>::new(Config::new());
/// ```
pub trait Config: Debug {
    /// The command that can be used by the game and the player to interact with the physics
    /// simulation. Typically, this is an enum of some kind, but it is up to you.
    type CommandType: Command;

    /// The subset of state information about the world that can be used to fully recreate the
    /// world. Needs to be serializable so that it can be sent from the server to the client. There
    /// is nothing stopping you from making the [`SnapshotType`](World::SnapshotType) the same type
    /// as your [`World`] if you are ok with serializing the entire physics engine state. If you
    /// think sending your entire [`World`] is a bit too heavy-weighted, you can hand-craft and
    /// optimise your own `SnapshotType` structure to be as light-weight as possible.
    type SnapshotType: Debug + Clone + Serialize + DeserializeOwned + Send + Sync + 'static;

    /// The subset of state information about the world that is to be displayed/rendered. This is
    /// used by the client to create the perfect blend of state information for the current
    /// rendering frame. You could make this [`DisplayState`](World::DisplayStateType) the same
    /// structure as your [`World`] if you don't mind CrystalOrb making lots of copies of your
    /// entire [`World`] structure and performing interpolation on all your state variables.
    type DisplayStateType: DisplayState;

    /// Externam networking implementation used to transmit CrystalOrb data packets.
    type NetworkResourceType: NetworkResource;

    /// Maximum amount of client lag in seconds that the server will compensate for.
    /// A higher number allows a client with a high ping to be able to perform actions
    /// on the world at the correct time from the client's point of view.
    /// The server will only simulate `lag_compensation_latency` seconds in the past,
    /// and let each client predict `lag_compensation_latency` seconds ahead of the server
    /// using the command buffers.
    const LAG_COMPENSATION_LATENCY: f64 = 0.3;

    /// When a client receives a snapshot update of the entire world from the server, the client
    /// uses this snapshot to update their simulation. However, immediately displaying the result
    /// of this update will have entities suddenly teleporting to their new destinations. Instead,
    /// we keep simulating the world without the new snapshot information, and slowly blend into the
    /// world with the new snapshot information. We linearly interpolate from the old and new
    /// worlds, taking `blend_latency` seconds before the user sees the entities in their
    /// updated locations.
    const BLEND_LATENCY: f64 = 0.2;

    /// The number of seconds that gets simulated with every [`World`](world::World)
    /// [`step`](fixed_timestepper::Stepper). This is the physics simulation "dt". This can be
    /// different to the `delta_seconds` between each
    /// [`Client::update`](client::Client::update)/[`Server::update`](server::Server::update) call.
    /// For more accurate and predictable physics simulations, you may want to have a smaller
    /// `timestep_seconds`. If your physics simulation is computationally intensive, you may want
    /// to have a larger `timestep_seconds`.
    const TIMESTEP_SECONDS: f64 = 1.0 / 60.0;

    /// The number of of clock sync responses from the server before the client averages them
    /// to update the client's clock. The higher this number, the more resilient it is to
    /// jitter, but the longer it takes before the client becomes ready.
    const CLOCK_SYNC_NEEDED_SAMPLE_COUNT: usize = 8;

    /// The assumed probability that the next clock_sync response sample is an outlier. Before
    /// the samples are averaged, outliers are ignored from the calculation. The number of
    /// samples to ignore is determined by this numebr, with a minimum of one sample ignored
    /// from each extreme (i.e. the max and the min sample gets ignored).
    const CLOCK_SYNC_ASSUMED_OUTLIER_RATE: f64 = 0.2;

    /// This determines how rapid the client sends clock_sync requests to the server,
    /// represented as the number of seconds before sending the next request.
    const CLOCK_SYNC_REQUEST_PERIOD: f64 = 0.2;

    /// How big the difference needs to be between the following two:
    /// 1. the server clock offset value that the client is currently using for its
    ///    calculations, compared with
    /// 2. the current rolling average server clock offset that was measured using clock_sync
    ///    messages,
    /// before updating the clients.
    const MAX_TOLERABLE_CLOCK_DEVIATION: f64 = 0.1;

    /// How many seconds to wait before the server sends another snapshot out to the clients. The
    /// smaller this number, the higher the network traffic, but the sooner the client gets to
    /// correctly apply other clients' commands at the correct timestmp. Regarding the latter
    /// point, this is because commands take time to travel between clients, and by the time a
    /// client receives another client's command, the command's intended timestamp would have
    /// already been passed. The client can't rewind the world to reapply the command at the
    /// intended timestamp, so the best thing it can do is to apply at the current timestamp, and
    /// wait until the next server snapshot before finally applying the command at the intended
    /// timestamp during the snapshot fastforwarding process.
    const SNAPSHOT_SEND_PERIOD: f64 = 0.1;

    /// This is the limit that CrystalOrb enforces to limit the number of times that the
    /// [`World`](world::World)'s [`step`](crate::fixed_timestepper::Stepper::step) function gets
    /// called per rendering frame (i.e. per [`Client::update`](crate::client::Client::update) or
    /// [`Server::update`](crate::server::Server::update)). This is to prevent a catastrophic
    /// situation where CrystalOrb could not keep up in one rendering frame, and making it worse in
    /// the next frame, and subsequently "freezing" up in a positive feedback loop.
    const UPDATE_DELTA_SECONDS_MAX: f64 = 0.25;

    /// Due to floating-point rounding errors and due to the `update_delta_seconds_max` limit
    /// that CrystalOrb enforces, the simulation [`Timestamp`](timestamp::Timestamp) might
    /// slowly drift away from the intended value according to the system clock (which is a way
    /// to make sure that different clients on different machines stay in sync in terms of time).
    /// When this time drift is small, CrystalOrb can compensate it in the next update by running
    /// extra simulation frames or some fewer frames than usual. However, if the timestamp drift
    /// becomes too large, it is not possible to correct all of this drift in one update using this
    /// method, and instead we need to perform a "time-skip" without running the corresponding
    /// simulation steps. The threshold for this drift before we start skipping some simulation
    /// frames is defined by this configuration parameter.
    ///
    /// Large timestamp drifts could happen when, for example, the game client has some system
    /// lag. If the game client is hosted in the web browser, for exmple, then large timestamp
    /// drifts can occur when the browser tab goes out of focus and sleeps. Large timestamp
    /// drifts can also occur on the laptop when the computer enters sleep mode, etc.
    const TIMESTAMP_SKIP_THRESHOLD_SECONDS: f64 = 1.0;

    /// When the [`Client`](client::Client) receives a [`Snapshot`](world::World::SnapshotType)
    /// from the [`Server`](server::Server), the snapshot is going to have a timestamp older than
    /// the current client simulation timestamp. Before the snapshot can be blended into the
    /// client, it needs to fastforwarded to the current timestamp by running more simulation frmes
    /// on the snapshot than on the world that is currently displayed on the client. This
    /// configuration parameter specifies how much faster the snapshot can be simulated compared
    /// with the existing client world during the fastforwarding process. A value of `2`, for
    /// example, would represent that the received server snapshot could only run at most `2`
    /// simulation steps every time the existing client world runs one step. In this scenario, if
    /// the server snapshot is `10` frames behind the client, then it would take `10` client frames
    /// before the server snapshot ctches up with the client.
    const FASTFORWARD_MAX_PER_STEP: usize = 10;

    /// In crystalorb, the physics simulation is assumed to be running at a fixed timestep that is
    /// different to the rendering refresh rate. To suppress some forms of temporal aliasing due to
    /// these different timesteps, crystalorb allows the interpolate between the simulated physics
    /// frames to derive the displayed state.
    const TWEENING_METHOD: TweeningMethod = TweeningMethod::Interpolated;

    /// Re-expresses the amount of lag to compensate in terms of number of frames at the prescribed
    /// timestep.
    fn lag_compensation_frame_count() -> i16 {
        (Self::LAG_COMPENSATION_LATENCY / Self::TIMESTEP_SECONDS).round() as i16
    }

    /// Re-expresses the speed of blending the server snapshot in terms of the amount that the
    /// interpolation parameter `t` needs to be incremented on each step.
    fn blend_progress_per_frame() -> f64 {
        Self::TIMESTEP_SECONDS / Self::BLEND_LATENCY
    }

    /// The number of samples N so that, before we calculate the rolling average, we skip the N
    /// lowest and N highest samples.
    fn clock_sync_samples_to_discard_per_extreme() -> usize {
        (Self::CLOCK_SYNC_NEEDED_SAMPLE_COUNT as f64 * Self::CLOCK_SYNC_ASSUMED_OUTLIER_RATE / 2.0)
            .max(1.0)
            .ceil() as usize
    }

    /// The total number of samples that need to be kept in the ring buffer, so that, after
    /// discarding the outliers, there is enough samples to calculate the rolling average.
    fn clock_sync_samples_needed_to_store() -> usize {
        Self::CLOCK_SYNC_NEEDED_SAMPLE_COUNT + Self::clock_sync_samples_to_discard_per_extreme() * 2
    }
}
