use crystalorb::{
    client::{Client, ClientState},
    clocksync::ClockSyncMessage,
    command::Command,
    fixed_timestepper::Stepper,
    network_resource::{Connection, ConnectionHandleType, NetworkResource},
    server::Server,
    timestamp::Timestamped,
    world::{DisplayState, World},
    Config,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    any::{type_name, Any, TypeId},
    cell::{Cell, RefCell},
    collections::{HashMap, VecDeque},
    error::Error,
    fmt::Debug,
    rc::Rc,
};
use tracing::info;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct MockWorld {
    pub initial_empty_ticks: usize,
    pub command_history: Vec<Vec<MockCommand>>,
    pub dx: i32,
    pub x: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct MockCommand(pub i32);

impl Default for MockWorld {
    fn default() -> Self {
        Self {
            initial_empty_ticks: 0,
            command_history: vec![vec![]],
            dx: 0,
            x: 0,
        }
    }
}

impl World for MockWorld {
    type CommandType = MockCommand;
    type SnapshotType = MockWorld;
    type DisplayStateType = MockWorld;

    fn command_is_valid(_command: &MockCommand, _client_id: usize) -> bool {
        true
    }

    fn apply_command(&mut self, command: &MockCommand) {
        self.dx += command.0;
        self.command_history
            .last_mut()
            .unwrap()
            .push(command.clone());
    }

    fn apply_snapshot(&mut self, snapshot: MockWorld) {
        self.initial_empty_ticks = snapshot.initial_empty_ticks;
        self.command_history = snapshot.command_history;
        self.dx = snapshot.dx;
        self.x = snapshot.x;
    }

    fn snapshot(&self) -> MockWorld {
        self.clone()
    }

    fn display_state(&self) -> MockWorld {
        self.clone()
    }
}

impl Command for MockCommand {}

impl Stepper for MockWorld {
    fn step(&mut self) {
        self.x += self.dx as i64;
        if self.command_history.len() == 1 && self.command_history.get(0).unwrap().len() == 0 {
            self.initial_empty_ticks += 1;
        } else {
            self.command_history.push(Vec::new());
        }
    }
}

impl DisplayState for MockWorld {
    fn from_interpolation(state1: &Self, state2: &Self, t: f64) -> Self {
        if t == 1.0 {
            state2.clone()
        } else {
            state1.clone()
        }
    }
}

impl NetworkResource for MyNetwork {
    type ConnectionType<'a> = MockConnectionRef<'a>;
    fn broadcast_message<MessageType>(&mut self, message: MessageType)
    where
        MessageType: Debug + Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
    {
        for connection in self.connections.values_mut() {
            if connection.is_connected.get() {
                connection.get_mut::<MessageType>().send(message.clone());
            }
        }
    }
    fn send_message<MessageType>(
        &mut self,
        handle: ConnectionHandleType,
        message: MessageType,
    ) -> Result<Option<MessageType>, Box<dyn Error + Send>>
    where
        MessageType: Debug + Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
    {
        let connection = self.connections.get_mut(&handle).unwrap();
        assert!(connection.is_connected.get());
        Ok(connection.get_mut::<MessageType>().send(message))
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

impl<'a> Connection for MockConnectionRef<'a> {
    fn recv<MessageType>(&mut self) -> Option<MessageType>
    where
        MessageType: Debug + Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
    {
        assert!(self.0.is_connected.get());
        self.0.get_mut::<MessageType>().recv()
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

#[derive(Default)]
pub struct MyNetwork {
    pub connections: HashMap<ConnectionHandleType, MockConnection>,
}

impl MyNetwork {
    pub fn new_mock_network() -> (MyNetwork, (MyNetwork, MyNetwork)) {
        let mut client_1_net = MyNetwork::default();
        let mut client_2_net = MyNetwork::default();
        let mut server_net = MyNetwork::default();

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

        MockConnection::register_channel::<Timestamped<MockWorld>>(
            &mut client_1_connection,
            &mut server_1_connection,
        );
        MockConnection::register_channel::<Timestamped<MockWorld>>(
            &mut client_2_connection,
            &mut server_2_connection,
        );

        MockConnection::register_channel::<Timestamped<MockCommand>>(
            &mut client_1_connection,
            &mut server_1_connection,
        );
        MockConnection::register_channel::<Timestamped<MockCommand>>(
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

    pub fn tick(&mut self) {
        for connection in self.connections.values_mut() {
            connection.tick();
        }
    }
}

pub struct MockConnection {
    pub channels: HashMap<TypeId, Box<dyn Tick>>,
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
            .expect(&format!(
                "Message of type {:?} should be registered",
                type_name::<T>()
            ))
            .as_any()
            .downcast_mut()
            .expect("Channel is of the right type")
    }

    pub fn tick(&mut self) {
        for channel in self.channels.values_mut() {
            channel.tick();
        }
    }
}

pub struct MockChannel<T> {
    pub inbox: Rc<RefCell<DelayedQueue<T>>>,
    pub outbox: Rc<RefCell<DelayedQueue<T>>>,
}

pub trait Tick {
    fn tick(&mut self);
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
}

impl<T: 'static> Tick for MockChannel<T> {
    fn tick(&mut self) {
        self.inbox.borrow_mut().tick();
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}

#[derive(Default)]
pub struct DelayedQueue<T> {
    incoming: VecDeque<T>,
    outgoing: VecDeque<T>,
}

impl<T> DelayedQueue<T> {
    pub fn new() -> Self {
        Self {
            incoming: VecDeque::new(),
            outgoing: VecDeque::new(),
        }
    }

    pub fn tick(&mut self) {
        self.outgoing.append(&mut self.incoming);
    }

    pub fn send(&mut self, message: T) -> Option<T> {
        self.incoming.push_back(message);
        None
    }

    pub fn recv(&mut self) -> Option<T> {
        self.outgoing.pop_front()
    }
}

pub struct MockConnectionRef<'a>(&'a mut MockConnection);

pub struct MockClientServer {
    pub config: Config,
    pub server: Server<MockWorld>,
    pub client_1: Client<MockWorld>,
    pub client_2: Client<MockWorld>,
    pub server_net: MyNetwork,
    pub client_1_net: MyNetwork,
    pub client_2_net: MyNetwork,
    pub clock: f64,
    pub client_1_clock_offset: f64,
    pub client_2_clock_offset: f64,
}

impl MockClientServer {
    pub fn new(config: Config) -> MockClientServer {
        let (server_net, (client_1_net, client_2_net)) = MyNetwork::new_mock_network();
        MockClientServer {
            config: config.clone(),
            client_1: Client::<MockWorld>::new(config.clone()),
            client_2: Client::<MockWorld>::new(config.clone()),
            server: Server::<MockWorld>::new(config.clone()),
            server_net,
            client_1_net,
            client_2_net,
            client_1_clock_offset: 0.0,
            client_2_clock_offset: 0.0,

            // Start quarterway to avoid aliasing/precision issues.
            // Note: not halfway, since that is the threshold for timestamp drift.
            clock: config.timestep_seconds * 0.25,
        }
    }

    pub fn update_until_clients_ready(&mut self, delta_seconds: f64) {
        while !matches!(self.client_1.state(), ClientState::Ready(_))
            || !matches!(self.client_2.state(), ClientState::Ready(_))
        {
            self.update(delta_seconds);
        }

        info!("");
        info!("##################### Ready #######################");
        info!("");
    }

    pub fn update(&mut self, delta_seconds: f64) {
        info!("");
        info!(
            "------------ Update by {} seconds --------------------------",
            delta_seconds
        );
        self.clock += delta_seconds;
        info!(
            "------------ >> Update server by {} seconds --------------------------",
            delta_seconds
        );
        self.server
            .update(delta_seconds, self.clock, &mut self.server_net);
        info!(
            "------------ >> Update client 1 by {} seconds --------------------------",
            delta_seconds
        );
        self.client_1.update(
            delta_seconds,
            self.clock + self.client_1_clock_offset,
            &mut self.client_1_net,
        );
        info!(
            "------------ >> Update client 2 by {} seconds --------------------------",
            delta_seconds
        );
        self.client_2.update(
            delta_seconds,
            self.clock + self.client_2_clock_offset,
            &mut self.client_2_net,
        );

        self.client_1_net.tick();
        self.client_2_net.tick();
        self.server_net.tick();
    }
}
