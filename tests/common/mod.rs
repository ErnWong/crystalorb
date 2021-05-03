use crystalorb::{
    client::{Client, ClientState},
    command::Command,
    fixed_timestepper::Stepper,
    server::Server,
    world::{DisplayState, World},
    Config,
};
use crystalorb_mock_network::MockNetwork;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
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

pub struct MockClientServer {
    pub config: Config,
    pub server: Server<MockWorld>,
    pub client_1: Client<MockWorld>,
    pub client_2: Client<MockWorld>,
    pub server_net: MockNetwork,
    pub client_1_net: MockNetwork,
    pub client_2_net: MockNetwork,
    pub clock: f64,
    pub client_1_clock_offset: f64,
    pub client_2_clock_offset: f64,
}

impl MockClientServer {
    pub fn new(config: Config) -> MockClientServer {
        let (server_net, (client_1_net, client_2_net)) =
            MockNetwork::new_mock_network::<MockWorld>();

        // Start quarterway to avoid aliasing/precision issues.
        // Note: not halfway, since that is the threshold for timestamp drift.
        let clock_initial = config.timestep_seconds * 0.25;

        MockClientServer {
            config: config.clone(),
            client_1: Client::<MockWorld>::new(config.clone()),
            client_2: Client::<MockWorld>::new(config.clone()),
            server: Server::<MockWorld>::new(config.clone(), clock_initial),
            server_net,
            client_1_net,
            client_2_net,
            client_1_clock_offset: 0.0,
            client_2_clock_offset: 0.0,
            clock: clock_initial,
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

        self.client_1_net.tick(delta_seconds);
        self.client_2_net.tick(delta_seconds);
        self.server_net.tick(delta_seconds);
    }
}
