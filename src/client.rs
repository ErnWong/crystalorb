//! This module houses the structures that are specific to the management of a CrystalOrb game
//! client.
//!
//! The [`Client`] structure is what you create, store, and update, analogous to the
//! [`Server`](crate::server::Server) structure. However, there isn't much functionality you can
//! directly access from a [`Client`] object. This is because most functionality depends on what
//! [`stage`] the [`Client`] is at, with different functionality provided at different stages. To
//! access other important and useful functionality to view and control your client, you can call
//! [`Client::stage`] and [`Client::stage_mut`] which will return some [`stage`] structure
//! containing the extra functionality.
//!
//! For the majority of the time, you would probably want to use [`stage::Ready`] in order
//! to perform the following useful operations:
//!
//! - [Issuing commands](stage::Ready::issue_command) that come from the player.
//!     ```
//!     use crystalorb::{Config, client::{Client, stage::StageMut}};
//!     use crystalorb_demo::{DemoWorld, DemoCommand, PlayerSide, PlayerCommand};
//!     use crystalorb_mock_network::MockNetwork;
//!
//!     // Using a mock network as an example...
//!     let (_, (mut network, _)) =
//!         MockNetwork::new_mock_network::<DemoWorld>();
//!
//!     let mut client = Client::<DemoWorld>::new(Config::new());
//!
//!     // ...later on in your update loop, in response to a player input...
//!
//!     if let StageMut::Ready(mut ready_client) = client.stage_mut() {
//!         let command = DemoCommand::new(PlayerSide::Left, PlayerCommand::Jump, true);
//!         ready_client.issue_command(command, &mut network);
//!     }
//!     ```
//! - [Getting the current display state](stage::Ready::display_state) to render on the screen.
//!     ```
//!     use crystalorb::{
//!         Config,
//!         client::{Client, stage::Stage},
//!         world::DisplayState,
//!     };
//!     use crystalorb_demo::{DemoWorld, DemoDisplayState};
//!
//!     fn render(display_state: &DemoDisplayState) {
//!         // [Your game rendering code]
//!         println!("{:?}", display_state);
//!     }
//!
//!     let client = Client::<DemoWorld>::new(Config::new());
//!
//!     // ...later on in your update loop...
//!
//!     if let Stage::Ready(ready_client) = client.stage() {
//!         render(ready_client.display_state().display_state());
//!     }
//!     ```

use crate::{clocksync::ClockSyncer, network_resource::NetworkResource, Config};
use tracing::warn;

pub mod stage;
use stage::{Stage, StageMut, StageOwned};

pub mod simulator;
use simulator::Simulator;

/// This is the top-level structure of CrystalOrb for your game client, analogous to the
/// [`Server`](crate::server::Server) for game servers. You create, store, and update this client
/// instance to run your game on the client side.
#[derive(Debug)]
pub struct Client<SimulatorType: Simulator> {
    config: Config,
    stage: StageOwned<SimulatorType>,
}

impl<SimulatorType: Simulator> Client<SimulatorType> {
    /// Constructs a new [`Client`].
    ///
    /// # Examples
    ///
    /// Using the default configuration parameters:
    ///
    /// ```
    /// use crystalorb::{Config, client::Client};
    /// use crystalorb_demo::DemoWorld;
    ///
    /// let client = Client::<DemoWorld>::new(Config::new());
    /// ```
    ///
    /// Overriding some configuration parameter:
    ///
    /// ```
    /// use crystalorb::{Config, client::Client};
    /// use crystalorb_demo::DemoWorld;
    ///
    /// let client = Client::<DemoWorld>::new(Config {
    ///     lag_compensation_latency: 0.5,
    ///     ..Config::new()
    /// });
    /// ```
    pub fn new(config: Config) -> Self {
        Self {
            config: config.clone(),
            stage: StageOwned::SyncingClock(ClockSyncer::new(config)),
        }
    }

    /// Perform the next update for the current rendering frame. You would typically call this in
    /// your game engine's update loop of some kind.
    ///
    /// # Examples
    ///
    /// Here is how one might call [`Client::update`] outside of any game engine:
    ///
    /// ```
    /// use crystalorb::{Config, client::Client};
    /// use crystalorb_demo::DemoWorld;
    /// use crystalorb_mock_network::MockNetwork;
    /// use std::time::Instant;
    ///
    /// // Using a mock network as an example...
    /// let (_, (mut network, _)) =
    ///    MockNetwork::new_mock_network::<DemoWorld>();
    ///
    /// let mut client = Client::<DemoWorld>::new(Config::new());
    /// let startup_time = Instant::now();
    /// let mut previous_time = Instant::now();
    ///
    /// loop {
    ///     let current_time = Instant::now();
    ///     let delta_seconds = current_time.duration_since(previous_time).as_secs_f64();
    ///     let seconds_since_startup = current_time.duration_since(startup_time).as_secs_f64();
    ///
    ///     client.update(delta_seconds, seconds_since_startup, &mut network);
    ///
    ///     // ...Other update code omitted...
    ///     # break;
    /// }
    /// ```
    pub fn update<NetworkResourceType: NetworkResource>(
        &mut self,
        delta_seconds: f64,
        seconds_since_startup: f64,
        net: &mut NetworkResourceType,
    ) {
        let positive_delta_seconds = delta_seconds.max(0.0);
        #[allow(clippy::float_cmp)]
        if delta_seconds != positive_delta_seconds {
            warn!(
                "Attempted to update client with a negative delta_seconds {}. Clamping it to zero.",
                delta_seconds
            );
        }

        self.stage
            .update(delta_seconds, seconds_since_startup, &self.config, net);
    }

    /// Get the current stage of the [`Client`], which provides access to extra functionality
    /// depending on what stage it is currently in. See [`Client::stage_mut`], which provides
    /// functionality to mutate the client in some way.
    ///
    /// # Example
    ///
    /// To check on the client's progress at connecting with the server, you can check the number
    /// of clocksync samples during the client's [`SyncingClock`](Stage::SyncingClock)
    /// stage:
    ///
    /// ```
    /// use crystalorb::{Config, client::{Client, stage::Stage}};
    /// use crystalorb_demo::DemoWorld;
    ///
    /// let client = Client::<DemoWorld>::new(Config::new());
    ///
    /// // ...Later on...
    ///
    /// if let Stage::SyncingClock(syncing_clock_client) = client.stage() {
    ///     println!(
    ///         "Connection progress: {}%",
    ///         syncing_clock_client.sample_count() as f64
    ///             / syncing_clock_client.samples_needed() as f64 * 100.0
    ///     );
    /// }
    /// ```
    pub fn stage(&self) -> Stage<SimulatorType> {
        Stage::from(&self.stage)
    }

    /// Get the current stage f the [`Client`], which provides access to extra functionality
    /// depending on what stage it is currently in. For shareable immutable version, see
    /// [`Client::stage`].
    ///
    /// # Example
    ///
    /// To issue a command, you'll need to access the client in its [`Ready`](Stage::Ready)
    /// stage:
    ///
    /// ```
    /// use crystalorb::{Config, client::{Client, stage::StageMut}};
    /// use crystalorb_demo::{DemoWorld, DemoCommand, PlayerSide, PlayerCommand};
    /// use crystalorb_mock_network::MockNetwork;
    ///
    /// // Using a mock network as an example...
    /// let (_, (mut network, _)) =
    ///    MockNetwork::new_mock_network::<DemoWorld>();
    ///
    /// let mut client = Client::<DemoWorld>::new(Config::new());
    ///
    /// // ...Later on...
    ///
    /// if let StageMut::Ready(mut ready_client) = client.stage_mut() {
    ///     let command = DemoCommand::new(PlayerSide::Left, PlayerCommand::Jump, true);
    ///     ready_client.issue_command(command, &mut network);
    /// }
    /// ```
    pub fn stage_mut(&mut self) -> StageMut<SimulatorType> {
        StageMut::from(&mut self.stage)
    }
}
