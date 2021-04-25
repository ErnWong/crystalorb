use crate::fixed_timestepper::Stepper;
use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    fmt::Debug,
    num::Wrapping,
    ops::{Add, Deref, DerefMut, Range, Sub},
};

#[derive(Eq, PartialEq, Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct Timestamp(Wrapping<i16>);

impl Timestamp {
    /// See note about transitivity for Timestamp's Ord implementation.
    pub const MAX_COMPARABLE_RANGE: i16 = i16::MAX;

    pub fn from_seconds(seconds: f64, timestep_seconds: f64) -> Self {
        Self::from(FloatTimestamp::from_seconds(seconds, timestep_seconds))
    }

    pub fn increment(&mut self) {
        self.0 += Wrapping(1);
    }

    pub fn as_seconds(&self, timestep_seconds: f64) -> f64 {
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
    /// To guarantee transitivity (for example, to use in BTreeMaps), ensure that all values being
    /// compared against each other are at most std::i16::MAX length of each other.
    /// (Maybe std::i16::MAX is off by one, but it is at least on the conservative side)
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

#[derive(PartialEq, Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct FloatTimestamp(f64);

impl FloatTimestamp {
    pub fn from_seconds(seconds: f64, timestep_seconds: f64) -> Self {
        Self::from_unwrapped(seconds / timestep_seconds)
    }

    pub fn from_unwrapped(frames: f64) -> Self {
        let frames_wrapped = (frames + 15.0f64.exp2()).rem_euclid(16.0f64.exp2()) - 15.0f64.exp2();
        Self(frames_wrapped)
    }

    pub fn as_seconds(&self, timestep_seconds: f64) -> f64 {
        self.0 * timestep_seconds
    }

    pub fn ceil(&self) -> Timestamp {
        Timestamp(Wrapping(self.0.ceil() as i16))
    }

    pub fn floor(&self) -> Timestamp {
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

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Timestamped<T> {
    inner: T,
    timestamp: Timestamp,
}

impl<T> Timestamped<T> {
    pub fn new(inner: T, timestamp: Timestamp) -> Self {
        Self { inner, timestamp }
    }

    pub fn inner(&self) -> &T {
        &self.inner
    }

    pub fn inner_mut(&mut self) -> &mut T {
        &mut self.inner
    }

    pub fn timestamp(&self) -> Timestamp {
        self.timestamp
    }

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
