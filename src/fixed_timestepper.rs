pub trait Stepper {
    fn step(&mut self) -> f32;
}

pub trait FixedTimestepper: Stepper {
    fn advance(&mut self, delta_seconds: f32);
}

pub fn advance<T>(stepper: &mut T, delta_seconds: f32, initial_overshoot_seconds: f32) -> f32
where
    T: FixedTimestepper,
{
    let mut overshoot_seconds = initial_overshoot_seconds - delta_seconds;
    while overshoot_seconds < 0.0 {
        overshoot_seconds += stepper.step();
    }
    overshoot_seconds
}
