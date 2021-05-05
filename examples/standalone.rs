#![feature(generic_associated_types)]

use crystalorb::{
    client::{Client, ClientStage},
    command::Command,
    fixed_timestepper::Stepper,
    server::Server,
    world::{DisplayState, World},
    Config, TweeningMethod,
};
use crystalorb_mock_network::MockNetwork;
use serde::{Deserialize, Serialize};
use std::{fmt::Debug, time::Instant};
use tracing::Level;
use tracing_subscriber;

#[derive(Default)]
pub struct MyWorld {
    position: f64,
    velocity: f64,

    // Your World implementation might contain cached state/calculations, for example.
    cached_momentum: Option<f64>,
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
    position: f64,
    velocity: f64,
}

#[derive(Clone, Default, Debug)]
pub struct MyDisplayState {
    position: f64,
    // Unless you use the velocity value for rendering in some way (e.g. motion blur), you might
    // not need to include it here in this display state.
    velocity: f64,
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
        const DELTA_SECONDS: f64 = 1.0 / 60.0;
        const MASS: f64 = 2.0;
        self.position += self.velocity * DELTA_SECONDS;
        self.cached_momentum = Some(self.velocity * MASS);
    }
}

impl DisplayState for MyDisplayState {
    fn from_interpolation(state1: &Self, state2: &Self, t: f64) -> Self {
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

fn main() {
    tracing_subscriber::fmt().with_max_level(Level::WARN).init();

    let (mut server_net, (mut client_1_net, mut client_2_net)) =
        MockNetwork::new_mock_network::<MyWorld>();

    client_1_net.connect();
    client_2_net.connect();

    let config = Config {
        lag_compensation_latency: 0.3,
        blend_latency: 0.2,
        timestep_seconds: 1.0 / 60.0,
        clock_sync_needed_sample_count: 32,
        clock_sync_request_period: 0.2,
        clock_sync_assumed_outlier_rate: 0.2,
        max_tolerable_clock_deviation: 0.1,
        snapshot_send_period: 0.1,
        update_delta_seconds_max: 0.25,
        timestamp_skip_threshold_seconds: 1.0,
        fastforward_max_per_step: 10,
        tweening_method: TweeningMethod::Interpolated,
    };

    let mut client_1 = Client::<MyWorld>::new(config.clone());
    let mut client_2 = Client::<MyWorld>::new(config.clone());
    let mut server = Server::<MyWorld>::new(config.clone(), 0.0);

    let startup_time = Instant::now();
    let mut previous_time = Instant::now();

    loop {
        let current_time = Instant::now();
        let delta_seconds = current_time.duration_since(previous_time).as_secs_f64();
        let seconds_since_startup = current_time.duration_since(startup_time).as_secs_f64();

        let server_display_state = server.display_state();
        let mut client_1_display_state = None;
        let mut client_2_display_state = None;

        if let ClientStage::Ready(ready_client_1) = client_1.stage_mut() {
            if (0.0..1.0).contains(&(seconds_since_startup % 10.0)) {
                ready_client_1.issue_command(MyCommand::Accelerate, &mut client_1_net);
            }
            client_1_display_state = Some(ready_client_1.display_state());
        }
        if let ClientStage::Ready(ready_client_2) = client_2.stage_mut() {
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

        client_1_net.tick(delta_seconds);
        client_2_net.tick(delta_seconds);
        server_net.tick(delta_seconds);

        previous_time = current_time;
    }
}
