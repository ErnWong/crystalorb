use crystalorb::{
    client::{
        stage::{Stage, StageMut},
        Client,
    },
    clocksync::ClockSyncMessage,
    command::Command,
    fixed_timestepper::Stepper,
    server::Server,
    timestamp::Timestamped,
    world::{DisplayState, World},
    Config, TweeningMethod,
};
use crystalorb_mock_network::MockNetwork;
use js_sys::Array;
use rapier2d::{na::Vector2, prelude::*};
use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Display, Formatter};
use tracing::Level;
use wasm_bindgen::prelude::*;

const GRAVITY: Vector2<Real> = Vector2::new(0.0, -9.81 * 30.0);
const TIMESTEP: f64 = 1.0 / 64.0;

pub struct DemoWorld {
    pipeline: PhysicsPipeline,
    island_manager: IslandManager,
    broad_phase: BroadPhase,
    narrow_phase: NarrowPhase,
    bodies: RigidBodySet,
    colliders: ColliderSet,
    joints: JointSet,
    ccd_solver: CCDSolver,
    player_left: Player,
    player_right: Player,
    doodad: Player,
}

pub struct Player {
    body_handle: RigidBodyHandle,
    _collider_handle: ColliderHandle,
    input: PlayerInput,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, Copy)]
pub struct PlayerInput {
    jump: bool,
    left: bool,
    right: bool,
}

#[wasm_bindgen]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DemoCommand {
    pub player_side: PlayerSide,
    pub command: PlayerCommand,
    pub value: bool,
}

#[wasm_bindgen]
impl DemoCommand {
    #[wasm_bindgen(constructor)]
    pub fn new(player_side: PlayerSide, command: PlayerCommand, value: bool) -> Self {
        Self {
            player_side,
            command,
            value,
        }
    }
}

impl Display for DemoCommand {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} {} {}",
            match self.player_side {
                PlayerSide::Left => "P1",
                PlayerSide::Right => "P2",
            },
            match self.command {
                PlayerCommand::Left => "Left",
                PlayerCommand::Right => "Right",
                PlayerCommand::Jump => "Jump",
            },
            match self.value {
                true => "On",
                false => "Off",
            }
        )
    }
}

#[wasm_bindgen]
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub enum PlayerSide {
    Left,
    Right,
}

#[wasm_bindgen]
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub enum PlayerCommand {
    Jump,
    Left,
    Right,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DemoSnapshot {
    player_left: PlayerSnapshot,
    player_right: PlayerSnapshot,
    doodad: PlayerSnapshot,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PlayerSnapshot {
    position: Isometry<Real>,
    linvel: Vector2<Real>,
    angvel: Real,
    input: PlayerInput,
}

#[wasm_bindgen]
#[derive(Clone, Debug)]
pub struct DemoDisplayState {
    player_left_position: Isometry<Real>,
    player_right_position: Isometry<Real>,
    doodad_position: Isometry<Real>,
}

#[wasm_bindgen]
impl DemoDisplayState {
    pub fn player_left_translation_x(&self) -> Real {
        self.player_left_position.translation.vector[0]
    }

    pub fn player_left_translation_y(&self) -> Real {
        self.player_left_position.translation.vector[1]
    }

    pub fn player_left_angle(&self) -> Real {
        self.player_left_position.rotation.angle()
    }

    pub fn player_right_translation_x(&self) -> Real {
        self.player_right_position.translation.vector[0]
    }

    pub fn player_right_translation_y(&self) -> Real {
        self.player_right_position.translation.vector[1]
    }

    pub fn player_right_angle(&self) -> Real {
        self.player_right_position.rotation.angle()
    }

    pub fn doodad_translation_x(&self) -> Real {
        self.doodad_position.translation.vector[0]
    }

    pub fn doodad_translation_y(&self) -> Real {
        self.doodad_position.translation.vector[1]
    }

    pub fn doodad_angle(&self) -> Real {
        self.doodad_position.rotation.angle()
    }
}

impl Default for DemoWorld {
    fn default() -> Self {
        Self::new()
    }
}

impl DemoWorld {
    pub fn new() -> Self {
        let mut bodies = RigidBodySet::new();
        let mut colliders = ColliderSet::new();
        colliders.insert_with_parent(
            ColliderBuilder::cuboid(1.0, 100.0).restitution(0.5).build(),
            bodies.insert(
                RigidBodyBuilder::new_static()
                    .translation(vector![0.0, 0.0])
                    .ccd_enabled(true)
                    .build(),
            ),
            &mut bodies,
        );
        colliders.insert_with_parent(
            ColliderBuilder::cuboid(1.0, 100.0).restitution(0.5).build(),
            bodies.insert(
                RigidBodyBuilder::new_static()
                    .translation(vector![180.0, 0.0])
                    .ccd_enabled(true)
                    .build(),
            ),
            &mut bodies,
        );
        colliders.insert_with_parent(
            ColliderBuilder::cuboid(180.0, 1.0).restitution(0.5).build(),
            bodies.insert(
                RigidBodyBuilder::new_static()
                    .translation(vector![0.0, 0.0])
                    .ccd_enabled(true)
                    .build(),
            ),
            &mut bodies,
        );
        colliders.insert_with_parent(
            ColliderBuilder::cuboid(180.0, 1.0).restitution(0.5).build(),
            bodies.insert(
                RigidBodyBuilder::new_static()
                    .translation(vector![0.0, 100.0])
                    .ccd_enabled(true)
                    .build(),
            ),
            &mut bodies,
        );
        let left_body_handle = bodies.insert(
            RigidBodyBuilder::new_dynamic()
                .translation(vector![10.0, 80.0])
                .ccd_enabled(true)
                .build(),
        );
        let right_body_handle = bodies.insert(
            RigidBodyBuilder::new_dynamic()
                .translation(vector![150.0, 80.0])
                .ccd_enabled(true)
                .build(),
        );
        let doodad_body_handle = bodies.insert(
            RigidBodyBuilder::new_dynamic()
                .translation(vector![80.0, 80.0])
                .ccd_enabled(true)
                .build(),
        );
        let left_collider_handle = colliders.insert_with_parent(
            ColliderBuilder::ball(10.0)
                .density(0.1)
                .restitution(0.5)
                .build(),
            left_body_handle,
            &mut bodies,
        );
        let right_collider_handle = colliders.insert_with_parent(
            ColliderBuilder::ball(10.0)
                .density(0.1)
                .restitution(0.5)
                .build(),
            right_body_handle,
            &mut bodies,
        );
        let doodad_collider_handle = colliders.insert_with_parent(
            ColliderBuilder::ball(10.0)
                .density(0.1)
                .restitution(0.5)
                .build(),
            doodad_body_handle,
            &mut bodies,
        );
        Self {
            pipeline: PhysicsPipeline::new(),
            island_manager: IslandManager::new(),
            broad_phase: BroadPhase::new(),
            narrow_phase: NarrowPhase::new(),
            bodies,
            colliders,
            joints: JointSet::new(),
            ccd_solver: CCDSolver::new(),
            player_left: Player {
                body_handle: left_body_handle,
                _collider_handle: left_collider_handle,
                input: Default::default(),
            },
            player_right: Player {
                body_handle: right_body_handle,
                _collider_handle: right_collider_handle,
                input: Default::default(),
            },
            doodad: Player {
                body_handle: doodad_body_handle,
                _collider_handle: doodad_collider_handle,
                input: Default::default(),
            },
        }
    }
}

impl World for DemoWorld {
    type CommandType = DemoCommand;
    type SnapshotType = DemoSnapshot;
    type DisplayStateType = DemoDisplayState;

    fn command_is_valid(command: &Self::CommandType, client_id: usize) -> bool {
        match command.player_side {
            PlayerSide::Left => client_id == 0,
            PlayerSide::Right => client_id == 1,
        }
    }

    fn apply_command(&mut self, command: &Self::CommandType) {
        let player_input = &mut match command.player_side {
            PlayerSide::Left => &mut self.player_left,
            PlayerSide::Right => &mut self.player_right,
        }
        .input;
        match command.command {
            PlayerCommand::Jump => player_input.jump = command.value,
            PlayerCommand::Left => player_input.left = command.value,
            PlayerCommand::Right => player_input.right = command.value,
        }
    }

    fn apply_snapshot(&mut self, snapshot: Self::SnapshotType) {
        let body_left = self.bodies.get_mut(self.player_left.body_handle).unwrap();
        body_left.set_position(snapshot.player_left.position, true);
        body_left.set_linvel(snapshot.player_left.linvel, true);
        body_left.set_angvel(snapshot.player_left.angvel, true);

        let body_right = self.bodies.get_mut(self.player_right.body_handle).unwrap();
        body_right.set_position(snapshot.player_right.position, true);
        body_right.set_linvel(snapshot.player_right.linvel, true);
        body_right.set_angvel(snapshot.player_right.angvel, true);

        let body_doodad = self.bodies.get_mut(self.doodad.body_handle).unwrap();
        body_doodad.set_position(snapshot.doodad.position, true);
        body_doodad.set_linvel(snapshot.doodad.linvel, true);
        body_doodad.set_angvel(snapshot.doodad.angvel, true);

        self.player_left.input = snapshot.player_left.input;
        self.player_right.input = snapshot.player_right.input;
        self.doodad.input = snapshot.doodad.input;
    }

    fn snapshot(&self) -> Self::SnapshotType {
        let body_left = self.bodies.get(self.player_left.body_handle).unwrap();
        let body_right = self.bodies.get(self.player_right.body_handle).unwrap();
        let body_doodad = self.bodies.get(self.doodad.body_handle).unwrap();
        DemoSnapshot {
            player_left: PlayerSnapshot {
                position: *body_left.position(),
                linvel: *body_left.linvel(),
                angvel: body_left.angvel(),
                input: self.player_left.input,
            },
            player_right: PlayerSnapshot {
                position: *body_right.position(),
                linvel: *body_right.linvel(),
                angvel: body_right.angvel(),
                input: self.player_right.input,
            },
            doodad: PlayerSnapshot {
                position: *body_doodad.position(),
                linvel: *body_doodad.linvel(),
                angvel: body_doodad.angvel(),
                input: self.doodad.input,
            },
        }
    }

    fn display_state(&self) -> Self::DisplayStateType {
        let body_left = self.bodies.get(self.player_left.body_handle).unwrap();
        let body_right = self.bodies.get(self.player_right.body_handle).unwrap();
        let body_doodad = self.bodies.get(self.doodad.body_handle).unwrap();
        DemoDisplayState {
            player_left_position: *body_left.position(),
            player_right_position: *body_right.position(),
            doodad_position: *body_doodad.position(),
        }
    }
}

impl Stepper for DemoWorld {
    fn step(&mut self) {
        for player in &mut [&mut self.player_left, &mut self.player_right] {
            let body = self.bodies.get_mut(player.body_handle).unwrap();
            body.apply_force(
                Vector2::new(
                    ((player.input.right as i32) - (player.input.left as i32)) as f32 * 4000.0,
                    0.0,
                ),
                true,
            );
            if player.input.jump {
                body.apply_impulse(Vector2::new(0.0, 4000.0), true);
                player.input.jump = false;
            }
        }
        self.pipeline.step(
            &GRAVITY,
            &IntegrationParameters {
                dt: TIMESTEP as f32,
                ..Default::default()
            },
            &mut self.island_manager,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.bodies,
            &mut self.colliders,
            &mut self.joints,
            &mut self.ccd_solver,
            &(),
            &(),
        );
    }
}

impl Command for DemoCommand {}

impl DisplayState for DemoDisplayState {
    fn from_interpolation(state1: &Self, state2: &Self, t: f64) -> Self {
        DemoDisplayState {
            player_left_position: state1
                .player_left_position
                .lerp_slerp(&state2.player_left_position, t as f32),
            player_right_position: state1
                .player_right_position
                .lerp_slerp(&state2.player_right_position, t as f32),
            doodad_position: state1
                .doodad_position
                .lerp_slerp(&state2.doodad_position, t as f32),
        }
    }
}

struct NetworkedServer {
    server: Server<DemoWorld>,
    network: MockNetwork,
}

struct NetworkedClient {
    client: Client<DemoWorld>,
    network: MockNetwork,
}

#[wasm_bindgen]
pub struct Demo {
    server: NetworkedServer,
    client_left: NetworkedClient,
    client_right: NetworkedClient,
}

#[wasm_bindgen]
pub enum CommsChannel {
    ToServerClocksync,
    ToServerCommand,
    ToClientClocksync,
    ToClientCommand,
    ToClientSnapshot,
}

#[wasm_bindgen]
impl Demo {
    #[wasm_bindgen(constructor)]
    pub fn new(seconds_since_startup: f64) -> Self {
        let config = Config {
            timestep_seconds: TIMESTEP,
            //snapshot_send_period: 0.3,
            //blend_latency: 0.5,
            tweening_method: TweeningMethod::MostRecentlyPassed,
            ..Default::default()
        };
        let (server_network, (client_left_network, client_right_network)) =
            MockNetwork::new_mock_network::<DemoWorld>();
        Self {
            server: NetworkedServer {
                server: Server::new(config.clone(), seconds_since_startup),
                network: server_network,
            },
            client_left: NetworkedClient {
                client: Client::new(config.clone()),
                network: client_left_network,
            },
            client_right: NetworkedClient {
                client: Client::new(config),
                network: client_right_network,
            },
        }
    }

    pub fn update(&mut self, delta_seconds: f64, seconds_since_startup: f64) {
        self.server.network.tick(delta_seconds);
        self.client_left.network.tick(delta_seconds);
        self.client_right.network.tick(delta_seconds);
        self.server.server.update(
            delta_seconds,
            seconds_since_startup,
            &mut self.server.network,
        );
        self.client_left.client.update(
            delta_seconds,
            seconds_since_startup,
            &mut self.client_left.network,
        );
        self.client_right.client.update(
            delta_seconds,
            seconds_since_startup,
            &mut self.client_right.network,
        );
    }

    fn client(&self, side: PlayerSide) -> &NetworkedClient {
        match side {
            PlayerSide::Left => &self.client_left,
            PlayerSide::Right => &self.client_right,
        }
    }

    fn client_mut(&mut self, side: PlayerSide) -> &mut NetworkedClient {
        match side {
            PlayerSide::Left => &mut self.client_left,
            PlayerSide::Right => &mut self.client_right,
        }
    }

    pub fn issue_command(&mut self, command: DemoCommand) {
        let client = self.client_mut(command.player_side);
        if let StageMut::Ready(mut ready_client) = client.client.stage_mut() {
            ready_client.issue_command(command, &mut client.network);
        }
    }

    pub fn get_server_commands(&mut self) -> Array {
        self.server
            .server
            .buffered_commands()
            .map(|(timestamp, commands)| {
                commands
                    .iter()
                    .map(move |command| JsValue::from(format!("{} {}", timestamp, command)))
            })
            .flatten()
            .collect()
    }

    pub fn get_client_commands(&mut self, side: PlayerSide) -> Array {
        match self.client(side).client.stage() {
            Stage::Ready(client) => client
                .buffered_commands()
                .map(|(timestamp, commands)| {
                    commands
                        .iter()
                        .map(move |command| JsValue::from(format!("{} {}", timestamp, command)))
                })
                .flatten()
                .collect(),
            _ => Array::new(),
        }
    }

    pub fn new_comms_activity_count(&mut self, side: PlayerSide, channel: CommsChannel) -> usize {
        match &mut self.client_mut(side).network.connections.get_mut(&0) {
            Some(connection) => match channel {
                CommsChannel::ToServerCommand => connection
                    .get_mut::<Timestamped<DemoCommand>>()
                    .new_outgoing_activity_count(),
                CommsChannel::ToServerClocksync => connection
                    .get_mut::<ClockSyncMessage>()
                    .new_outgoing_activity_count(),
                CommsChannel::ToClientCommand => connection
                    .get_mut::<Timestamped<DemoCommand>>()
                    .new_incoming_activity_count(),
                CommsChannel::ToClientClocksync => connection
                    .get_mut::<ClockSyncMessage>()
                    .new_incoming_activity_count(),
                CommsChannel::ToClientSnapshot => connection
                    .get_mut::<Timestamped<DemoSnapshot>>()
                    .new_incoming_activity_count(),
            },
            None => 0,
        }
    }

    pub fn set_network_delay(&mut self, side: PlayerSide, delay: f64) {
        self.client_mut(side).network.set_delay(delay);
    }

    pub fn connect(&mut self, side: PlayerSide) {
        self.client_mut(side).network.connect();
    }

    pub fn disconnect(&mut self, side: PlayerSide) {
        self.client_mut(side).network.disconnect();
    }

    pub fn client_timestamp(&self, side: PlayerSide) -> String {
        match self.client(side).client.stage() {
            Stage::SyncingClock(client) => format!(
                "Syncing {}/{}",
                client.sample_count(),
                client.samples_needed()
            ),
            Stage::SyncingInitialState(client) => {
                format!("{}", client.last_completed_timestamp())
            }
            Stage::Ready(client) => format!("{}", client.last_completed_timestamp()),
        }
    }

    pub fn client_display_state(&self, side: PlayerSide) -> Option<DemoDisplayState> {
        match self.client(side).client.stage() {
            Stage::Ready(client) => Some(client.display_state().display_state().clone()),
            _ => None,
        }
    }

    pub fn client_reconciliation_status(&self, side: PlayerSide) -> String {
        match self.client(side).client.stage() {
            Stage::Ready(client) => format!("{}", client.reconciliation_status()),
            _ => String::from("Inactive"),
        }
    }

    pub fn server_timestamp(&self) -> String {
        format!("{}", self.server.server.last_completed_timestamp())
    }

    pub fn server_display_state(&self) -> DemoDisplayState {
        self.server.server.display_state().inner().clone()
    }
}

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    tracing_wasm::set_as_global_default_with_config(
        tracing_wasm::WASMLayerConfigBuilder::new()
            .set_max_level(Level::INFO)
            .build(),
    );
    Ok(())
}
