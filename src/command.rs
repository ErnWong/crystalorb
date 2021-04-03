use crate::timestamp::{Timestamp, Timestamped};
use bevy::prelude::*;
use serde::{de::DeserializeOwned, Serialize};
use std::{cmp::Reverse, collections::BTreeMap, fmt::Debug, ops::Range};

pub trait Command: Clone + Sync + Send + 'static + Serialize + DeserializeOwned + Debug {}

#[derive(Clone)]
pub struct CommandBuffer<CommandType: Command> {
    map: BTreeMap<Reverse<Timestamp>, Vec<CommandType>>,
    timestamp: Timestamp,
}

impl<CommandType: Command> Default for CommandBuffer<CommandType> {
    fn default() -> Self {
        Self {
            map: Default::default(),
            timestamp: Default::default(),
        }
    }
}

impl<CommandType: Command> CommandBuffer<CommandType> {
    pub fn new() -> Self {
        Self::default()
    }

    /// The command buffer attempts to sort the commands by their timestamp, but the timestamps
    /// uses a wrapped difference to calculate the ordering. In order to prevent stale commands
    /// from being replayed, and in order to keep the commands transitively consistently
    /// ordered, we restrict the command buffer to only half of the available timestamp
    /// datasize.
    pub fn acceptable_timestamp_range(&self) -> Range<Timestamp> {
        Timestamp::comparable_range_with_midpoint(self.timestamp)
    }

    pub fn timestamp(&self) -> Timestamp {
        self.timestamp
    }

    pub fn update_timestamp(&mut self, timestamp: Timestamp) {
        self.timestamp = timestamp;

        // Now we discard commands that fall outside of the acceptable timestamp range.
        // (1) future commands are still accepted into the command buffer without breaking the
        // transitivity laws.
        // (2) stale commands are discarded so that they don't wrap around and replayed again like
        // a ghost.

        let acceptable_timestamp_range = self.acceptable_timestamp_range();

        // Discard stale commands.
        let stale_commands = self
            .map
            .split_off(&Reverse(acceptable_timestamp_range.start - 1));
        if !stale_commands.is_empty() {
            warn!(
                "Discarded {:?} stale commands due to timestamp update! This should rarely happen. The commands were {:?}",
                stale_commands.len(),
                stale_commands
            );
        }

        // In case we rewind the midpoint timestamp, discard commands too far in the future.
        loop {
            if let Some((key, value)) = self.map.last_key_value() {
                if key.0 >= acceptable_timestamp_range.end {
                    warn!("Discarding future command {:?} after timestamp update! This should rarely happen", value);
                    self.map.remove(&key.clone());
                    continue;
                }
            }
            break;
        }
    }

    pub fn insert(&mut self, command: Timestamped<CommandType>) {
        if self
            .acceptable_timestamp_range()
            .contains(&command.timestamp())
        {
            if let Some(commands_at_timestamp) = self.map.get_mut(&Reverse(command.timestamp())) {
                commands_at_timestamp.push(command.inner().clone());
            } else {
                self.map
                    .insert(Reverse(command.timestamp()), vec![command.inner().clone()]);
            }
        } else {
            warn!(
                "Tried to insert a command of timestamp {:?} outside of the acceptable range {:?}. The command {:?} is dropped",
                command.timestamp(),
                self.acceptable_timestamp_range(),
                command
            );
        }
    }

    pub fn drain_up_to(&mut self, newest_timestamp_to_drain: Timestamp) -> Vec<CommandType> {
        self.map
            .split_off(&Reverse(newest_timestamp_to_drain))
            .values()
            .flatten()
            .map(|command| command.clone())
            .collect()
    }

    pub fn commands_at(&self, timestamp: Timestamp) -> Option<impl Iterator<Item = &CommandType>> {
        if let Some(commands) = self.map.get(&Reverse(timestamp)) {
            Some(commands.iter())
        } else {
            None
        }
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }
}
