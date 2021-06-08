# crystalorb-mock-network

Provides an implementation of `crystalorb::network_recource::NetworkResource` that works offline, backed using `VecDeque`, allowing [`CrystalOrb`](https://github.com/ErnWong/crystalorb) to work offline for demo and testing purposes.

## Usage

```rust
use crystalorb_mock_network::MockNetwork;
use crystalorb_demo::DemoWorld;
use crystalorb::{client::Client, server::Server, Config};

let (mut server_net, (mut client_1_net, mut client_2_net)) =
    MockNetwork::new_mock_network::<DemoWorld>();

let client_1 = Client::<DemoWorld>::new(Config::default());
let client_2 = Client::<DemoWorld>::new(Config::default());
let server = Server::<DemoWorld>::new(Config::default(), 0.0);

// You need to manually call connect.
client_1_net.connect();
client_2_net.connect();

// (You can also simulate a disconnection later on)
// client_1_net.disconnect();
// client_2_net.disconnect();

// (You can also set the latency of each connection)
// client_1_net.set_delay(number_of_seconds_as_f64);

// Later, in your update loop:
fn update_loop(
  delta_seconds: f64,
  seconds_since_startup: f64,
  client_1: &mut Client<DemoWorld>,
  client_2: &mut Client<DemoWorld>,
  server: &mut Server<DemoWorld>,
  client_1_net: &mut MockNetwork,
  client_2_net: &mut MockNetwork,
  server_net: &mut MockNetwork,
) {
    // You can then use these network resource instances to update
    // the `crystalorb` clients and server.
    client_1.update(delta_seconds, seconds_since_startup, client_1_net);
    client_2.update(delta_seconds, seconds_since_startup, client_2_net);
    server.update(delta_seconds, seconds_since_startup, server_net);

    // Call the `tick` method to simulate the flow of messages.
    // If you don't call `tick`, then no messages will be able to pass through the network.
    client_1_net.tick(delta_seconds);
    client_2_net.tick(delta_seconds);
    server_net.tick(delta_seconds);
}
```
