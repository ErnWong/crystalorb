# crystalorb-mock-network

Provides an implementation of `crystalorb::network_recource::NetworkResource` that works offline, backed using `VecDeque`, allowing [`CrystalOrb`](https://github.com/ErnWong/crystalorb) to work offline for demo and testing purposes.

## Usage

```rust
let (mut server_net, (mut client_1_net, mut client_2_net)) =
    MockNetwork::new_mock_network::<MyWorld>();

// You need to manually call connect.
client_1_net.connect();
client_2_net.connect();

// (You can also simulate a disconnection later on)
// client_1_net.disconnect();
// client_2_net.disconnect();

// (You can also set the latency of each connection)
// client_1_net.set_delay(number_of_seconds_as_f64);

// In your update loop:
loop {
    // You can then use these network resource instances to update
    // the `crystalorb` clients and server.
    client_1.update(delta_seconds, seconds_since_startup, &mut client_1_net);
    client_2.update(delta_seconds, seconds_since_startup, &mut client_2_net);
    server.update(delta_seconds, seconds_since_startup, &mut server_net);

    // Call the `tick` method to simulate the flow of messages.
    client_1_net.tick(delta_seconds);
    client_2_net.tick(delta_seconds);
    server_net.tick(delta_seconds);
}
```
