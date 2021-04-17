pub trait Stepper {
    fn step(&mut self);
}

pub trait FixedTimestepper: Stepper {
    fn advance(&mut self, delta_seconds: f32);
}

pub enum TerminationCondition {
    LastUndershoot,
    FirstOvershoot,
}

pub fn advance<T>(
    stepper: &mut T,
    delta_seconds: f32,
    initial_overshoot_seconds: f32,
    timestep: f32,
    termination_condition: TerminationCondition,
) -> f32
where
    T: FixedTimestepper,
{
    let mut overshoot_seconds = initial_overshoot_seconds - delta_seconds;
    loop {
        let next_overshoot_seconds = overshoot_seconds + timestep;
        let termination_compare_value = match &termination_condition {
            TerminationCondition::LastUndershoot => next_overshoot_seconds,
            TerminationCondition::FirstOvershoot => overshoot_seconds,
        };
        if termination_compare_value >= 0.0 {
            break;
        }
        stepper.step();
        overshoot_seconds = next_overshoot_seconds;
    }
    overshoot_seconds
}

pub fn advance_without_overshoot<T>(
    stepper: &mut T,
    delta_seconds: f32,
    initial_overshoot_seconds: f32,
    timestep: f32,
) -> f32
where
    T: FixedTimestepper,
{
    advance(
        stepper,
        delta_seconds,
        initial_overshoot_seconds,
        timestep,
        TerminationCondition::LastUndershoot,
    )
}

pub fn advance_with_overshoot<T>(
    stepper: &mut T,
    delta_seconds: f32,
    initial_overshoot_seconds: f32,
    timestep: f32,
) -> f32
where
    T: FixedTimestepper,
{
    advance(
        stepper,
        delta_seconds,
        initial_overshoot_seconds,
        timestep,
        TerminationCondition::FirstOvershoot,
    )
}
