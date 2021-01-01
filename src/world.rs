use crate::{
    command::{Command, CommandBuffer},
    fixed_timestepper::Stepper,
    timestamp::{Timestamp, Timestamped},
};
use std::fmt::Debug;
use turbulence::message_channels::ChannelMessage;

pub trait State: Default + ChannelMessage + Debug + Clone {
    fn from_interpolation(state1: &Self, state2: &Self, t: f32) -> Self;
}

pub trait World: Stepper + Default + Send + Sync + 'static {
    type CommandType: Command;
    type StateType: State;

    fn command_is_valid(command: &Self::CommandType, client_id: usize) -> bool;
    fn apply_command(&mut self, command: &Self::CommandType);
    fn set_state(&mut self, target_state: Self::StateType);
    fn state(&self) -> Self::StateType;
}

impl<WorldType: World> Timestamped<WorldType> {
    pub fn apply_commands(&mut self, command_buffer: &CommandBuffer<WorldType::CommandType>) {
        if let Some(commands) = command_buffer.commands_at(self.timestamp()) {
            for command in commands {
                self.apply_command(command);
            }
        }
    }

    pub fn fast_forward_to_timestamp(
        &mut self,
        timestamp: &Timestamp,
        command_buffer: &CommandBuffer<WorldType::CommandType>,
    ) {
        while self.timestamp() < *timestamp {
            self.apply_commands(command_buffer);
            self.step();
        }
    }

    pub fn set_state(&mut self, snapshot: Timestamped<WorldType::StateType>) {
        self.set_timestamp(snapshot.timestamp());
        self.inner_mut().set_state(snapshot.inner().clone());
    }

    pub fn state(&self) -> Timestamped<WorldType::StateType> {
        Timestamped::new(self.inner().state(), self.timestamp())
    }
}
