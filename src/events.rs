use bevy_networking_turbulence::NetworkEvent;
use std::convert::TryFrom;

#[derive(Debug)]
pub enum ClientConnectionEvent {
    Connected(usize),
    Disconnected(usize),
}

impl TryFrom<&NetworkEvent> for ClientConnectionEvent {
    type Error = &'static str;
    fn try_from(network_event: &NetworkEvent) -> Result<Self, Self::Error> {
        match network_event {
            NetworkEvent::Connected(handle) => {
                Ok(ClientConnectionEvent::Connected(*handle as usize))
            }
            NetworkEvent::Disconnected(handle) => {
                Ok(ClientConnectionEvent::Disconnected(*handle as usize))
            }
            NetworkEvent::Packet(..) => Err("Packet is not a client connection event"),
        }
    }
}

impl ClientConnectionEvent {
    pub fn client_id(&self) -> usize {
        match self {
            ClientConnectionEvent::Connected(client_id) => *client_id,
            ClientConnectionEvent::Disconnected(client_id) => *client_id,
        }
    }
}
