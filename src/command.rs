//! Traits and structures relating to and for managing commands. Commands are messages sent from
//! outside the physics simulation to alter how the physics simulation runs. For example, causing a
//! rigid body for a player to jump requires a jump command, and causing a player to spawn requires
//! a spawn command.

use crate::timestamp::{Timestamp, Timestamped};
use serde::{de::DeserializeOwned, Serialize};
use std::{cmp::Reverse, collections::BTreeMap, fmt::Debug, ops::Range};
use tracing::warn;

/// A command is a request to change the physics simulation in some way, issued from outside the
/// physics simulation. It is the way in which players and any non-physics game logic can interact
/// with the physics simulation.
///
/// # Example
///
/// ```
/// use crystalorb::command::Command;
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Serialize, Deserialize, Debug, Clone)]
/// struct Player(usize);
///
/// #[derive(Serialize, Deserialize, Debug, Clone)]
/// enum GameCommand {
///     Spawn(Player),
///     Despawn(Player),
///     Jump(Player),
///     Left(Player),
///     Right(Player),
/// }
///
/// impl Command for GameCommand {};
/// ```
pub trait Command: Clone + Sync + Send + 'static + Serialize + DeserializeOwned + Debug {}

/// A handy structure for receiving commands out-of-order, and consuming them in-order. This also
/// unintuitively keeps track of the "current" timestamp for whatever uses this [`CommandBuffer`].
/// This is because the [`CommandBuffer`] needs to maintain an acceptable window of command
/// timestamps centered around the current timestamp, or else the command timestamps would be too
/// far about and [wouldn't be
/// comparable](crate::timestamp::Timestamp::comparable_range_with_midpoint).
#[derive(Clone, Debug)]
pub(crate) struct CommandBuffer<CommandType: Command> {
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
    #[allow(dead_code)]
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
            // Note: since the map is reversed, the latest command is ordered first.
            if let Some((key, value)) = self.map.first_key_value() {
                if key.0 >= acceptable_timestamp_range.end {
                    warn!("Discarding future command {:?} after timestamp update! This should rarely happen", value);
                    let key_to_remove = *key;
                    self.map.remove(&key_to_remove);
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
            .rev() // Our map is in reverse order.
            .flatten()
            .cloned()
            .collect()
    }

    pub fn drain_all(&mut self) -> Vec<CommandType> {
        let commands = self.map.values().rev().flatten().cloned().collect();
        self.map.clear();
        commands
    }

    #[allow(dead_code)]
    pub fn commands_at(&self, timestamp: Timestamp) -> Option<impl Iterator<Item = &CommandType>> {
        self.map
            .get(&Reverse(timestamp))
            .map(|commands| commands.iter())
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = (Timestamp, &Vec<CommandType>)> {
        self.map
            .iter()
            .rev()
            .map(|(timestamp, commands)| (timestamp.0, commands))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl Command for i32 {}

    #[test]
    fn when_timestamp_incremented_then_command_buffer_discards_stale_commands() {
        // GIVEN a command buffer with old commands, one of which is at the lowest extreme of the
        // acceptable range.
        let mut command_buffer = CommandBuffer::<i32>::new();
        let acceptable_range = command_buffer.acceptable_timestamp_range();
        let t = [
            acceptable_range.start,
            acceptable_range.start + 1,
            acceptable_range.end - 2,
            acceptable_range.end - 1,
        ];
        command_buffer.insert(Timestamped::new(0, t[0]));
        command_buffer.insert(Timestamped::new(1, t[1]));
        command_buffer.insert(Timestamped::new(2, t[2]));
        command_buffer.insert(Timestamped::new(3, t[3]));
        assert_eq!(
            *command_buffer.commands_at(t[0]).unwrap().next().unwrap(),
            0
        );
        assert_eq!(
            *command_buffer.commands_at(t[1]).unwrap().next().unwrap(),
            1
        );
        assert_eq!(
            *command_buffer.commands_at(t[2]).unwrap().next().unwrap(),
            2
        );
        assert_eq!(
            *command_buffer.commands_at(t[3]).unwrap().next().unwrap(),
            3
        );

        // WHEN we increment the timestamp.
        command_buffer.update_timestamp(command_buffer.timestamp() + 1);

        // THEN only the one command that now falls outside of the new acceptable range gets
        // discared.
        assert!(command_buffer.commands_at(t[0]).is_none());
        assert_eq!(
            *command_buffer.commands_at(t[1]).unwrap().next().unwrap(),
            1
        );
        assert_eq!(
            *command_buffer.commands_at(t[2]).unwrap().next().unwrap(),
            2
        );
        assert_eq!(
            *command_buffer.commands_at(t[3]).unwrap().next().unwrap(),
            3
        );
    }

    #[test]
    fn when_timestamp_decremented_then_command_buffer_discards_unripe_commands() {
        // GIVEN a command buffer with old commands, one of which is at the highest extreme of the
        // acceptable range.
        let mut command_buffer = CommandBuffer::<i32>::new();
        let acceptable_range = command_buffer.acceptable_timestamp_range();
        let t = [
            acceptable_range.start,
            acceptable_range.start + 1,
            acceptable_range.end - 2,
            acceptable_range.end - 1,
        ];
        command_buffer.insert(Timestamped::new(0, t[0]));
        command_buffer.insert(Timestamped::new(1, t[1]));
        command_buffer.insert(Timestamped::new(2, t[2]));
        command_buffer.insert(Timestamped::new(3, t[3]));
        assert_eq!(
            *command_buffer.commands_at(t[0]).unwrap().next().unwrap(),
            0
        );
        assert_eq!(
            *command_buffer.commands_at(t[1]).unwrap().next().unwrap(),
            1
        );
        assert_eq!(
            *command_buffer.commands_at(t[2]).unwrap().next().unwrap(),
            2
        );
        assert_eq!(
            *command_buffer.commands_at(t[3]).unwrap().next().unwrap(),
            3
        );

        // WHEN we decrement the timestamp.
        command_buffer.update_timestamp(command_buffer.timestamp() - 1);

        // THEN only the one command that now falls outside of the new acceptable range gets
        // discared.
        assert_eq!(
            *command_buffer.commands_at(t[0]).unwrap().next().unwrap(),
            0
        );
        assert_eq!(
            *command_buffer.commands_at(t[1]).unwrap().next().unwrap(),
            1
        );
        assert_eq!(
            *command_buffer.commands_at(t[2]).unwrap().next().unwrap(),
            2
        );
        assert!(command_buffer.commands_at(t[3]).is_none());
    }

    #[test]
    fn when_inserting_command_outside_acceptable_range_then_command_is_discarded() {
        // GIVEN an empty command buffer
        let mut command_buffer = CommandBuffer::<i32>::new();

        // WHEN we insert commands, some of which are outside the acceptable range.
        let acceptable_range = command_buffer.acceptable_timestamp_range();
        let t = [
            acceptable_range.start - 1,
            acceptable_range.start,
            acceptable_range.end - 1,
            acceptable_range.end,
            acceptable_range.end + Timestamp::MAX_COMPARABLE_RANGE / 2,
        ];
        command_buffer.insert(Timestamped::new(0, t[0]));
        command_buffer.insert(Timestamped::new(1, t[1]));
        command_buffer.insert(Timestamped::new(2, t[2]));
        command_buffer.insert(Timestamped::new(3, t[3]));
        command_buffer.insert(Timestamped::new(4, t[4]));
        assert!(command_buffer.commands_at(t[0]).is_none());
        assert_eq!(
            *command_buffer.commands_at(t[1]).unwrap().next().unwrap(),
            1
        );
        assert_eq!(
            *command_buffer.commands_at(t[2]).unwrap().next().unwrap(),
            2
        );
        assert!(command_buffer.commands_at(t[3]).is_none());
        assert!(command_buffer.commands_at(t[4]).is_none());
    }

    #[test]
    fn test_drain_up_to() {
        // GIVEN a command buffer with several commands.
        let mut command_buffer = CommandBuffer::<i32>::new();
        command_buffer.insert(Timestamped::new(0, command_buffer.timestamp() + 1));
        command_buffer.insert(Timestamped::new(1, command_buffer.timestamp() + 5));
        command_buffer.insert(Timestamped::new(2, command_buffer.timestamp() + 2));
        command_buffer.insert(Timestamped::new(3, command_buffer.timestamp() + 4));
        command_buffer.insert(Timestamped::new(4, command_buffer.timestamp() + 8));
        command_buffer.insert(Timestamped::new(5, command_buffer.timestamp() + 6));
        command_buffer.insert(Timestamped::new(6, command_buffer.timestamp() + 7));
        command_buffer.insert(Timestamped::new(7, command_buffer.timestamp() + 3));

        // WHEN we drain the command buffer up to a certain timestamp.
        let drained_commands = command_buffer.drain_up_to(command_buffer.timestamp() + 4);

        // THEN we get the commands up to and including the specified timestamp, in order.
        assert_eq!(drained_commands.len(), 4);
        assert_eq!(drained_commands[0], 0);
        assert_eq!(drained_commands[1], 2);
        assert_eq!(drained_commands[2], 7);
        assert_eq!(drained_commands[3], 3);

        // THEN we the command buffer will have discarded commands up to and including the
        // specified timestamp, while keeping the other commands.
        assert!(command_buffer
            .commands_at(command_buffer.timestamp() + 1)
            .is_none());
        assert!(command_buffer
            .commands_at(command_buffer.timestamp() + 2)
            .is_none());
        assert!(command_buffer
            .commands_at(command_buffer.timestamp() + 3)
            .is_none());
        assert!(command_buffer
            .commands_at(command_buffer.timestamp() + 4)
            .is_none());
        assert_eq!(
            *command_buffer
                .commands_at(command_buffer.timestamp() + 5)
                .unwrap()
                .next()
                .unwrap(),
            1
        );
        assert_eq!(
            *command_buffer
                .commands_at(command_buffer.timestamp() + 6)
                .unwrap()
                .next()
                .unwrap(),
            5
        );
        assert_eq!(
            *command_buffer
                .commands_at(command_buffer.timestamp() + 7)
                .unwrap()
                .next()
                .unwrap(),
            6
        );
        assert_eq!(
            *command_buffer
                .commands_at(command_buffer.timestamp() + 8)
                .unwrap()
                .next()
                .unwrap(),
            4
        );
    }
}
