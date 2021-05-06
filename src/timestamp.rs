//! Types for identifying/indexing and comparing the time between simulation frames.
//!
//! Many things in CrystalOrb are timestamped. Each frame of a [`World`](crate::world::World)
//! simulation are assigned a [`Timestamp`]. The corresponding
//! [snapshots](crate::world::World::SnapshotType), [commands](crate::world::World::CommandType),
//! and [display states](crate::world::World::DisplayStateType) are all timestamped so that the
//! client and server knows which simulation frame they are associated with.

use crate::fixed_timestepper::Stepper;
use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    fmt::{Debug, Display, Formatter, Result},
    num::Wrapping,
    ops::{Add, Deref, DerefMut, Range, Sub},
};

/// Represents and identifies a simulation instant.
#[derive(Eq, PartialEq, Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct Timestamp(Wrapping<i16>);

impl Timestamp {
    /// See note about transitivity for Timestamp's Ord implementation.
    pub const MAX_COMPARABLE_RANGE: i16 = i16::MAX;

    /// Find the corresponding timestamp for the current time in seconds.
    pub fn from_seconds(seconds: f64, timestep_seconds: f64) -> Self {
        Self::from(FloatTimestamp::from_seconds(seconds, timestep_seconds))
    }

    /// Modify itself to become the timestamp of the next frame.
    pub fn increment(&mut self) {
        self.0 += Wrapping(1);
    }

    /// Find the corresponding time in seconds for this timestamp. Since timestamps repeat over
    /// time, this function returns the time closest to zero. This makes it useful to find the
    /// number of seconds between two timestamps.
    ///
    /// # Example
    ///
    /// ```
    /// use crystalorb::timestamp::Timestamp;
    /// use float_cmp::approx_eq;
    /// const TIMESTEP: f64 = 1.0 / 60.0;
    ///
    /// // Given two timestamps.
    /// let t1 = Timestamp::default();
    /// let t2 = t1 + 50;
    ///
    /// // We can get the seconds between these two timestamps.
    /// let seconds_difference = (t2 - t1).as_seconds(TIMESTEP);
    /// assert!(approx_eq!(f64, seconds_difference, 50.0 / 60.0, ulps=1));
    /// ```
    pub fn as_seconds(self, timestep_seconds: f64) -> f64 {
        self.0 .0 as f64 * timestep_seconds
    }

    /// See note about transitivity for Timestamp's Ord implementation.
    pub fn comparable_range_with_midpoint(midpoint: Timestamp) -> Range<Timestamp> {
        let max_distance_from_midpoint = Self::MAX_COMPARABLE_RANGE / 2;
        (midpoint - max_distance_from_midpoint)..(midpoint + max_distance_from_midpoint)
    }
}

impl From<FloatTimestamp> for Timestamp {
    fn from(float_timestamp: FloatTimestamp) -> Self {
        Self(Wrapping(float_timestamp.0 as i16))
    }
}

impl Add<i16> for Timestamp {
    type Output = Self;
    fn add(self, rhs: i16) -> Self::Output {
        Self(self.0 + Wrapping(rhs))
    }
}

impl Sub<i16> for Timestamp {
    type Output = Self;
    fn sub(self, rhs: i16) -> Self::Output {
        Self(self.0 - Wrapping(rhs))
    }
}

impl Sub<Timestamp> for Timestamp {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

impl Ord for Timestamp {
    /// Note: This is technically not transitive, since we are doing wrapped differences.
    /// To guarantee transitivity (for example, to use in `BTreeMap`s), ensure that all values being
    /// compared against each other are at most `std::i16::MAX` length of each other.
    /// (Maybe `std::i16::MAX` is off by one, but it is at least on the conservative side)
    fn cmp(&self, other: &Self) -> Ordering {
        let difference: Wrapping<i16> = self.0 - other.0;
        match difference {
            d if d < Wrapping(0) => Ordering::Less,
            d if d == Wrapping(0) => Ordering::Equal,
            d if d > Wrapping(0) => Ordering::Greater,
            _ => unreachable!(),
        }
    }
}

impl PartialOrd for Timestamp {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl From<Timestamp> for i16 {
    fn from(timestamp: Timestamp) -> i16 {
        timestamp.0 .0
    }
}

impl Display for Timestamp {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "T={:+06}", self.0 .0)
    }
}

/// Representation of time in the same units as [`Timestamp`], but whereas [`Timestamp`] identifies
/// which whole number of frames only, [`FloatTimestamp`] can represent any time in the continuous
/// region between two adjacent frames.
#[derive(PartialEq, Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct FloatTimestamp(f64);

impl FloatTimestamp {
    /// Convert the time from seconds into [`Timestamp`] units (1 per frame, i.e. 1 per
    /// timestep), and fit it into the [`Timestamp`] space.
    pub fn from_seconds(seconds: f64, timestep_seconds: f64) -> Self {
        Self::from_unwrapped(seconds / timestep_seconds)
    }

    /// Fit the time in [`Timestamp`] units into the [`Timestamp`] space by wrapping.
    pub fn from_unwrapped(frames: f64) -> Self {
        let frames_wrapped =
            (frames + 15.0_f64.exp2()).rem_euclid(16.0_f64.exp2()) - 15.0_f64.exp2();
        Self(frames_wrapped)
    }

    /// Find the corresponding time in seconds for this float timestamp. Since timestamps
    /// repeat over time, this function returns the time closest to zero. This makes it useful
    /// to find the number of seconds between two float timestamps.
    ///
    /// # Example
    ///
    /// ```
    /// use crystalorb::timestamp::FloatTimestamp;
    /// use float_cmp::approx_eq;
    /// const TIMESTEP: f64 = 1.0 / 60.0;
    ///
    /// // Given two float timestamps.
    /// let t1 = FloatTimestamp::from_unwrapped(123.2);
    /// let t2 = FloatTimestamp::from_unwrapped(123.7);
    ///
    /// // We can get the seconds between these two float timestamps.
    /// let seconds_difference = (t2 - t1).as_seconds(TIMESTEP);
    /// assert!(approx_eq!(f64, seconds_difference, 0.5 / 60.0, ulps=1));
    /// ```
    pub fn as_seconds(self, timestep_seconds: f64) -> f64 {
        self.0 * timestep_seconds
    }

    /// Round up to the next whole-number [`Timestamp`] (or its own value if it is already a
    /// whole number).
    ///
    /// # Example
    ///
    /// ```
    /// use crystalorb::timestamp::{FloatTimestamp, Timestamp};
    ///
    /// let t1 = FloatTimestamp::from_unwrapped(123.4);
    /// let t2 = FloatTimestamp::from_unwrapped(123.0);
    ///
    /// assert_eq!(t1.ceil(), Timestamp::default() + 124);
    /// assert_eq!(t2.ceil(), Timestamp::default() + 123);
    /// ```
    pub fn ceil(self) -> Timestamp {
        Timestamp(Wrapping(self.0.ceil() as i16))
    }

    /// Round down to the previous whole-number [`Timestamp`] (or its own value if it is
    /// already a whole number).
    ///
    /// # Example
    ///
    /// ```
    /// use crystalorb::timestamp::{FloatTimestamp, Timestamp};
    ///
    /// let t1 = FloatTimestamp::from_unwrapped(123.4);
    /// let t2 = FloatTimestamp::from_unwrapped(123.0);
    ///
    /// assert_eq!(t1.floor(), Timestamp::default() + 123);
    /// assert_eq!(t2.floor(), Timestamp::default() + 123);
    /// ```
    pub fn floor(self) -> Timestamp {
        Timestamp(Wrapping(self.0.floor() as i16))
    }
}

impl Sub<FloatTimestamp> for FloatTimestamp {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Self::from_unwrapped(self.0 - rhs.0)
    }
}

impl From<Timestamp> for FloatTimestamp {
    fn from(timestamp: Timestamp) -> FloatTimestamp {
        FloatTimestamp(timestamp.0 .0 as f64)
    }
}

/// Associate a [`Timestamp`] with another type.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Timestamped<T> {
    inner: T,
    timestamp: Timestamp,
}

impl<T> Timestamped<T> {
    /// Wrap the given data with the given [`Timestamp`].
    pub fn new(inner: T, timestamp: Timestamp) -> Self {
        Self { inner, timestamp }
    }

    /// Get a reference to the inner data without the [`Timestamp`].
    pub fn inner(&self) -> &T {
        &self.inner
    }

    /// Get a mutable reference to the inner data without the [`Timestamp`].
    pub fn inner_mut(&mut self) -> &mut T {
        &mut self.inner
    }

    /// Get the associated [`Timestamp`].
    pub fn timestamp(&self) -> Timestamp {
        self.timestamp
    }

    /// Update the current [`Timestamp`] associated with the inner piece of data.
    pub fn set_timestamp(&mut self, timestamp: Timestamp) {
        self.timestamp = timestamp;
    }
}

impl<T: Stepper> Stepper for Timestamped<T> {
    fn step(&mut self) {
        self.inner.step();
        self.timestamp.increment();
    }
}

impl<T> Deref for Timestamped<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> DerefMut for Timestamped<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub fn interesting_timestamps() -> [Timestamp; 7] {
        [
            Timestamp::default() + std::i16::MIN,
            Timestamp::default() + std::i16::MIN / 2,
            Timestamp::default() - 1,
            Timestamp::default(),
            Timestamp::default() + 1,
            Timestamp::default() + std::i16::MAX / 2,
            Timestamp::default() + std::i16::MAX,
        ]
    }

    struct InterestingOffsets {
        plus_one: Timestamp,
        plus_limit: Timestamp,
        plus_wrapped: Timestamp,
        plus_wrapped_limit: Timestamp,
        plus_wrapped_full: Timestamp,
        minus_one: Timestamp,
        minus_limit: Timestamp,
        minus_wrapped: Timestamp,
        minus_wrapped_limit: Timestamp,
        minus_wrapped_full: Timestamp,
    }

    fn generate_interesting_offsets(initial: Timestamp) -> InterestingOffsets {
        let plus_one = initial + 1;
        let plus_limit = initial + i16::MAX;
        let plus_wrapped = plus_limit + 1;
        let plus_wrapped_limit = plus_limit - i16::MIN;
        let plus_wrapped_full = plus_wrapped_limit + 1;

        let minus_one = initial - 1;
        let minus_limit = initial + i16::MIN;
        let minus_wrapped = minus_limit - 1;
        let minus_wrapped_limit = minus_limit - i16::MAX;
        let minus_wrapped_full = minus_wrapped_limit - 1;

        InterestingOffsets {
            plus_one,
            plus_limit,
            plus_wrapped,
            plus_wrapped_limit,
            plus_wrapped_full,
            minus_one,
            minus_limit,
            minus_wrapped,
            minus_wrapped_limit,
            minus_wrapped_full,
        }
    }

    #[test]
    fn test_timestamp_ord() {
        fn test_timestamp_ord_with_initial(initial: Timestamp) {
            let offsets = generate_interesting_offsets(initial);
            assert!(offsets.plus_one > initial);
            assert!(offsets.plus_limit > initial);
            assert!(offsets.plus_wrapped < initial);
            assert!(offsets.plus_wrapped_limit < initial);
            assert!(offsets.plus_wrapped_full == initial);
            assert!(offsets.minus_one < initial);
            assert!(offsets.minus_limit < initial);
            assert!(offsets.minus_wrapped > initial);
            assert!(offsets.minus_wrapped_limit > initial);
            assert!(offsets.minus_wrapped_full == initial);
        }

        for timestamp in &interesting_timestamps() {
            test_timestamp_ord_with_initial(*timestamp);
        }
    }

    #[test]
    fn test_timestamp_difference() {
        fn test_timestamp_difference_with_initial(initial: Timestamp) {
            let offsets = generate_interesting_offsets(initial);
            assert_eq!(offsets.plus_one - initial, Timestamp::default() + 1);
            assert_eq!(
                offsets.plus_limit - initial,
                Timestamp::default() + i16::MAX
            );
            assert_eq!(
                offsets.plus_wrapped - initial,
                Timestamp::default() + i16::MIN
            );
            assert_eq!(
                offsets.plus_wrapped_limit - initial,
                Timestamp::default() - 1
            );
            assert_eq!(offsets.plus_wrapped_full - initial, Timestamp::default());
            assert_eq!(offsets.minus_one - initial, Timestamp::default() - 1);
            assert_eq!(
                offsets.minus_limit - initial,
                Timestamp::default() + i16::MIN
            );
            assert_eq!(
                offsets.minus_wrapped - initial,
                Timestamp::default() + i16::MAX
            );
            assert_eq!(
                offsets.minus_wrapped_limit - initial,
                Timestamp::default() + 1
            );
            assert_eq!(offsets.minus_wrapped_full - initial, Timestamp::default());
        }

        for timestamp in &interesting_timestamps() {
            test_timestamp_difference_with_initial(*timestamp);
        }
    }

    #[test]
    fn test_timestamp_increment() {
        for timestamp in &interesting_timestamps() {
            let mut incremented = timestamp.clone();
            incremented.increment();
            assert!(incremented > *timestamp);
            assert_eq!(incremented - *timestamp, Timestamp::default() + 1);
        }
    }

    #[test]
    fn test_timestamp_from_seconds() {
        assert_eq!(Timestamp::from_seconds(0.0, 1.0), Timestamp::default());
        assert_eq!(Timestamp::from_seconds(1.0, 1.0), Timestamp::default() + 1);
        assert_eq!(
            Timestamp::from_seconds(0.25, 0.25),
            Timestamp::default() + 1
        );
        assert_eq!(Timestamp::from_seconds(-1.0, 1.0), Timestamp::default() - 1);
        assert_eq!(
            Timestamp::from_seconds(i16::MAX as f64, 1.0),
            Timestamp::default() + i16::MAX,
        );
        assert_eq!(
            Timestamp::from_seconds((i16::MAX as f64) + 1.0, 1.0),
            Timestamp::default() + i16::MIN
        );
        assert_eq!(
            Timestamp::from_seconds(i16::MIN as f64, 1.0),
            Timestamp::default() + i16::MIN
        );
        assert_eq!(
            Timestamp::from_seconds((i16::MIN as f64) - 1.0, 1.0),
            Timestamp::default() + i16::MAX
        );
    }

    #[test]
    fn test_timestamp_as_seconds() {
        assert_eq!(Timestamp::from_seconds(0.0, 1.0).as_seconds(1.0), 0.0);
        assert_eq!(Timestamp::from_seconds(1.0, 1.0).as_seconds(1.0), 1.0);
        assert_eq!(Timestamp::from_seconds(1.0, 1.0).as_seconds(0.25), 0.25);
        assert_eq!(Timestamp::from_seconds(0.25, 0.25).as_seconds(0.25), 0.25);
        assert_eq!(Timestamp::from_seconds(-1.0, 1.0).as_seconds(1.0), -1.0);
        assert_eq!(
            Timestamp::from_seconds(i16::MAX as f64, 1.0).as_seconds(1.0),
            i16::MAX as f64,
        );
        assert_eq!(
            Timestamp::from_seconds((i16::MAX as f64) + 1.0, 1.0).as_seconds(1.0),
            i16::MIN as f64,
        );
        assert_eq!(
            Timestamp::from_seconds(i16::MIN as f64, 1.0).as_seconds(1.0),
            i16::MIN as f64,
        );
        assert_eq!(
            Timestamp::from_seconds((i16::MIN as f64) - 1.0, 1.0).as_seconds(1.0),
            i16::MAX as f64,
        );
    }
}
