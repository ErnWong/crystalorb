#![feature(const_fn_floating_point_arithmetic)]
#![feature(map_first_last)]
#![feature(const_generics)]
#![feature(generic_associated_types)]

pub mod client;
pub mod clocksync;
pub mod command;
pub mod fixed_timestepper;
pub mod network_resource;
pub mod old_new;
pub mod server;
pub mod timestamp;
pub mod world;

/// Represent how to calculate the current client display state based on the simulated undershot
/// and overshot frames (since the two frames closest to the current time may not exactly line up
/// with the current time).
#[derive(Clone)]
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
    pub fn shape_interpolation_t(&self, t: f64) -> f64 {
        assert!((0.0..=1.0).contains(&t));
        match self {
            TweeningMethod::MostRecentlyPassed => t.floor(),
            TweeningMethod::Nearest => t.round(),
            TweeningMethod::Interpolated => t,
        }
    }
}

#[derive(Clone)]
pub struct Config {
    /// Maximum amount of client lag in seconds that the server will compensate for.
    /// A higher number allows a client with a high ping to be able to perform actions
    /// on the world at the correct time from the client's point of view.
    /// The server will only simulate `lag_compensation_latency` seconds in the past,
    /// and let each client predict `lag_compensation_latency` seconds ahead of the server
    /// using the command buffers.
    pub lag_compensation_latency: f64,

    /// When a client receives a snapshot update of the entire world from the server, the client
    /// uses this snapshot to update their simulation. However, immediately displaying the result
    /// of this update will have entities suddenly teleporting to their new destinations. Instead,
    /// we keep simulating the world without the new snapshot information, and slowly fade into the
    /// world with the new snapshot information. We linearly interpolate from the old and new
    /// worlds in `interpolation_latency` seconds.
    pub interpolation_latency: f64,

    pub timestep_seconds: f64,

    /// The number of of clock sync responses from the server before the client averages them
    /// to update the client's clock. The higher this number, the more resilient it is to
    /// jitter, but the longer it takes before the client becomes ready.
    pub clock_sync_needed_sample_count: usize,

    /// The assumed probability that the next clock_sync response sample is an outlier. Before
    /// the samples are averaged, outliers are ignored from the calculation. The number of
    /// samples to ignore is determined by this numebr, with a minimum of one sample ignored
    /// from each extreme (i.e. the max and the min sample gets ignored).
    pub clock_sync_assumed_outlier_rate: f64,

    /// This determines how rapid the client sends clock_sync requests to the server,
    /// represented as the number of seconds before sending the next request.
    pub clock_sync_request_period: f64,

    /// How big the difference needs to be between the following two before updating:
    /// (1) the server clock offset value that the client is currently using for its
    /// calculations, compared with
    /// (2) the current rolling average server clock offset that was measured using clock_sync
    /// messages,
    pub max_tolerable_clock_deviation: f64,

    pub snapshot_send_period: f64,

    pub update_delta_seconds_max: f64,

    pub timestamp_skip_threshold_seconds: f64,

    pub fastforward_max_per_step: usize,

    /// In crystalorb, the physics simulation is assumed to be running at a fixed timestep that is
    /// different to the rendering refresh rate. To suppress some forms of temporal aliasing due to
    /// these different timesteps, crystalorb allows the interpolate between the simulated physics
    /// frames to derive the displayed state.
    pub tweening_method: TweeningMethod,
}

impl Config {
    pub const fn new() -> Self {
        Self {
            lag_compensation_latency: 0.3,
            interpolation_latency: 0.2,
            timestep_seconds: 1.0 / 60.0,
            clock_sync_needed_sample_count: 8,
            clock_sync_request_period: 0.2,
            clock_sync_assumed_outlier_rate: 0.2,
            max_tolerable_clock_deviation: 0.1,
            snapshot_send_period: 0.1,
            update_delta_seconds_max: 0.25,
            timestamp_skip_threshold_seconds: 1.0,
            fastforward_max_per_step: 10,
            tweening_method: TweeningMethod::Interpolated,
        }
    }

    pub fn lag_compensation_frame_count(&self) -> i16 {
        (self.lag_compensation_latency / self.timestep_seconds).round() as i16
    }

    pub fn interpolation_progress_per_frame(&self) -> f64 {
        self.timestep_seconds / self.interpolation_latency
    }

    /// The number of samples N so that, before we calculate the rolling average, we skip the N
    /// lowest and N highest samples.
    pub fn clock_sync_samples_to_discard_per_extreme(&self) -> usize {
        (self.clock_sync_needed_sample_count as f64 * self.clock_sync_assumed_outlier_rate / 2.0)
            .max(1.0)
            .ceil() as usize
    }

    /// The total number of samples that need to be kept in the ring buffer, so that, after
    /// discarding the outliers, there is enough samples to calculate the rolling average.
    pub fn clock_sync_samples_needed_to_store(&self) -> usize {
        self.clock_sync_needed_sample_count + self.clock_sync_samples_to_discard_per_extreme() * 2
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}
