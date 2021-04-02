use crate::timestamp::{Timestamp, Timestamped};
use serde::{de::DeserializeOwned, Serialize};
use std::{cmp::Reverse, collections::BTreeMap, fmt::Debug};

pub trait Command: Clone + Sync + Send + 'static + Serialize + DeserializeOwned + Debug {}

#[derive(Clone)]
pub struct CommandBuffer<CommandType: Command> {
    map: BTreeMap<Reverse<Timestamp>, Vec<CommandType>>,
}

impl<CommandType: Command> Default for CommandBuffer<CommandType> {
    fn default() -> Self {
        Self {
            map: Default::default(),
        }
    }
}

impl<CommandType: Command> CommandBuffer<CommandType> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, command: Timestamped<CommandType>) {
        if let Some(commands_at_timestamp) = self.map.get_mut(&Reverse(command.timestamp())) {
            commands_at_timestamp.push(command.inner().clone());
        } else {
            self.map
                .insert(Reverse(command.timestamp()), vec![command.inner().clone()]);
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
