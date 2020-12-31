use crate::{
    command::Command,
    timestamp::{EarliestFirst, Timestamp, Timestamped},
};
use std::{collections::BinaryHeap, fmt::Debug};
use turbulence::message_channels::ChannelMessage;

pub trait State: Default + ChannelMessage + Debug + Clone {
    fn from_interpolation(state1: &Self, state2: &Self, t: f32) -> Self;
}

pub trait World: Default + Send + Sync + 'static {
    type CommandType: Command;
    type StateType: State;

    fn apply_command(&mut self, command: &Self::CommandType);
    fn step(&mut self);
    fn set_state(&mut self, target_state: Self::StateType);
    fn state(&self) -> Self::StateType;
}

impl<WorldType: World> Timestamped<WorldType> {
    pub fn apply_stale_commands(
        &mut self,
        command_buffer: &mut BinaryHeap<EarliestFirst<WorldType::CommandType>>,
    ) {
        while let Some(command) = command_buffer.peek() {
            if command.timestamp() > self.timestamp() {
                break;
            }
            self.apply_command(command.inner());
            command_buffer.pop();
        }
    }

    pub fn fast_forward_to_timestamp(
        &mut self,
        timestamp: &Timestamp,
        command_buffer: &mut BinaryHeap<EarliestFirst<WorldType::CommandType>>,
    ) {
        while self.timestamp() < *timestamp {
            self.apply_stale_commands(command_buffer);
            self.step();
        }
    }

    pub fn set_state(&mut self, snapshot: Timestamped<WorldType::StateType>) {
        self.set_timestamp(snapshot.timestamp());
        self.inner_mut().set_state(snapshot.inner().clone());
    }
}
