#![feature(generic_associated_types)]

use crystalorb::{
    client::ClientState, timestamp::Timestamp, world::Tweened, Config, TweeningMethod,
};
use pretty_assertions::assert_eq;
use test_env_log::test;

mod common;

use common::{MockClientServer, MockCommand, MockWorld, TIMESTEP_SECONDS};

#[test]
fn while_all_commands_originate_from_single_client_then_that_client_should_match_server_exactly() {
    for frames_per_update in &[1.0, 0.5, 1.0 / 3.0, 1.5, 2.0, 3.0, 4.0, 6.0] {
        // GIVEN a server and multiple clients in a perfect network.
        const FRAMES_TO_LAG_BEHIND: i32 = 12;
        assert_eq!(
            (FRAMES_TO_LAG_BEHIND as f64 / frames_per_update).fract(),
            0.0,
            "lag needs to be multiple of frames_per_update so the display states line up.",
        );
        let mut mock_client_server = MockClientServer::new(Config {
            lag_compensation_latency: FRAMES_TO_LAG_BEHIND as f64 * TIMESTEP_SECONDS,
            interpolation_latency: 0.2,
            timestep_seconds: TIMESTEP_SECONDS,
            clock_sync_needed_sample_count: 8,
            clock_sync_request_period: 0.0,
            clock_sync_assumed_outlier_rate: 0.2,
            max_tolerable_clock_deviation: 0.1,
            snapshot_send_period: 0.1,
            update_delta_seconds_max: 0.5,
            timestamp_skip_threshold_seconds: 1.0,
            fastforward_max_per_step: 10,
            tweening_method: TweeningMethod::MostRecentlyPassed,
        });
        mock_client_server.client_1_net.connect();
        mock_client_server.client_2_net.connect();

        // GIVEN that the clients are ready.
        mock_client_server.update_until_clients_ready(TIMESTEP_SECONDS * frames_per_update);

        // WHEN a single chosen client issue commands.
        let mut commands = [
            vec![0, 1, 2],
            vec![3, 4],
            vec![5],
            vec![6, 7],
            vec![],
            vec![8, 9, 10, 11, 12],
        ];
        let start_timestamp = match mock_client_server.client_1.state() {
            ClientState::Ready(client) => client.simulating_timestamp(),
            _ => unreachable!(),
        };
        let target_timestamp =
            start_timestamp + (commands.len() as i16).max(*frames_per_update as i16);
        let mut client_state_history: Vec<Tweened<MockWorld>> = Vec::new();
        let mut server_state_history: Vec<Tweened<MockWorld>> = Vec::new();

        while mock_client_server.server.display_state().timestamp() < target_timestamp {
            let current_client_timestamp = match mock_client_server.client_1.state() {
                ClientState::Ready(client) => {
                    Timestamp::default() + client.display_state().float_timestamp() as i16
                }
                _ => unreachable!(),
            };
            let update_client = current_client_timestamp < target_timestamp;

            if update_client {
                match mock_client_server.client_1.state_mut() {
                    ClientState::Ready(client) => {
                        let current_index = (i16::from(current_client_timestamp - start_timestamp))
                            .clamp(0, commands.len() as i16 - 1)
                            as usize;
                        for commands_for_single_timestep in commands[0..=current_index].iter_mut() {
                            for command in commands_for_single_timestep.drain(..) {
                                client.issue_command(
                                    MockCommand(command),
                                    &mut mock_client_server.client_1_net,
                                );
                            }
                        }
                    }
                    _ => unreachable!(),
                }
            }

            mock_client_server.update(TIMESTEP_SECONDS * frames_per_update);
            server_state_history.push(mock_client_server.server.display_state().into());

            if update_client {
                match mock_client_server.client_1.state_mut() {
                    ClientState::Ready(client) => {
                        client_state_history.push(client.display_state().clone())
                    }
                    _ => unreachable!(),
                }
            }
        }

        // THEN the recorded server states should perfectly match the chosen client's states.
        assert_eq!(
            server_state_history[server_state_history.len() - client_state_history.len()..],
            client_state_history[..],
            "frames per update: {}",
            frames_per_update
        );
    }
}

#[test]
fn while_no_commands_are_issued_then_all_clients_should_match_server_exactly() {
    for frames_per_update in &[1.0, 0.5, 1.0 / 3.0, 1.5, 2.0, 3.0, 4.0, 6.0] {
        // GIVEN a server and multiple clients in a perfect network.
        const FRAMES_TO_LAG_BEHIND: i32 = 12;
        assert_eq!(
            (FRAMES_TO_LAG_BEHIND as f64 / frames_per_update).fract(),
            0.0,
            "lag needs to be multiple of frames_per_update so the display states line up.",
        );
        let mut mock_client_server = MockClientServer::new(Config {
            lag_compensation_latency: FRAMES_TO_LAG_BEHIND as f64 * TIMESTEP_SECONDS,
            interpolation_latency: 0.2,
            timestep_seconds: TIMESTEP_SECONDS,
            clock_sync_needed_sample_count: 8,
            clock_sync_request_period: 0.0,
            clock_sync_assumed_outlier_rate: 0.2,
            max_tolerable_clock_deviation: 0.1,
            snapshot_send_period: 0.1,
            update_delta_seconds_max: 0.5,
            timestamp_skip_threshold_seconds: 1.0,
            fastforward_max_per_step: 10,
            tweening_method: TweeningMethod::MostRecentlyPassed,
        });
        mock_client_server.client_1_net.connect();
        mock_client_server.client_2_net.connect();

        mock_client_server
            .server
            .issue_command(MockCommand(123), &mut mock_client_server.server_net);

        // WHEN no commands are issued.
        mock_client_server.update_until_clients_ready(TIMESTEP_SECONDS * frames_per_update);

        // WHEN a single chosen client issue commands.
        let start_timestamp = match mock_client_server.client_1.state() {
            ClientState::Ready(client) => client.simulating_timestamp(),
            _ => unreachable!(),
        };
        let target_timestamp = start_timestamp + 100;
        let mut client_1_state_history: Vec<Tweened<MockWorld>> = Vec::new();
        let mut client_2_state_history: Vec<Tweened<MockWorld>> = Vec::new();
        let mut server_state_history: Vec<Tweened<MockWorld>> = Vec::new();

        while mock_client_server.server.display_state().timestamp() < target_timestamp {
            let current_client_timestamp = match mock_client_server.client_1.state() {
                ClientState::Ready(client) => {
                    Timestamp::default() + client.display_state().float_timestamp() as i16
                }
                _ => unreachable!(),
            };
            let update_client = current_client_timestamp < target_timestamp;

            mock_client_server.update(TIMESTEP_SECONDS * frames_per_update);
            server_state_history.push(mock_client_server.server.display_state().into());

            if update_client {
                match mock_client_server.client_1.state_mut() {
                    ClientState::Ready(client) => {
                        client_1_state_history.push(client.display_state().clone())
                    }
                    _ => unreachable!(),
                }
                match mock_client_server.client_2.state_mut() {
                    ClientState::Ready(client) => {
                        client_2_state_history.push(client.display_state().clone())
                    }
                    _ => unreachable!(),
                }
            }
        }

        // THEN the recorded server states should perfectly match every client's states.
        assert_eq!(
            server_state_history[server_state_history.len() - client_1_state_history.len()..],
            client_1_state_history[..],
            "client_1, frames per update: {}",
            frames_per_update
        );
        assert_eq!(
            server_state_history[server_state_history.len() - client_2_state_history.len()..],
            client_2_state_history[..],
            "client_2, frames per update: {}",
            frames_per_update
        );
    }
}
