use bevy_networking_turbulence::{Connection, ConnectionHandle, NetworkResource};
use crystalorb::{
    clocksync::ClockSyncMessage,
    network_resource::{Connection, ConnectionHandleType, NetworkResource},
    timestamp::Timestamped,
    world::World,
};
use serde::{de::DeserializeOwned, Serialize};
use std::{convert::TryInto, error::Error, fmt::Debug};
use turbulence::MessageChannels;

pub struct WrappedNetworkResource<'a>(pub &'a mut NetworkResource);
pub struct WrappedConnection<'a>(pub &'a mut MessageChannels);

impl<'a> NetworkResource for WrappedNetworkResource<'a> {
    type ConnectionType<'b> = WrappedConnection<'b>;

    fn broadcast_message<MessageType>(&mut self, message: MessageType)
    where
        MessageType: Debug + Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
    {
        self.0.broadcast_message(message);
    }

    fn send_message<MessageType>(
        &mut self,
        handle: usize,
        message: MessageType,
    ) -> Result<Option<MessageType>, Box<dyn Error + Send>>
    where
        MessageType: Debug + Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
    {
        self.0.send_message(handle.try_into().unwrap(), message)
    }

    fn connections<'c>(
        &'c mut self,
    ) -> Box<dyn Iterator<Item = (usize, WrappedConnection<'c>)> + 'c> {
        Box::new(self.0.connections.iter_mut().map(|(handle, connection)| {
            (
                *handle as usize,
                WrappedConnection(connection.channels().unwrap()),
            )
        }))
    }

    fn get_connection(&mut self, handle: ConnectionHandleType) -> Option<Self::ConnectionType<'_>> {
        WrappedConnection(self.0.connections.get_mut(handle as ConnectionHandle))
    }
}

impl<'a> Connection for WrappedConnection<'a> {
    fn recv<MessageType>(&mut self) -> Option<MessageType>
    where
        MessageType: Debug + Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
    {
        self.0.recv()
    }

    fn send<MessageType>(&mut self, message: MessageType) -> Option<MessageType>
    where
        MessageType: Debug + Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
    {
        self.0.send(message)
    }

    fn flush<MessageType>(&mut self)
    where
        MessageType: Debug + Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
    {
        self.0.flush::<MessageType>()
    }
}
