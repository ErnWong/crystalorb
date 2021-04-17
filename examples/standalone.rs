#![feature(generic_associated_types)]

use crystalorb::{
    channels::ClockSyncMessage,
    client::{Client, ClientState},
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
    cell::RefCell,
    collections::{HashMap, VecDeque},
    error::Error,
    fmt::Debug,
    rc::Rc,
    time::Instant,
};
use tracing::Level;
use tracing_subscriber;

#[derive(Default)]
pub struct MyWorld {
    position: f32,
    velocity: f32,

    // Your World implementation might contain cached state/calculations, for example.
    cached_momentum: Option<f32>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum MyCommand {
    // Here, you would put down the things that you want to externally affect the physics
    // simulation. The most common would be player commands. Other things might include spawning
    // npcs or triggering high-level events if they are not part of the physics simulation.
    Accelerate,
    Decelerate,
    Cheat,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct MySnapshot {
    // Here, you would probably want to put down the minimal subset of states that can be used to
    // describe the whole physics simulation at any point of time.
    position: f32,
    velocity: f32,
}

#[derive(Clone, Default, Debug)]
pub struct MyDisplayState {
    position: f32,
    // Unless you use the velocity value for rendering in some way (e.g. motion blur), you might
    // not need to include it here in this display state.
    velocity: f32,
    // You might also include other derived state that are useful for rendering.
}

impl World for MyWorld {
    type CommandType = MyCommand;
    type SnapshotType = MySnapshot;
    type DisplayStateType = MyDisplayState;

    fn command_is_valid(command: &MyCommand, client_id: usize) -> bool {
        // Only client 42 has permission to cheat, for example.
        match command {
            MyCommand::Cheat => client_id == 42,
            _ => true,
        }
    }

    fn apply_command(&mut self, command: &MyCommand) {
        match command {
            MyCommand::Accelerate => self.velocity += 1.0,
            MyCommand::Decelerate => self.velocity -= 1.0,
            MyCommand::Cheat => self.position = 0.0,
        }
    }

    fn apply_snapshot(&mut self, snapshot: MySnapshot) {
        self.position = snapshot.position;
        self.velocity = snapshot.velocity;
        self.cached_momentum = None;
    }

    fn snapshot(&self) -> MySnapshot {
        MySnapshot {
            position: self.position,
            velocity: self.velocity,
        }
    }

    fn display_state(&self) -> MyDisplayState {
        MyDisplayState {
            position: self.position,
            velocity: self.velocity,
        }
    }
}

impl Command for MyCommand {}

impl Stepper for MyWorld {
    fn step(&mut self) {
        const DELTA_SECONDS: f32 = 1.0 / 60.0;
        const MASS: f32 = 2.0;
        self.position += self.velocity * DELTA_SECONDS;
        self.cached_momentum = Some(self.velocity * MASS);
    }
}

impl DisplayState for MyDisplayState {
    fn from_interpolation(state1: &Self, state2: &Self, t: f32) -> Self {
        MyDisplayState {
            position: (1.0 - t) * state1.position + t * state2.position,
            velocity: (1.0 - t) * state1.velocity + t * state2.velocity,
            // You can, for example, also do some more complex interpolation such as SLERP for
            // things that undergo rotation. To prevent some weird interpolation glitches (such as
            // deformable bodies imploding into themselves), you may need to transform points into
            // their local coordinates before interpolating.
        }
    }
}

impl NetworkResource for MyNetwork {
    type ConnectionType<'a> = MyConnectionRef<'a>;
    fn broadcast_message<MessageType>(&mut self, message: MessageType)
    where
        MessageType: Debug + Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
    {
        for connection in self.connections.values_mut() {
            connection.get_mut::<MessageType>().send(message.clone());
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
        Ok(self
            .connections
            .get_mut(&handle)
            .unwrap()
            .get_mut::<MessageType>()
            .send(message))
    }
    fn connections<'a>(
        &'a mut self,
    ) -> Box<dyn Iterator<Item = (ConnectionHandleType, Self::ConnectionType<'a>)> + 'a> {
        Box::new(
            self.connections
                .iter_mut()
                .map(|(handle, connection)| (*handle, MyConnectionRef(connection))),
        )
    }
}

impl<'a> Connection for MyConnectionRef<'a> {
    fn recv<MessageType>(&mut self) -> Option<MessageType>
    where
        MessageType: Debug + Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
    {
        self.0.get_mut::<MessageType>().recv()
    }
    fn send<MessageType>(&mut self, message: MessageType) -> Option<MessageType>
    where
        MessageType: Debug + Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
    {
        self.0.get_mut::<MessageType>().send(message)
    }
    fn flush<MessageType>(&mut self)
    where
        MessageType: Debug + Clone + Serialize + DeserializeOwned + Send + Sync + 'static,
    {
        // No-op in our mock network. You may want to forward this call to your network library.
    }
}

fn main() {
    tracing_subscriber::fmt().with_max_level(Level::WARN).init();

    let (mut server_net, (mut client_1_net, mut client_2_net)) = MyNetwork::new_mock_network();

    let config = Config {
        lag_compensation_latency: 0.3,
        interpolation_latency: 0.2,
        timestep_seconds: 1.0 / 60.0,
        timestamp_sync_needed_sample_count: 32,
        initial_clock_sync_period: 0.2,
        heartbeat_period: 0.7,
        snapshot_send_period: 0.1,
        update_delta_seconds_max: 0.25,
        timestamp_skip_threshold_seconds: 1.0,
        fastforward_max_per_step: 10,
        clock_offset_update_factor: 0.1,
    };

    let mut client_1 = Client::<MyWorld>::new(config.clone());
    let mut client_2 = Client::<MyWorld>::new(config.clone());
    let mut server = Server::<MyWorld>::new(config.clone());

    let startup_time = Instant::now();
    let mut previous_time = Instant::now();

    loop {
        let current_time = Instant::now();
        let delta_seconds = current_time.duration_since(previous_time).as_secs_f32();
        let seconds_since_startup = current_time.duration_since(startup_time).as_secs_f64();

        let server_display_state = server.display_state();
        let mut client_1_display_state = None;
        let mut client_2_display_state = None;

        if let ClientState::Ready(ready_client_1) = client_1.state_mut() {
            if (0.0..1.0).contains(&(seconds_since_startup % 10.0)) {
                ready_client_1.issue_command(MyCommand::Accelerate, &mut client_1_net);
            }
            client_1_display_state = Some(ready_client_1.display_state());
        }
        if let ClientState::Ready(ready_client_2) = client_2.state_mut() {
            if (5.0..6.0).contains(&(seconds_since_startup % 10.0)) {
                ready_client_2.issue_command(MyCommand::Decelerate, &mut client_2_net);
            }
            client_2_display_state = Some(ready_client_2.display_state());
        }

        println!(
            "Server: {:?}, Client 1: {:?}, Client 2: {:?}",
            server_display_state, client_1_display_state, client_2_display_state
        );

        client_1.update(delta_seconds, seconds_since_startup, &mut client_1_net);
        client_2.update(delta_seconds, seconds_since_startup, &mut client_2_net);
        server.update(delta_seconds, seconds_since_startup, &mut server_net);

        previous_time = current_time;
    }
}

// Dummy network backed by vecs.

#[derive(Default)]
pub struct MyNetwork {
    pub connections: HashMap<ConnectionHandleType, MyConnection>,
}

impl MyNetwork {
    pub fn new_mock_network() -> (MyNetwork, (MyNetwork, MyNetwork)) {
        let mut client_1_net = MyNetwork::default();
        let mut client_2_net = MyNetwork::default();
        let mut server_net = MyNetwork::default();

        let mut client_1_connection = MyConnection::default();
        let mut client_2_connection = MyConnection::default();
        let mut server_1_connection = MyConnection::default();
        let mut server_2_connection = MyConnection::default();

        MyConnection::register_channel::<ClockSyncMessage>(
            &mut client_1_connection,
            &mut server_1_connection,
        );
        MyConnection::register_channel::<ClockSyncMessage>(
            &mut client_2_connection,
            &mut server_2_connection,
        );

        MyConnection::register_channel::<Timestamped<MySnapshot>>(
            &mut client_1_connection,
            &mut server_1_connection,
        );
        MyConnection::register_channel::<Timestamped<MySnapshot>>(
            &mut client_2_connection,
            &mut server_2_connection,
        );

        MyConnection::register_channel::<Timestamped<MyCommand>>(
            &mut client_1_connection,
            &mut server_1_connection,
        );
        MyConnection::register_channel::<Timestamped<MyCommand>>(
            &mut client_2_connection,
            &mut server_2_connection,
        );

        client_1_net.connections.insert(0, client_1_connection);
        client_2_net.connections.insert(0, client_2_connection);
        server_net.connections.insert(0, server_1_connection);
        server_net.connections.insert(1, server_2_connection);

        (server_net, (client_1_net, client_2_net))
    }
}

#[derive(Default)]
pub struct MyConnection {
    pub channels: HashMap<TypeId, Box<dyn Any>>,
}

impl MyConnection {
    pub fn register_channel<T: 'static>(
        connection_1: &mut MyConnection,
        connection_2: &mut MyConnection,
    ) {
        let (channel_1, channel_2) = MyChannel::<T>::new_pair();
        let key = TypeId::of::<T>();
        connection_1.channels.insert(key, Box::new(channel_1));
        connection_2.channels.insert(key, Box::new(channel_2));
    }

    pub fn get_mut<T: 'static>(&mut self) -> &mut MyChannel<T> {
        let key = TypeId::of::<T>();
        self.channels
            .get_mut(&key)
            .expect(&format!(
                "Message of type {:?} should be registered",
                type_name::<T>()
            ))
            .downcast_mut()
            .expect("Channel is of the right type")
    }
}

pub struct MyChannel<T> {
    pub inbox: Rc<RefCell<VecDeque<T>>>,
    pub outbox: Rc<RefCell<VecDeque<T>>>,
}

impl<T> MyChannel<T> {
    pub fn new_pair() -> (MyChannel<T>, MyChannel<T>) {
        let channel_1 = MyChannel {
            inbox: Rc::new(RefCell::new(VecDeque::new())),
            outbox: Rc::new(RefCell::new(VecDeque::new())),
        };
        let channel_2 = MyChannel {
            inbox: channel_1.outbox.clone(),
            outbox: channel_1.inbox.clone(),
        };
        (channel_1, channel_2)
    }

    pub fn send(&mut self, message: T) -> Option<T> {
        self.outbox.borrow_mut().push_back(message);
        None
    }

    pub fn recv(&mut self) -> Option<T> {
        self.inbox.borrow_mut().pop_front()
    }
}

pub struct MyConnectionRef<'a>(&'a mut MyConnection);
