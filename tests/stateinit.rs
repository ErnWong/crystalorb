#![feature(generic_associated_types)]

use crystalorb::{client::ClientState, Config, TweeningMethod};
use pretty_assertions::assert_eq;
use test_env_log::test;

mod common;

use common::{MockClientServer, MockCommand, TIMESTEP_SECONDS};

#[test]
fn when_client_becomes_ready_state_should_already_be_initialised() {
    for frames_per_update in &[1.0, 0.5, 0.3, 2.0, 1.5, 10.0] {
        // GIVEN a server and multiple clients in a perfect network.
        const FRAMES_TO_LAG_BEHIND: i32 = 10;
        let mut mock_client_server = MockClientServer::new(Config {
            lag_compensation_latency: FRAMES_TO_LAG_BEHIND as f64 * TIMESTEP_SECONDS,
            interpolation_latency: 0.2,
            timestep_seconds: TIMESTEP_SECONDS,
            clock_sync_needed_sample_count: 8,
            clock_sync_request_period: 0.0,
            clock_sync_assumed_outlier_rate: 0.2,
            max_tolerable_clock_deviation: 0.1,
            snapshot_send_period: 0.1,
            update_delta_seconds_max: 0.25,
            timestamp_skip_threshold_seconds: 1.0,
            fastforward_max_per_step: 10,
            tweening_method: TweeningMethod::MostRecentlyPassed,
        });

        // GIVEN the server has some specific non-default initial state.
        mock_client_server
            .server
            .issue_command(MockCommand(1234), &mut mock_client_server.server_net);
        mock_client_server.update(1.0);

        // GIVEN the clients connect after having this initial command issued.
        mock_client_server.client_1_net.connect();
        mock_client_server.client_2_net.connect();

        // WHEN that the clients are ready.
        mock_client_server.update_until_clients_ready(TIMESTEP_SECONDS * frames_per_update);

        // THEN all clients' states are initialised to that server's state.
        for client in &[mock_client_server.client_1, mock_client_server.client_2] {
            match client.state() {
                ClientState::Ready(ready_client) => {
                    assert_eq!(
                        ready_client.display_state().dx,
                        1234,
                        "frames_per_update: {}",
                        frames_per_update
                    );
                    assert_eq!(
                        ready_client.display_state().initial_empty_ticks,
                        mock_client_server
                            .server
                            .display_state()
                            .initial_empty_ticks,
                        "frames_per_update: {}",
                        frames_per_update
                    );
                }
                _ => unreachable!("Client should be ready by now"),
            }
        }
    }
}
