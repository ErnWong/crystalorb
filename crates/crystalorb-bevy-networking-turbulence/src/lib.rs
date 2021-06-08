#![feature(generic_associated_types)]

use bevy_networking_turbulence::{ConnectionHandle, NetworkResource};
use crystalorb::network_resource::{
    Connection as ConnectionTrait, ConnectionHandleType, NetworkResource as NetworkResourceTrait,
};
use serde::{de::DeserializeOwned, Serialize};
use std::fmt::Debug;
use turbulence::MessageChannels;

pub struct WrappedNetworkResource<'a>(pub &'a mut NetworkResource);
pub struct WrappedConnection<'a>(pub &'a mut MessageChannels);

impl<'a> NetworkResourceTrait for WrappedNetworkResource<'a> {
    type ConnectionType<'b> = WrappedConnection<'b>;

    fn get_connection(&mut self, handle: ConnectionHandleType) -> Option<Self::ConnectionType<'_>> {
        self.0
            .connections
            .get_mut(&(handle as ConnectionHandle))
            .map(|connection| WrappedConnection(connection.channels().unwrap()))
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
}

impl<'a> ConnectionTrait for WrappedConnection<'a> {
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
