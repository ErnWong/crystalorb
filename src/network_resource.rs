//! The set of traits that need to be implemented to tell CrystalOrb how to use your external
//! networking library to send and receive messages.

use serde::{de::DeserializeOwned, Serialize};
use std::{error::Error, fmt::Debug};

use crate::{clocksync::ClockSyncMessage, timestamp::Timestamped, world::World};

/// Used for identifying each connection. Note that this is how the
/// [`Server`](crate::server::Server) determines the
/// [`client_id`](crate::client::stage::Ready::client_id) for each client, so they should be unique
/// among all clients that is or has once connected during the server's uptime.
pub type ConnectionHandleType = usize;

/// CrystalOrb needs an external networking library before it can be used. Such networking library
/// will be responsible for sending three kinds of messages:
///
/// 1. [`ClockSyncMessage`](crate::clocksync::ClockSyncMessage)
/// 2. `Timestamped<SnapshotType>` (see [`Timestamped`](crate::timestamp::Timestamped) and
///    [`SnapshotType`](crate::world::World::SnapshotType)).
/// 3. `Timestamped<CommandType>` (see [`Timestamped`](crate::timestamp::Timestamped) and
///    [`CommandType`](crate::world::World::CommandType)).
///
/// In theory, the external networking library would not be responsible for understanding these
/// messages. It would only be responsible for sending and receiving them.
///
/// How these message channels get multiplexed is up to you / the external networking library.
/// Whether or not these message channels are reliable or unreliable, ordered or unordered, is also
/// up to you / the external networking library. CrystalOrb is written assuming that
/// `ClockSyncMessage` and `SnapshotType` are unreliable and unordered, while `CommandType` is
/// reliable but unordered.
///
/// This interface is based off on the interface provided by the
/// [`bevy_networking_turbulence`](https://github.com/smokku/bevy_networking_turbulence) plugin. See
/// [`crystalorb-bevy-networking-turbulence`](https://github.com/ErnWong/crystalorb/tree/crates/crystalorb-bevy-networking-turbulence)
/// for an example for integrating with `bevy_networking_turbulence`.
pub trait NetworkResource<WorldType: World> {
    /// The [`Connection`] structure that CrystalOrb will use to send/receive messages from a
    /// specific remote machine. This may probably be a wrapper to a mutable reference to some
    /// connection type that is used by your external networking library of choice, in which
    /// case a generic lifetime parameter is provided here that you can use.
    type ConnectionType<'a>: Connection<WorldType>
    where
        Self: 'a,
        WorldType: 'a;

    /// Iterate through the available connections. For servers, this would be the list of current
    /// client connections that are still alive. For the client, this would only contain the
    /// connection to the server once the connection has been established.
    fn connections<'a>(
        &'a mut self,
    ) -> Box<dyn Iterator<Item = (ConnectionHandleType, Self::ConnectionType<'a>)> + 'a>;

    /// Get a specific connection given its connection handle.
    fn get_connection(&mut self, handle: ConnectionHandleType) -> Option<Self::ConnectionType<'_>>;

    /// Optional: Send the given message to all active connections. A default implementation is
    /// already given that uses [`NetworkResource::connections`] and [`Connection::send`].
    ///
    /// CrystalOrb will invoke this method with the three message types as specified in
    /// [`NetworkResource`].
    fn broadcast_message<MessageType>(&mut self, message: MessageType)
    where
        MessageType: Debug + Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
    {
        // Adapted from bevy_networking_turbulence.
        for (_, mut connection) in self.connections() {
            connection.send(message.clone());
            connection.flush::<MessageType>();
        }
    }

    /// Optional: Send the given message to the given connection. A default implementation is
    /// already given that uses [`NetworkResource::get_connection`] and [`Connection::send`].
    ///
    /// CrystalOrb will invoke this method with the three message types as specified in
    /// [`NetworkResource`].
    ///
    /// # Errors
    ///
    /// Returns [`NotFound`](std::io::ErrorKind::NotFound) [`std::io::Error`] if a connection with
    /// the given `handle` could not be found.
    fn send_message<MessageType>(
        &mut self,
        handle: ConnectionHandleType,
        message: MessageType,
    ) -> Result<Option<MessageType>, Box<dyn Error + Send>>
    where
        MessageType: Debug + Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
    {
        // Adapted from bevy_networking_turbulence.
        match self.get_connection(handle) {
            Some(mut connection) => {
                let unsent = connection.send(message);
                connection.flush::<MessageType>();
                Ok(unsent)
            }
            None => Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "No such connection",
            ))),
        }
    }
}

/// Representation of a connection to a specific remote machine that allows CrystalOrb to send and
/// receive messages.
pub trait Connection<WorldType: World> {
    /// Interface for CrystalOrb to receive the next command message.
    fn recv_command(&mut self) -> Option<Timestamped<WorldType::CommandType>>;
    /// Interface for CrystalOrb to receive the next snapshot message.
    fn recv_snapshot(&mut self) -> Option<Timestamped<WorldType::SnapshotType>>;
    /// Interface for CrystalOrb to receive the next clock sync message.
    fn recv_clock_sync(&mut self) -> Option<ClockSyncMessage>;

    /// Interface for CrystalOrb to try sending a message to the connection's destination. If
    /// unsuccessful, the message should be returned. Otherwise, `None` should be returned.
    ///
    /// CrystalOrb will invoke this method with the three message types as specified in
    /// [`NetworkResource`].
    fn send<MessageType>(&mut self, message: MessageType) -> Option<MessageType>
    where
        MessageType: Debug + Clone + Serialize + DeserializeOwned + Send + Sync + 'static;

    /// Interface that CrystalOrb would use to ensure that any pending messages are sent.
    ///
    /// CrystalOrb will invoke this method with the three message types as specified in
    /// [`NetworkResource`].
    fn flush<MessageType>(&mut self)
    where
        MessageType: Debug + Clone + Serialize + DeserializeOwned + Send + Sync + 'static;
}
