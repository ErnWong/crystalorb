#![feature(generic_associated_types)]
#![feature(extended_key_value_attributes)]
#![doc = include_str!("../README.markdown")]

use crystalorb::{
    clocksync::ClockSyncMessage,
    network_resource::{Connection, ConnectionHandleType, NetworkResource},
    timestamp::Timestamped,
    world::World,
};
use serde::{de::DeserializeOwned, Serialize};
use std::{
    any::{type_name, Any, TypeId},
    cell::{Cell, RefCell},
    collections::{HashMap, VecDeque},
    fmt::Debug,
    rc::Rc,
};

#[derive(Default)]
pub struct MockNetwork {
    pub connections: HashMap<ConnectionHandleType, MockConnection>,
}

impl MockNetwork {
    pub fn new_mock_network<WorldType: World>() -> (MockNetwork, (MockNetwork, MockNetwork)) {
        let mut client_1_net = MockNetwork::default();
        let mut client_2_net = MockNetwork::default();
        let mut server_net = MockNetwork::default();

        let (mut client_1_connection, mut server_1_connection) = MockConnection::new_pair();
        let (mut client_2_connection, mut server_2_connection) = MockConnection::new_pair();

        MockConnection::register_channel::<ClockSyncMessage>(
            &mut client_1_connection,
            &mut server_1_connection,
        );
        MockConnection::register_channel::<ClockSyncMessage>(
            &mut client_2_connection,
            &mut server_2_connection,
        );

        MockConnection::register_channel::<Timestamped<WorldType::SnapshotType>>(
            &mut client_1_connection,
            &mut server_1_connection,
        );
        MockConnection::register_channel::<Timestamped<WorldType::SnapshotType>>(
            &mut client_2_connection,
            &mut server_2_connection,
        );

        MockConnection::register_channel::<Timestamped<WorldType::CommandType>>(
            &mut client_1_connection,
            &mut server_1_connection,
        );
        MockConnection::register_channel::<Timestamped<WorldType::CommandType>>(
            &mut client_2_connection,
            &mut server_2_connection,
        );

        client_1_net.connections.insert(0, client_1_connection);
        client_2_net.connections.insert(0, client_2_connection);
        server_net.connections.insert(0, server_1_connection);
        server_net.connections.insert(1, server_2_connection);

        (server_net, (client_1_net, client_2_net))
    }

    pub fn connect(&mut self) {
        for connection in self.connections.values_mut() {
            connection.is_connected.set(true);
        }
    }

    pub fn disconnect(&mut self) {
        for connection in self.connections.values_mut() {
            connection.is_connected.set(false);
        }
    }

    pub fn set_delay(&mut self, delay: f64) {
        for connection in self.connections.values_mut() {
            connection.set_delay(delay);
        }
    }

    pub fn tick(&mut self, delta_seconds: f64) {
        for connection in self.connections.values_mut() {
            connection.tick(delta_seconds);
        }
    }
}

pub struct MockConnection {
    pub channels: HashMap<TypeId, Box<dyn DelayedChannel>>,
    pub is_connected: Rc<Cell<bool>>,
}

impl MockConnection {
    pub fn new_pair() -> (MockConnection, MockConnection) {
        let connection_1 = MockConnection {
            channels: Default::default(),
            is_connected: Rc::new(Cell::new(false)),
        };
        let connection_2 = MockConnection {
            channels: Default::default(),
            is_connected: connection_1.is_connected.clone(),
        };
        (connection_1, connection_2)
    }

    pub fn register_channel<T: 'static>(
        connection_1: &mut MockConnection,
        connection_2: &mut MockConnection,
    ) {
        let (channel_1, channel_2) = MockChannel::<T>::new_pair();
        let key = TypeId::of::<T>();
        connection_1.channels.insert(key, Box::new(channel_1));
        connection_2.channels.insert(key, Box::new(channel_2));
    }

    pub fn get_mut<T: 'static>(&mut self) -> &mut MockChannel<T> {
        let key = TypeId::of::<T>();
        self.channels
            .get_mut(&key)
            .unwrap_or_else(|| {
                panic!(
                    "Message of type {:?} should be registered",
                    type_name::<T>()
                )
            })
            .as_any()
            .downcast_mut()
            .expect("Channel is of the right type")
    }

    pub fn set_delay(&mut self, delay: f64) {
        for channel in self.channels.values_mut() {
            channel.set_delay(delay);
        }
    }

    pub fn tick(&mut self, delta_seconds: f64) {
        for channel in self.channels.values_mut() {
            channel.tick(delta_seconds);
        }
    }
}

pub struct MockChannel<T> {
    pub inbox: Rc<RefCell<DelayedQueue<T>>>,
    pub outbox: Rc<RefCell<DelayedQueue<T>>>,
}

pub trait DelayedChannel {
    fn tick(&mut self, delta_seconds: f64);
    fn set_delay(&mut self, delay: f64);
    fn as_any(&mut self) -> &mut dyn Any;
}

impl<T> MockChannel<T> {
    pub fn new_pair() -> (MockChannel<T>, MockChannel<T>) {
        let channel_1 = MockChannel {
            inbox: Rc::new(RefCell::new(DelayedQueue::new())),
            outbox: Rc::new(RefCell::new(DelayedQueue::new())),
        };
        let channel_2 = MockChannel {
            inbox: channel_1.outbox.clone(),
            outbox: channel_1.inbox.clone(),
        };
        (channel_1, channel_2)
    }

    pub fn send(&mut self, message: T) -> Option<T> {
        self.outbox.borrow_mut().send(message)
    }

    pub fn recv(&mut self) -> Option<T> {
        self.inbox.borrow_mut().recv()
    }

    pub fn new_outgoing_activity_count(&mut self) -> usize {
        self.outbox.borrow_mut().new_activity_count()
    }

    pub fn new_incoming_activity_count(&mut self) -> usize {
        self.inbox.borrow_mut().new_activity_count()
    }
}

impl<T: 'static> DelayedChannel for MockChannel<T> {
    fn tick(&mut self, delta_seconds: f64) {
        self.inbox.borrow_mut().tick(delta_seconds);
    }

    fn set_delay(&mut self, delay: f64) {
        self.inbox.borrow_mut().set_delay(delay);
        self.outbox.borrow_mut().set_delay(delay);
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}

#[derive(Default)]
pub struct DelayedQueue<T> {
    new_activity_count: usize,
    incoming: VecDeque<(T, f64)>,
    outgoing: VecDeque<T>,
    delay: f64,
}

impl<T> DelayedQueue<T> {
    pub fn new() -> Self {
        Self {
            new_activity_count: 0,
            incoming: VecDeque::new(),
            outgoing: VecDeque::new(),
            delay: 0.0,
        }
    }

    pub fn set_delay(&mut self, delay: f64) {
        self.delay = delay;
    }

    pub fn tick(&mut self, delta_seconds: f64) {
        while self
            .incoming
            .front()
            .map_or(false, |(_, age)| *age >= self.delay)
        {
            self.outgoing
                .push_back(self.incoming.pop_front().unwrap().0);
        }

        for (_, age) in &mut self.incoming {
            // Note: Never decrement age. The network is immune to timeskips for our
            // demo.
            *age += delta_seconds.max(0.0);
        }
    }

    pub fn send(&mut self, message: T) -> Option<T> {
        self.new_activity_count += 1;
        self.incoming.push_back((message, 0.0));
        None
    }

    pub fn recv(&mut self) -> Option<T> {
        self.outgoing.pop_front()
    }

    pub fn new_activity_count(&mut self) -> usize {
        let count = self.new_activity_count;
        self.new_activity_count = 0;
        count
    }
}

pub struct MockConnectionRef<'a>(&'a mut MockConnection);

impl<WorldType: World> NetworkResource<WorldType> for MockNetwork {
    type ConnectionType<'a> = MockConnectionRef<'a>;

    fn get_connection(&mut self, handle: ConnectionHandleType) -> Option<Self::ConnectionType<'_>> {
        self.connections
            .get_mut(&handle)
            .filter(|connection| connection.is_connected.get())
            .map(|connection| MockConnectionRef(connection))
    }

    fn connections<'a>(
        &'a mut self,
    ) -> Box<dyn Iterator<Item = (ConnectionHandleType, Self::ConnectionType<'a>)> + 'a> {
        Box::new(
            self.connections
                .iter_mut()
                .filter(|(_handle, connection)| connection.is_connected.get())
                .map(|(handle, connection)| (*handle, MockConnectionRef(connection))),
        )
    }
}

impl<'a, WorldType: World> Connection<WorldType> for MockConnectionRef<'a> {
    fn recv_command(&mut self) -> Option<Timestamped<WorldType::CommandType>> {
        assert!(self.0.is_connected.get());
        self.0
            .get_mut::<Timestamped<WorldType::CommandType>>()
            .recv()
    }

    fn recv_snapshot(&mut self) -> Option<Timestamped<WorldType::SnapshotType>> {
        assert!(self.0.is_connected.get());
        self.0
            .get_mut::<Timestamped<WorldType::SnapshotType>>()
            .recv()
    }

    fn recv_clock_sync(&mut self) -> Option<ClockSyncMessage> {
        assert!(self.0.is_connected.get());
        self.0.get_mut::<ClockSyncMessage>().recv()
    }

    fn send<MessageType>(&mut self, message: MessageType) -> Option<MessageType>
    where
        MessageType: Debug + Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
    {
        assert!(self.0.is_connected.get());
        self.0.get_mut::<MessageType>().send(message)
    }
    fn flush<MessageType>(&mut self)
    where
        MessageType: Debug + Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
    {
        assert!(self.0.is_connected.get());
        // No-op in our mock network. You may want to forward this call to your network library.
    }
}
