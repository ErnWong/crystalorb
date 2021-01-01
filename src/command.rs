use crate::timestamp::{Timestamp, Timestamped};
use serde::{de::DeserializeOwned, Serialize};
use std::{cmp::Reverse, collections::BTreeMap, fmt::Debug};

pub trait Command: Clone + Sync + Send + 'static + Serialize + DeserializeOwned + Debug {}

pub struct CommandBuffer<CommandType: Command> {
    map: BTreeMap<Reverse<Timestamp>, Vec<CommandType>>,
}

impl<CommandType: Command> CommandBuffer<CommandType> {
    pub fn new() -> Self {
        Self {
            map: Default::default(),
        }
    }

    pub fn insert(&mut self, command: Timestamped<CommandType>) {
        if let Some(commands_at_timestamp) = self.map.get_mut(&Reverse(command.timestamp())) {
            commands_at_timestamp.push(command.inner().clone());
        } else {
            self.map
                .insert(Reverse(command.timestamp()), vec![command.inner().clone()]);
        }
    }

    pub fn discard_old(&mut self, oldest_timestamp_to_keep: Timestamp) {
        self.map.split_off(&Reverse(oldest_timestamp_to_keep - 1));
    }

    pub fn commands_at(&self, timestamp: Timestamp) -> Option<impl Iterator<Item = &CommandType>> {
        if let Some(commands) = self.map.get(&Reverse(timestamp)) {
            Some(commands.iter())
        } else {
            None
        }
    }
}
