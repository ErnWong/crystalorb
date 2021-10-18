# crystalorb-bevy-networking-turbulence

Provides a wrapper implementation of `crystalorb::network_recource::NetworkResource` that allows the `bevy_networking_turbulence` plugin to be used with [`CrystalOrb`](https://github.com/ErnWong/crystalorb).

For your convenience, a bevy plugin is also provided that performs the necessary setup (such as registering the required message channels in the `bevy_networking_turbulence` plugin, and registering the `Client`/`Server` resources)

## Pitfalls

At the moment, you still need to put all your physics and real-time game logic into the `CrystalOrb::world::World`, but, ideally, it would be great to put them into the bevy ECS instead (since the ECS is probably the reason why you want to use bevy in the first place). In the future, it might be possible to implement `CrystalOrb::world::World` for a bevy app (contributions welcome!!) so that we can run bevy inside CrystalOrb inside bevy (bevyception? That'll be a cool name if it's not already taken).

## Usage

- Add the `CrystalOrbClientPlugin` bevy [plugin](https://bevyengine.org/learn/book/getting-started/plugins/) into your client bevy app.
- Add the `CrystalOrbServerPlugin` bevy [plugin](https://bevyengine.org/learn/book/getting-started/plugins/) into your server bevy app.
- Access the `CrystalOrb::Client` on your client app as a bevy [resource](https://bevyengine.org/learn/book/getting-started/resources/).
- Access the `CrystalOrb::Server` on your server app as a bevy [resource](https://bevyengine.org/learn/book/getting-started/resources/).

Heres an example client:

```rust
use bevy::prelude::*;
use crystalorb_demo::{DemoWorld, DemoCommand, PlayerSide, PlayerCommand};
use crystalorb_bevy_networking_turbulence::{
  WrappedNetworkResource,
  CrystalOrbClientPlugin,
  crystalorb::{
    Config,
    client::{
      Client,
      stage::StageMut as ClientStageMut,
    },
  },
  CommandChannelSettings,
  bevy_networking_turbulence::{
    NetworkResource,
    MessageChannelSettings,
    MessageChannelMode,
    ReliableChannelSettings
  },
};
use std::time::Duration;

#[derive(Default)]
struct PlayerInputState {
  jump: bool,
}

fn player_input(
  mut state: Local<PlayerInputState>,
  input: Res<Input<KeyCode>>,
  mut client: ResMut<Client<DemoWorld>>,
  mut net: ResMut<NetworkResource>,
) {
  if let ClientStageMut::Ready(mut ready_client) = client.stage_mut() {
    let jump = input.pressed(KeyCode::Up);

    if jump != state.jump {
      ready_client.issue_command(
        DemoCommand::new(
          PlayerSide::Left,
          PlayerCommand::Jump,
          jump
        ),
        &mut WrappedNetworkResource(&mut *net),
      );

    }

    state.jump = jump;
  }
}

fn main() {
  App::build()
    // You can optionally override some message channel settings
    // There is `CommandChannelSettings`, `SnapshotChannelSettings`, and `ClockSyncChannelSettings`
    // Make sure you apply the same settings for both client and server.
    .insert_resource(CommandChannelSettings(
        MessageChannelSettings {
            channel: 0,
            channel_mode: MessageChannelMode::Compressed {
                reliability_settings: ReliableChannelSettings {
                    bandwidth: 4096,
                    recv_window_size: 1024,
                    send_window_size: 1024,
                    burst_bandwidth: 1024,
                    init_send: 512,
                    wakeup_time: Duration::from_millis(100),
                    initial_rtt: Duration::from_millis(200),
                    max_rtt: Duration::from_secs(2),
                    rtt_update_factor: 0.1,
                    rtt_resend_factor: 1.5,
                },
                max_chunk_len: 1024,
            },
            message_buffer_size: 64,
            packet_buffer_size: 64,
        }
    ))
    .add_plugins(DefaultPlugins)
    .add_plugin(CrystalOrbClientPlugin::<DemoWorld>::new(Config::default()))
    .add_system(player_input.system())
    .run();
}
```

Here's an example server:

```rust
use bevy::prelude::*;
use crystalorb_demo::DemoWorld;
use crystalorb_bevy_networking_turbulence::{
  CrystalOrbServerPlugin,
  crystalorb::Config,
  CommandChannelSettings,
  bevy_networking_turbulence::{
    MessageChannelSettings,
    MessageChannelMode,
    ReliableChannelSettings
  },
};
use std::time::Duration;

fn main() {
  App::build()
    // You can optionally override some message channel settings
    // There is `CommandChannelSettings`, `SnapshotChannelSettings`, and `ClockSyncChannelSettings`
    // Make sure you apply the same settings for both client and server.
    .insert_resource(CommandChannelSettings(
        MessageChannelSettings {
            channel: 0,
            channel_mode: MessageChannelMode::Compressed {
                reliability_settings: ReliableChannelSettings {
                    bandwidth: 4096,
                    recv_window_size: 1024,
                    send_window_size: 1024,
                    burst_bandwidth: 1024,
                    init_send: 512,
                    wakeup_time: Duration::from_millis(100),
                    initial_rtt: Duration::from_millis(200),
                    max_rtt: Duration::from_secs(2),
                    rtt_update_factor: 0.1,
                    rtt_resend_factor: 1.5,
                },
                max_chunk_len: 1024,
            },
            message_buffer_size: 64,
            packet_buffer_size: 64,
        }
    ))
    .add_plugins(DefaultPlugins)
    .add_plugin(CrystalOrbServerPlugin::<DemoWorld>::new(Config::default()))
    .run();
}
```
