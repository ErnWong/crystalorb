use crate::{
    command::{Command, CommandBuffer},
    fixed_timestepper::Stepper,
    timestamp::{Timestamp, Timestamped},
};
use std::fmt::Debug;
use turbulence::message_channels::ChannelMessage;

pub trait DisplayState: Default + Send + Sync + Clone {
    fn from_interpolation(state1: &Self, state2: &Self, t: f32) -> Self;
}

pub trait World: Stepper + Default + Send + Sync + 'static {
    type CommandType: Command;
    type SnapshotType: ChannelMessage + Clone + Debug;
    type DisplayStateType: DisplayState;

    fn command_is_valid(command: &Self::CommandType, client_id: usize) -> bool;
    fn apply_command(&mut self, command: &Self::CommandType);
    fn apply_snapshot(&mut self, snapshot: Self::SnapshotType);
    fn snapshot(&self) -> Self::SnapshotType;
    fn display_state(&self) -> Self::DisplayStateType;
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
        max_steps: usize,
    ) {
        for _ in 0..max_steps {
            if self.timestamp() >= *timestamp {
                break;
            }
            self.apply_commands(command_buffer);
            self.step();
        }
    }

    pub fn apply_snapshot(&mut self, snapshot: Timestamped<WorldType::SnapshotType>) {
        self.set_timestamp(snapshot.timestamp());
        self.inner_mut().apply_snapshot(snapshot.inner().clone());
    }

    pub fn snapshot(&self) -> Timestamped<WorldType::SnapshotType> {
        Timestamped::new(self.inner().snapshot(), self.timestamp())
    }
}
