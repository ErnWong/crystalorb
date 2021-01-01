use crate::fixed_timestepper::Stepper;
use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    fmt::Debug,
    num::Wrapping,
    ops::{Add, Deref, DerefMut, Sub},
};

#[derive(Eq, PartialEq, Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct Timestamp(Wrapping<i16>);

impl Timestamp {
    pub fn from_seconds(seconds: f64, timestep_seconds: f32) -> Self {
        let frames_f64 = seconds / timestep_seconds as f64;
        let frames_wrapped = ((frames_f64 + 2.0f64.powi(15)) % 2.0f64.powi(16)) - 2.0f64.powi(15);
        Self(Wrapping(frames_wrapped as i16))
    }

    pub fn increment(&mut self) {
        self.0 += Wrapping(1);
    }

    pub fn as_seconds(&self, timestep_seconds: f32) -> f32 {
        self.0 .0 as f32 * timestep_seconds
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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
    fn step(&mut self) -> f32 {
        let delta_seconds = self.inner.step();
        self.timestamp.increment();
        delta_seconds
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

pub struct EarliestPrioritized<T>(Timestamped<T>);

impl<T> From<Timestamped<T>> for EarliestPrioritized<T> {
    fn from(timestamp: Timestamped<T>) -> Self {
        Self(timestamp)
    }
}

impl<T> Deref for EarliestPrioritized<T> {
    type Target = Timestamped<T>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for EarliestPrioritized<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> Ord for EarliestPrioritized<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.timestamp().cmp(&other.timestamp()).reverse()
    }
}

impl<T> PartialOrd for EarliestPrioritized<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> PartialEq for EarliestPrioritized<T> {
    fn eq(&self, other: &Self) -> bool {
        self.timestamp() == other.timestamp()
    }
}

impl<T> Eq for EarliestPrioritized<T> {}
