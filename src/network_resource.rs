use serde::{de::DeserializeOwned, Serialize};
use std::{error::Error, fmt::Debug};

pub type ConnectionHandleType = usize;

pub trait NetworkResource {
    type ConnectionType<'a>: Connection;
    fn broadcast_message<MessageType>(&mut self, message: MessageType)
    where
        MessageType: Debug + Clone + Serialize + DeserializeOwned + Send + Sync + 'static;
    fn send_message<MessageType>(
        &mut self,
        handle: ConnectionHandleType,
        message: MessageType,
    ) -> Result<Option<MessageType>, Box<dyn Error + Send>>
    where
        MessageType: Debug + Clone + Serialize + DeserializeOwned + Send + Sync + 'static;
    fn connections<'a>(
        &'a mut self,
    ) -> Box<dyn Iterator<Item = (ConnectionHandleType, Self::ConnectionType<'a>)> + 'a>;
}

pub trait Connection {
    fn recv<MessageType>(&mut self) -> Option<MessageType>
    where
        MessageType: Debug + Clone + Serialize + DeserializeOwned + Send + Sync + 'static;
    fn send<MessageType>(&mut self, message: MessageType) -> Option<MessageType>
    where
        MessageType: Debug + Clone + Serialize + DeserializeOwned + Send + Sync + 'static;
    fn flush<MessageType>(&mut self)
    where
        MessageType: Debug + Clone + Serialize + DeserializeOwned + Send + Sync + 'static;
}
