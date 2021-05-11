use crate::timestamp::Timestamped;
use std::{fmt::Debug, ops::Deref};

/// The [`DisplayState`] represents the information about how to display the [`World`][world] at
/// its current state. For example, while a [`World`][world] might contain information about
/// player's position and velocities, some games may only need to know about the position to render
/// it (unless you're doing some fancy motion-blur). You can think of a [`DisplayState`] as the
/// "output" of a [`World`][world]. There is nothing stopping you from making the [`DisplayState`]
/// the same structure as the [`World`][world] if it makes more sense for your game, but most of
/// the time, the [`World`][world] structure may contain things that are inefficient to copy around
/// (e.g. an entire physics engine)
///
/// [world]: [crate::world::World]
pub trait DisplayState: Send + Sync + Clone + Debug {
    /// CrystalOrb needs to mix different [`DisplayState`]s from different [`World`][world]s
    /// together, as well as mix [`DisplayState`] from two adjacent timestamps. The
    /// [`from_interpolation`](DisplayState::from_interpolation) method tells CrystalOrb how to
    /// perform this "mix" operation. Here, the `t` parameter is the interpolation parameter that
    /// ranges between `0.0` and `1.0`, where `t = 0.0` represents the request to have 100% of
    /// `state1` and 0% of `state2`, and where `t = 1.0` represents the request to have 0% of
    /// `state1` and 100% of `state2`.
    ///
    /// A common operation to implement this function is through [linear
    /// interpolation](https://en.wikipedia.org/wiki/Linear_interpolation), which looks like this:
    ///
    /// ```text
    /// state1 * (1.0 - t) + state2 * t
    /// ```
    ///
    /// However, for things involving rotation, you may need to use [spherical linear
    /// interpolation](https://en.wikipedia.org/wiki/Slerp), or [circular
    /// statistics](https://en.wikipedia.org/wiki/Mean_of_circular_quantities), and perhaps you may
    /// need to convert between coordinate systems before/after performing the interpolation to get
    /// the right transformations about the correct pivot points.
    ///
    /// [world]: [crate::world::World]
    fn from_interpolation(state1: &Self, state2: &Self, t: f64) -> Self;
}

impl<T: DisplayState> DisplayState for Timestamped<T> {
    /// Interpolate between two timestamped display states. If the two timestamps are
    /// different, then the interpolation parameter `t` must be either `0.0` or `1.0`.
    ///
    /// # Panics
    ///
    /// Panics if the timestamps are different but the interpolation parameter is not `0.0` nor
    /// `1.0`, since timestamps are whole number values and cannot be continuously
    /// interpolated. Interpolating between two display states of different timestamps is known
    /// as "tweening" (i.e. animation in-betweening) and should be done using
    /// [`Tweened::from_interpolation`].
    fn from_interpolation(state1: &Self, state2: &Self, t: f64) -> Self {
        if t == 0.0 {
            state1.clone()
        } else if (t - 1.0).abs() < f64::EPSILON {
            state2.clone()
        } else {
            assert_eq!(state1.timestamp(), state2.timestamp(), "Can only interpolate between timestamped states of the same timestamp. If timestamps differ, you will need to use Tweened::from_interpolation to also interpolate the timestamp value into a float.");

            Self::new(
                DisplayState::from_interpolation(state1.inner(), state2.inner(), t),
                state1.timestamp(),
            )
        }
    }
}

/// This is the result when you interpolate/"blend"/"tween" between two [`DisplayState`]s of
/// adjacent timestamps (similar to ["Inbetweening"](https://en.wikipedia.org/wiki/Inbetweening) in
/// animation - the generation of intermediate frames). You get the [`DisplayState`] and a
/// floating-point, non-whole-number timestamp.
#[derive(Default, Debug, Clone, PartialEq)]
pub struct Tweened<T> {
    display_state: T,
    timestamp: f64,
}

impl<T: DisplayState> Tweened<T> {
    /// Get the resulting in-between [`DisplayState`].
    pub fn display_state(&self) -> &T {
        &self.display_state
    }

    /// Get the "logical timestamp" that [`Tweened::display_state`] corresponds with. For
    /// example, a `float_timestamp` of `123.4` represents the in-between frame that is 40% of
    /// the way between frame `123` and frame `124`.
    pub fn float_timestamp(&self) -> f64 {
        self.timestamp
    }
}

impl<T: DisplayState> Deref for Tweened<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.display_state
    }
}

impl<T: DisplayState> Tweened<T> {
    /// Interpolate between two timestamped dispay states to find the in-between display state.
    pub fn from_interpolation(state1: &Timestamped<T>, state2: &Timestamped<T>, t: f64) -> Self {
        // Note: timestamps are in modulo arithmetic, so we need to work using the wrapped
        // difference value.
        let timestamp_difference: i16 = (state2.timestamp() - state1.timestamp()).into();
        let timestamp_offset: f64 = t * (timestamp_difference as f64);
        let timestamp_interpolated = i16::from(state1.timestamp()) as f64 + timestamp_offset;
        Self {
            display_state: T::from_interpolation(state1.inner(), state2.inner(), t),
            timestamp: timestamp_interpolated,
        }
    }
}

impl<T: DisplayState> From<Timestamped<T>> for Tweened<T> {
    fn from(timestamped: Timestamped<T>) -> Self {
        Self {
            display_state: timestamped.inner().clone(),
            timestamp: i16::from(timestamped.timestamp()) as f64,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::timestamp::Timestamp;

    #[derive(Clone, Default, Debug, PartialEq)]
    struct MockDisplayState(f64);
    impl DisplayState for MockDisplayState {
        fn from_interpolation(state1: &Self, state2: &Self, t: f64) -> Self {
            Self(state1.0 * t + state2.0 * (1.0 - t))
        }
    }

    #[test]
    fn when_interpolating_displaystate_with_t_0_then_state1_is_returned() {
        // GIVEN
        let state1 = Timestamped::new(MockDisplayState(4.0), Timestamp::default() + 2);
        let state2 = Timestamped::new(MockDisplayState(8.0), Timestamp::default() + 5);

        // WHEN
        let interpolated = DisplayState::from_interpolation(&state1, &state2, 0.0);

        // THEN
        assert_eq!(state1, interpolated);
    }

    #[test]
    fn when_interpolating_displaystate_with_t_1_then_state2_is_returned() {
        // GIVEN
        let state1 = Timestamped::new(MockDisplayState(4.0), Timestamp::default() + 2);
        let state2 = Timestamped::new(MockDisplayState(8.0), Timestamp::default() + 5);

        // WHEN
        let interpolated = DisplayState::from_interpolation(&state1, &state2, 1.0);

        // THEN
        assert_eq!(state2, interpolated);
    }
}
