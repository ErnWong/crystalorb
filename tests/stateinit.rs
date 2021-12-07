#![feature(generic_associated_types)]

use crystalorb::{client::stage::Stage, Config, TweeningMethod};
use itertools::iproduct;
use pretty_assertions::assert_eq;
use test_log::test;

mod common;

use common::{MockClientServer, MockCommand};

#[test]
fn when_client_becomes_ready_state_should_already_be_initialised() {
    const TIMESTEP_SECONDS: f64 = 1.0 / 60.0;

    for frames_per_update in &[1.0, 0.5, 0.3, 2.0, 1.5, 10.0] {
        // GIVEN a server and multiple clients in a perfect network.
        const FRAMES_TO_LAG_BEHIND: i32 = 10;
        let mut mock_client_server = MockClientServer::new(Config {
            lag_compensation_latency: FRAMES_TO_LAG_BEHIND as f64 * TIMESTEP_SECONDS,
            blend_latency: 0.2,
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
            match client.stage() {
                Stage::Ready(ready_client) => {
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

/// This test is to replicate a bug where, after the client slept too long, had the
/// last_queued_snapshot wrapped to the other side, making all new server snapshots appear to be
/// "before" this very old snapshot. This is one of the many examples of why Timestamp should've
/// just used an i64 rather than a fancy pantsy Wrapped<i16>.
#[test]
fn when_client_doesnt_receive_snapshot_for_a_while_then_new_snapshot_is_still_accepted() {
    const TIMESTEP_SECONDS: f64 = 1.0 / 60.0;

    for (long_delay_seconds, should_disconnect) in iproduct!(
        &[
            -60.0,
            TIMESTEP_SECONDS * 14.0f64.exp2(),
            TIMESTEP_SECONDS * 14.5f64.exp2(),
            TIMESTEP_SECONDS * 15.0f64.exp2(),
            TIMESTEP_SECONDS * 15.5f64.exp2(),
        ],
        &[false, true]
    ) {
        // GIVEN a server and multiple clients in a perfect network.
        const FRAMES_TO_LAG_BEHIND: i32 = 10;
        let mut mock_client_server = MockClientServer::new(Config {
            lag_compensation_latency: FRAMES_TO_LAG_BEHIND as f64 * TIMESTEP_SECONDS,
            blend_latency: 0.2,
            timestep_seconds: TIMESTEP_SECONDS,
            clock_sync_needed_sample_count: 8,
            clock_sync_request_period: 0.0,
            clock_sync_assumed_outlier_rate: 0.2,
            max_tolerable_clock_deviation: 0.1,
            snapshot_send_period: 0.0,
            update_delta_seconds_max: 0.25,
            timestamp_skip_threshold_seconds: 1.0,
            fastforward_max_per_step: 10,
            tweening_method: TweeningMethod::MostRecentlyPassed,
        });
        mock_client_server.client_1_net.connect();
        mock_client_server.client_2_net.connect();

        // GIVEN that the clients are ready.
        mock_client_server.update_until_clients_ready(TIMESTEP_SECONDS);

        // GIVEN that a client does not hear from the server for a long time.
        if *should_disconnect {
            mock_client_server.client_1_net.disconnect();
        }
        let last_accepted_snapshot_timestamp_before_disconnect =
            match mock_client_server.client_1.stage() {
                Stage::Ready(client) => client.last_queued_snapshot_timestamp().clone(),
                _ => unreachable!(),
            };
        let last_received_snapshot_timestamp_before_disconnect =
            match mock_client_server.client_1.stage() {
                Stage::Ready(client) => client.last_received_snapshot_timestamp().clone(),
                _ => unreachable!(),
            };
        mock_client_server.update(*long_delay_seconds);

        // GIVEN that the server has some new state changes that the client doesn't know.
        if !*should_disconnect {
            mock_client_server.client_1_net.disconnect();
        }
        mock_client_server
            .server
            .issue_command(MockCommand(1234), &mut mock_client_server.server_net);
        let timestamp_for_new_command = mock_client_server
            .server
            .estimated_client_simulating_timestamp();
        mock_client_server.client_1_net.connect();

        // WHEN that client finally hears back from the server.
        let mut last_received_snapshot_timestamp_after_disconnect =
            last_received_snapshot_timestamp_before_disconnect;
        while last_received_snapshot_timestamp_after_disconnect != Some(timestamp_for_new_command) {
            mock_client_server.update(TIMESTEP_SECONDS);
            last_received_snapshot_timestamp_after_disconnect =
                match mock_client_server.client_1.stage() {
                    Stage::Ready(client) => client.last_received_snapshot_timestamp().clone(),
                    _ => unreachable!(),
                };
        }

        // THEN that client should accept the server's new snapshots.
        let last_accepted_snapshot_timestamp_after_disconnect =
            match mock_client_server.client_1.stage() {
                Stage::Ready(client) => client.last_queued_snapshot_timestamp().clone(),
                _ => unreachable!(),
            };
        assert_eq!(
            last_accepted_snapshot_timestamp_after_disconnect,
            last_received_snapshot_timestamp_after_disconnect,
            "Condition: Snapshot should not be rejected after {} delay",
            long_delay_seconds
        );
        assert_ne!(
            last_accepted_snapshot_timestamp_before_disconnect,
            last_accepted_snapshot_timestamp_after_disconnect,
            "Condition: Snapshot should be renewed after {} delay",
            long_delay_seconds
        );

        // THEN that client state should eventually reflect the server state change.
        for _ in 0..100 {
            mock_client_server.update(TIMESTEP_SECONDS)
        }
        let client_1_stage = mock_client_server.client_1.stage();
        let display_state = match &client_1_stage {
            Stage::Ready(client) => client.display_state(),
            _ => unreachable!(),
        };
        assert_eq!(
            display_state.dx, 1234,
            "Condition: State change should be reflected after {} delay",
            long_delay_seconds
        );
    }
}
