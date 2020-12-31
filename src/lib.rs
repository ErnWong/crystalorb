#![feature(const_fn_floating_point_arithmetic)]

pub mod channels;
pub mod client;
pub mod command;
pub mod events;
pub mod fixed_timestepper;
pub mod old_new;
pub mod server;
pub mod timestamp;
pub mod world;

pub use bevy_networking_turbulence as net;
pub use client::NetworkedPhysicsClientPlugin;
pub use server::NetworkedPhysicsServerPlugin;

#[derive(Clone)]
pub struct Config {
    /// Maximum amount of client lag in seconds that the server will compensate for.
    /// A higher number allows a client with a high ping to be able to perform actions
    /// on the world at the correct time from the client's point of view.
    /// The server will only simulate `lag_compensation_latency` seconds in the past,
    /// and let each client predict `lag_compensation_latency` seconds ahead of the server
    /// using the command buffers.
    pub lag_compensation_latency: f32,

    /// When a client receives a snapshot update of the entire world from the server, the client
    /// uses this snapshot to update their simulation. However, immediately displaying the result
    /// of this update will have entities suddenly teleporting to their new destinations. Instead,
    /// we keep simulating the world without the new snapshot information, and slowly fade into the
    /// world with the new snapshot information. We linearly interpolate from the old and new
    /// worlds in `interpolation_latency` seconds.
    pub interpolation_latency: f32,

    pub timestep_seconds: f32,

    pub timestamp_sync_needed_sample_count: usize,

    pub initial_clock_sync_period: f32,

    pub snapshot_send_period: f32,
}

impl Config {
    pub const fn new() -> Self {
        Self {
            lag_compensation_latency: 0.2,
            interpolation_latency: 0.1,
            timestep_seconds: 1.0 / 60.0,
            timestamp_sync_needed_sample_count: 4,
            initial_clock_sync_period: 0.2,
            snapshot_send_period: 0.2,
        }
    }
    pub fn lag_compensation_frame_count(&self) -> i16 {
        return (self.lag_compensation_latency / self.timestep_seconds).round() as i16;
    }

    pub fn interpolation_progress_per_frame(&self) -> f32 {
        return self.timestep_seconds / self.interpolation_latency;
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
