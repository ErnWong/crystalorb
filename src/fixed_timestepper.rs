use crate::{
    timestamp::{FloatTimestamp, Timestamp},
    Config,
};
use std::ops::{Deref, DerefMut};
use tracing::{trace, warn};

/// Arbitrary structure that can be updated in discrete steps.
pub trait Stepper {
    /// Update a single step.
    fn step(&mut self);
}

/// A [Stepper] that has a notion of timestamps that get incremented on each step. Each step
/// "completes" a frame for a particular [Timestamp].
pub trait FixedTimestepper: Stepper {
    /// The [Timestamp] of the frame that was last completed by the previous step.
    fn last_completed_timestamp(&self) -> Timestamp;

    /// Override the [Timestamp].
    fn reset_last_completed_timestamp(&mut self, corrected_timestamp: Timestamp);

    /// Optional template method that [TimeKeeper] can call after completing a batch of step calls.
    fn post_update(&mut self, _timestep_overshoot_seconds: f64) {}
}

/// The method for [TimeKeeper] to decide the last step to call as part of an
/// [update](TimeKeeper::update) call.
#[derive(PartialEq, Eq)]
pub enum TerminationCondition {
    /// [TimeKeeper::update] will stop on or just before the target time. As a result, the internal
    /// overshoot counter will always be zero or negative.
    LastUndershoot,

    /// [TimeKeeper::update] will stop on or just after the target time. As a result, the internal
    /// overshoot counter will always be zero or positive.
    FirstOvershoot,
}

impl TerminationCondition {
    /// Given a [FloatTimestamp], decompose it into an quantised [Timestamp] and the associated
    /// overshoot (in seconds) of that quantised [Timestamp] relative to the target
    /// [FloatTimestamp]. This is what the last completed timestamp and the overshoot of a
    /// [TimeKeeper] would be if it caught up to the given target float timestamp.
    pub fn decompose_float_timestamp(
        &self,
        float_timestamp: FloatTimestamp,
        timestep_seconds: f64,
    ) -> (Timestamp, f64) {
        let timestamp = match self {
            TerminationCondition::LastUndershoot => float_timestamp.floor(),
            TerminationCondition::FirstOvershoot => float_timestamp.ceil(),
        };
        let overshoot_seconds =
            (FloatTimestamp::from(timestamp) - float_timestamp).as_seconds(timestep_seconds);
        (timestamp, overshoot_seconds)
    }
}

/// Given something that can be stepped through in fixed timesteps (aka a [FixedTimestepper]), the
/// [TimeKeeper] provides higher-level functionality that makes sure the stepper is always caught
/// up with the external clock. You can call [TimeKeeper::update] at a framerate different to the
/// internal stepper's fixed timestep, and the [TimeKeeper] will execute an appropriate number of
/// steps on the stepper to meet the external framerate as close as possible.
pub struct TimeKeeper<T: FixedTimestepper, const TERMINATION_CONDITION: TerminationCondition> {
    /// The stepper whose time is managed by this [TimeKeeper].
    stepper: T,

    /// The number of seconds that the stepper has overshooted the requested render timestamp.
    timestep_overshoot_seconds: f64,

    config: Config,
}

impl<T: FixedTimestepper, const TERMINATION_CONDITION: TerminationCondition>
    TimeKeeper<T, TERMINATION_CONDITION>
{
    /// Wrap the given [FixedTimestepper] with a [TimeKeeper] that will manage the stepper's time.
    pub fn new(stepper: T, config: Config) -> Self {
        Self {
            stepper,
            timestep_overshoot_seconds: 0.0,
            config,
        }
    }

    /// Advance by the given time delta. Performs an appropriate number of steps on the stepper to
    /// try and reach the time delta. Stops when it reaches the configured [maximum step
    /// quota](Config::update_delta_seconds_max) to avoid freezing the process. If the stepper's
    /// timestamp desyncs too far away from the expected timestamp calculated from the absolute
    /// `server_seconds_since_startup` time, then this [TimeKeeper] attempts to compensate this
    /// drift by stretching/compressing the given time delta.
    pub fn update(&mut self, delta_seconds: f64, server_seconds_since_startup: f64) {
        let compensated_delta_seconds =
            self.delta_seconds_compensate_for_drift(delta_seconds, server_seconds_since_startup);

        self.advance_stepper(compensated_delta_seconds);
        self.timeskip_if_needed(server_seconds_since_startup);
        self.stepper.post_update(self.timestep_overshoot_seconds);

        trace!("Completed: {:?}", self.stepper.last_completed_timestamp());
    }

    /// Since the stepper can only perform whole numbers of steps, the timekeeper needs to keep
    /// track of the residual fractional number of steps past the "logical time". If we convert the
    /// stepper's timestamp into equivalent seconds, it would rarely align exactly with the actual
    /// current time in seconds. The `current_logical_timestamp` returned here refers to what
    /// theoretical timestep it currently thinks it is at, by combining the actual integer
    /// timestamp with the residual fractional amount the timekeeper has tracked internally.
    pub fn current_logical_timestamp(&self) -> FloatTimestamp {
        FloatTimestamp::from(self.stepper.last_completed_timestamp())
            - FloatTimestamp::from_seconds(
                self.timestep_overshoot_seconds,
                self.config.timestep_seconds,
            )
    }

    /// Calculates what logical timestamp the timekeeper should try to reach, purely based on the
    /// absolute time value.
    pub fn target_logical_timestamp(&self, server_seconds_since_startup: f64) -> FloatTimestamp {
        FloatTimestamp::from_seconds(server_seconds_since_startup, self.config.timestep_seconds)
    }

    /// Calculates the difference between the current logical timestamp and the target logical
    /// timestamp in terms of seconds.
    ///
    /// Positive refers that our timekeeper is ahead of the timestamp it is supposed to be, and
    /// negative refers that our timekeeper needs to catchup.
    ///
    /// Drift can build up due to several reasong:
    /// - Floating point rounding errors (unlikely),
    /// - A [TimeKeeper::update] call with a `delta_seconds` that was too large that it exceeded
    /// the configured update limit. See [Config::update_delta_seconds_max].
    /// - Your external clock itself is drifting, because, for example, it is syncing with another
    /// machine's clock over the network.
    pub fn timestamp_drift_seconds(&self, server_seconds_since_startup: f64) -> f64 {
        let frame_drift = self.current_logical_timestamp()
            - self.target_logical_timestamp(server_seconds_since_startup);
        let seconds_drift = frame_drift.as_seconds(self.config.timestep_seconds);

        trace!(
            "target logical timestamp: {:?}, current logical timestamp: {:?}, drift: {:?} ({} secs)",
            self.target_logical_timestamp(server_seconds_since_startup),
            self.current_logical_timestamp(),
            frame_drift,
            seconds_drift,
        );

        seconds_drift
    }

    fn delta_seconds_compensate_for_drift(
        &self,
        delta_seconds: f64,
        server_seconds_since_startup: f64,
    ) -> f64 {
        let timestamp_drift_seconds = {
            let drift = self.timestamp_drift_seconds(server_seconds_since_startup - delta_seconds);
            if drift.abs() < self.config.timestep_seconds * 0.5 {
                // Deadband to avoid oscillating about zero due to floating point precision. The
                // absolute time (rather than the delta time) is best used for coarse-grained drift
                // compensation.
                0.0
            } else {
                warn!(
                    "Timestamp has drifted by {} seconds. This should not happen too often.",
                    drift
                );
                drift
            }
        };
        let uncapped_compensated_delta_seconds = (delta_seconds - timestamp_drift_seconds).max(0.0);
        let compensated_delta_seconds = if uncapped_compensated_delta_seconds
            > self.config.update_delta_seconds_max
        {
            warn!("Attempted to advance more than the allowed delta seconds ({}). This should not happen too often.", uncapped_compensated_delta_seconds);
            self.config.update_delta_seconds_max
        } else {
            uncapped_compensated_delta_seconds
        };

        trace!(
            "Timestamp drift before advance: {:?}, delta_seconds: {:?}, adjusted delta_seconds: {:?}",
            timestamp_drift_seconds,
            delta_seconds,
            compensated_delta_seconds
        );

        compensated_delta_seconds
    }

    fn advance_stepper(&mut self, delta_seconds: f64) {
        self.timestep_overshoot_seconds -= delta_seconds;
        loop {
            let next_overshoot_seconds =
                self.timestep_overshoot_seconds + self.config.timestep_seconds;
            let termination_compare_value = match &TERMINATION_CONDITION {
                TerminationCondition::LastUndershoot => next_overshoot_seconds,
                TerminationCondition::FirstOvershoot => self.timestep_overshoot_seconds,
            };
            if termination_compare_value >= 0.0 {
                break;
            }
            self.stepper.step();
            self.timestep_overshoot_seconds = next_overshoot_seconds;
        }
    }

    fn timeskip_if_needed(&mut self, server_seconds_since_startup: f64) {
        let drift_seconds = self.timestamp_drift_seconds(server_seconds_since_startup);
        trace!("Timestamp drift after advance: {} sec", drift_seconds,);

        // If drift is too large and we still couldn't keep up, do a time skip.
        if drift_seconds.abs() >= self.config.timestamp_skip_threshold_seconds {
            let (corrected_timestamp, corrected_overshoot_seconds) = TERMINATION_CONDITION
                .decompose_float_timestamp(
                    self.target_logical_timestamp(server_seconds_since_startup),
                    self.config.timestep_seconds,
                );
            warn!(
                "TimeKeeper is too far behind. Skipping timestamp from {:?} to {:?} with overshoot from {} to {}",
                self.stepper.last_completed_timestamp(),
                corrected_timestamp,
                self.timestep_overshoot_seconds,
                corrected_overshoot_seconds,
            );
            self.stepper
                .reset_last_completed_timestamp(corrected_timestamp.into());
            self.timestep_overshoot_seconds = corrected_overshoot_seconds;
        }
    }
}

impl<T: FixedTimestepper, const C: TerminationCondition> Deref for TimeKeeper<T, C> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.stepper
    }
}

impl<T: FixedTimestepper, const C: TerminationCondition> DerefMut for TimeKeeper<T, C> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.stepper
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timestamp;
    use float_cmp::approx_eq;
    use itertools::iproduct;
    use test_env_log::test;
    use tracing::info;

    const CONFIG: Config = Config::new();

    struct MockStepper {
        steps: i16,
        last_completed_timestamp: Timestamp,
    }

    impl MockStepper {
        fn new(initial_timestamp: Timestamp) -> Self {
            Self {
                steps: 0,
                last_completed_timestamp: initial_timestamp,
            }
        }
    }

    impl Stepper for MockStepper {
        fn step(&mut self) {
            self.steps += 1;
            self.last_completed_timestamp.increment();
        }
    }

    impl FixedTimestepper for MockStepper {
        fn last_completed_timestamp(&self) -> Timestamp {
            self.last_completed_timestamp
        }

        fn reset_last_completed_timestamp(&mut self, corrected_timestamp: Timestamp) {
            self.last_completed_timestamp = corrected_timestamp;
        }
    }

    fn assert_approx_eq(lhs: f64, rhs: f64, subtest: &str, message: &str) {
        assert!(
            approx_eq!(f64, lhs, rhs, epsilon = 0.000000000001),
            "{}\n{}\nlhs={}\nrhs={}",
            subtest,
            message,
            lhs,
            rhs
        );
    }

    #[test]
    fn when_update_with_timestamp_drifted_within_the_frame_then_timestamp_drift_is_ignored() {
        for (small_drift_seconds, initial_wrapped_count, initial_timestamp, frames_per_update) in iproduct!(
            &[
                0.0f64,
                CONFIG.timestep_seconds * 0.001f64,
                -CONFIG.timestep_seconds * 0.001f64,
                CONFIG.timestep_seconds * 0.499f64,
                -CONFIG.timestep_seconds * 0.499f64,
            ],
            &[0.0, 1.0],
            &timestamp::tests::interesting_timestamps(),
            &[1.0, 1.7, 2.0, 2.5,]
        ) {
            let subtest = format!(
                "Subtest [drift: {}, wrapped_count: {}, initial timestep: {:?}, frames per update: {}]",
                small_drift_seconds, initial_wrapped_count, initial_timestamp, frames_per_update
            );
            info!("{}", subtest);

            // GIVEN a TimeKeeper starting at an interesting initial timestamp.
            let mut timekeeper: TimeKeeper<MockStepper, { TerminationCondition::FirstOvershoot }> =
                TimeKeeper::new(MockStepper::new(*initial_timestamp), CONFIG);
            let initial_seconds_since_startup = initial_timestamp
                .as_seconds(CONFIG.timestep_seconds)
                + initial_wrapped_count * 16.0f64.exp2() * CONFIG.timestep_seconds;
            assert_approx_eq(
                timekeeper.timestamp_drift_seconds(initial_seconds_since_startup),
                0.0f64,
                &subtest,
                "Precondition: Zero drift from initial time",
            );
            assert_approx_eq(
                timekeeper
                    .timestamp_drift_seconds(initial_seconds_since_startup - small_drift_seconds),
                *small_drift_seconds,
                &subtest,
                "Precondition: Correct drift calculation before update",
            );

            // WHEN updating the TimeKeeper with a drift smaller than half a timestep.
            let delta_seconds = CONFIG.timestep_seconds * frames_per_update;
            let drifted_seconds_since_startup =
                initial_seconds_since_startup + delta_seconds - small_drift_seconds;
            timekeeper.update(delta_seconds, drifted_seconds_since_startup);

            // THEN the TimeKeeper does not correct this time drift.
            assert_approx_eq(
                timekeeper.timestamp_drift_seconds(drifted_seconds_since_startup),
                *small_drift_seconds,
                &subtest,
                "Condition: Drift ignored in update",
            );

            // THEN the TimeKeeper steps through all the needed frames.
            assert_eq!(
                timekeeper.steps,
                frames_per_update.ceil() as i16,
                "{}\nCondition: All needed frames are stepped through",
                subtest
            );
        }
    }

    #[test]
    fn when_update_with_timestamp_drifted_beyond_a_frame_then_timestamp_gets_corrected() {
        for (moderate_drift_seconds, initial_wrapped_count, initial_timestamp, frames_per_update) in iproduct!(
            &[
                CONFIG.timestep_seconds * 0.5f64,
                -CONFIG.timestep_seconds * 0.5f64,
            ],
            &[0.0, 1.0],
            &timestamp::tests::interesting_timestamps(),
            &[1.0, 1.7, 2.0, 2.5]
        ) {
            let subtest = format!(
                "Subtest [drift: {}, wrapped_count: {}, initial timestep: {:?}, frames per update: {}]",
                moderate_drift_seconds, initial_wrapped_count, initial_timestamp, frames_per_update
            );
            info!("{}", subtest);

            // GIVEN a TimeKeeper starting at an interesting initial timestamp.
            let mut timekeeper: TimeKeeper<MockStepper, { TerminationCondition::FirstOvershoot }> =
                TimeKeeper::new(MockStepper::new(*initial_timestamp), CONFIG);
            let initial_seconds_since_startup = initial_timestamp
                .as_seconds(CONFIG.timestep_seconds)
                + initial_wrapped_count * 16.0f64.exp2() * CONFIG.timestep_seconds;
            assert_approx_eq(
                timekeeper.timestamp_drift_seconds(initial_seconds_since_startup),
                0.0f64,
                &subtest,
                "Precondition: Zero drift from initial time",
            );
            assert_approx_eq(
                timekeeper.timestamp_drift_seconds(
                    initial_seconds_since_startup - moderate_drift_seconds,
                ),
                *moderate_drift_seconds,
                &subtest,
                "Precondition: Correct drift calculation before update",
            );

            // WHEN updating the TimeKeeper with a drift at least half a timestep.
            let delta_seconds = CONFIG.timestep_seconds * frames_per_update;
            let drifted_seconds_since_startup =
                initial_seconds_since_startup + delta_seconds - moderate_drift_seconds;
            timekeeper.update(delta_seconds, drifted_seconds_since_startup);

            // THEN all of the drift will be corrected after the update.
            assert_approx_eq(
                timekeeper.timestamp_drift_seconds(drifted_seconds_since_startup),
                0.0f64,
                &subtest,
                "Condition: Drift corrected after update",
            );

            // THEN the TimeKeeper steps through all the needed frames.
            assert_eq!(
                timekeeper.steps,
                i16::from(timekeeper.last_completed_timestamp() - *initial_timestamp),
                "{}\nCondition: All needed frames are stepped through",
                subtest
            );
        }
    }

    #[test]
    fn when_update_with_timestamp_drifting_beyond_threshold_then_timestamps_are_skipped() {
        const MINIMUM_SKIPPABLE_DELTA_SECONDS: f64 =
            CONFIG.timestamp_skip_threshold_seconds + CONFIG.update_delta_seconds_max;
        for (big_drift_seconds, initial_wrapped_count, initial_timestamp, frames_per_update) in iproduct!(
            &[
                MINIMUM_SKIPPABLE_DELTA_SECONDS,
                -MINIMUM_SKIPPABLE_DELTA_SECONDS,
                MINIMUM_SKIPPABLE_DELTA_SECONDS * 2.0,
                -MINIMUM_SKIPPABLE_DELTA_SECONDS * 2.0,
            ],
            &[0.0, 1.0],
            &timestamp::tests::interesting_timestamps(),
            &[1.0, 1.7, 2.0, 2.5]
        ) {
            let subtest = format!(
                "Subtest [drift: {}, wrapped_count: {}, initial timestep: {:?}, frames per update: {}]",
                big_drift_seconds, initial_wrapped_count, initial_timestamp, frames_per_update
            );
            info!("{}", subtest);

            // GIVEN a TimeKeeper starting at an interesting initial timestamp.
            let mut timekeeper: TimeKeeper<MockStepper, { TerminationCondition::FirstOvershoot }> =
                TimeKeeper::new(MockStepper::new(*initial_timestamp), CONFIG);
            let initial_seconds_since_startup = initial_timestamp
                .as_seconds(CONFIG.timestep_seconds)
                + initial_wrapped_count * 16.0f64.exp2() * CONFIG.timestep_seconds;
            assert_approx_eq(
                timekeeper.timestamp_drift_seconds(initial_seconds_since_startup),
                0.0f64,
                &subtest,
                "Precondition: Zero drift from initial time",
            );
            assert_approx_eq(
                timekeeper
                    .timestamp_drift_seconds(initial_seconds_since_startup - big_drift_seconds),
                *big_drift_seconds,
                &subtest,
                "Precondition: Correct drift calculation before update",
            );

            // WHEN updating the TimeKeeper with a drift beyond the timeskip threshold.
            let delta_seconds = CONFIG.timestep_seconds * frames_per_update;
            let drifted_seconds_since_startup =
                initial_seconds_since_startup + delta_seconds - big_drift_seconds;
            timekeeper.update(delta_seconds, drifted_seconds_since_startup);

            // THEN all of the drift will be corrected after the update.
            assert_approx_eq(
                timekeeper.timestamp_drift_seconds(drifted_seconds_since_startup),
                0.0f64,
                &subtest,
                "Condition: Drift corrected after update",
            );

            // THEN the TimeKeeper would not have stepped pass its configured limit.
            let expected_step_count = if big_drift_seconds.is_sign_positive() {
                // If the world is ahead - it can't run any steps.
                0
            } else {
                // Plus 1 since we are overshooting.
                (CONFIG.update_delta_seconds_max / CONFIG.timestep_seconds).ceil() as i16 + 1
            };
            assert_eq!(
                timekeeper.steps, expected_step_count,
                "{}\nCondition: Frames pass the limit are not stepped through",
                subtest
            );
        }
    }

    #[test]
    fn while_updating_with_changing_delta_seconds_then_timestamp_should_not_be_drifting() {
        for (initial_wrapped_count, initial_timestamp) in
            iproduct!(&[0.0, 1.0], &timestamp::tests::interesting_timestamps())
        {
            let subtest = format!(
                "Subtest [wrapped_count: {}, initial timestep: {:?}]",
                initial_wrapped_count, initial_timestamp
            );
            info!("{}", subtest);

            // GIVEN a TimeKeeper starting at an interesting initial timestamp.
            let mut timekeeper: TimeKeeper<MockStepper, { TerminationCondition::FirstOvershoot }> =
                TimeKeeper::new(MockStepper::new(*initial_timestamp), CONFIG);
            let mut seconds_since_startup = initial_timestamp.as_seconds(CONFIG.timestep_seconds)
                + initial_wrapped_count * 16.0f64.exp2() * CONFIG.timestep_seconds;
            assert_approx_eq(
                timekeeper.timestamp_drift_seconds(seconds_since_startup),
                0.0f64,
                &subtest,
                "Precondition: Zero drift from initial time",
            );

            for frames_per_update in &[1.0, 1.7, 0.5, 2.5, 2.0] {
                // WHEN updating the TimeKeeper with different delta_seconds.
                let delta_seconds = CONFIG.timestep_seconds * frames_per_update;
                seconds_since_startup += delta_seconds;
                timekeeper.update(delta_seconds, seconds_since_startup);

                // THEN the time drift should always have remained at zero.
                assert_approx_eq(
                    timekeeper.timestamp_drift_seconds(seconds_since_startup),
                    0.0f64,
                    &subtest,
                    "Condition: Drift remains at zero after update",
                );
            }
        }
    }
}
