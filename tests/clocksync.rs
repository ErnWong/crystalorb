#![feature(generic_associated_types)]

use crystalorb::{client::ClientStage, Config, TweeningMethod};
use test_env_log::test;

mod common;

use common::MockClientServer;

#[test]
fn when_server_and_client_clocks_desync_then_client_should_resync_quickly() {
    const UPDATE_COUNT: usize = 200;
    const TIMESTEP_SECONDS: f64 = 1.0 / 64.0;

    for desync_seconds in &[
        0.0f64,
        0.5f64,
        -0.5f64,
        -1.0f64,
        -100.0f64,
        -1000.0f64,
        -10000.0f64,
    ] {
        // GIVEN a server and client in a perfect network.
        let mut mock_client_server = MockClientServer::new(Config {
            lag_compensation_latency: TIMESTEP_SECONDS * 16.0,
            blend_latency: 0.2,
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

        // GIVEN that the client is ready and synced up.
        mock_client_server.update_until_clients_ready(TIMESTEP_SECONDS);
        match mock_client_server.client_1.stage() {
            ClientStage::Ready(client) => {
                assert_eq!(
                    client.last_completed_timestamp(),
                    mock_client_server
                        .server
                        .estimated_client_last_completed_timestamp()
                        // Note: + 1 since client is overshooting.
                        + 1,
                    "Precondition: clocks are initially synced up"
                );
            }
            _ => unreachable!(),
        }

        // WHEN the client and server clocks are desynchronized.
        mock_client_server.client_1_clock_offset = *desync_seconds;

        // THEN the client should quickly offset its own clock to agree with the server.
        for _ in 0..UPDATE_COUNT {
            mock_client_server.update(TIMESTEP_SECONDS);
        }
        match mock_client_server.client_1.stage() {
            ClientStage::Ready(client) => {
                assert_eq!(
                    client.last_completed_timestamp(),
                    mock_client_server
                        .server
                        .estimated_client_last_completed_timestamp()
                        // Note: + 1 since client is overshooting.
                        + 1,
                    "Condition: Client synced up after {} updates (desync by {})",
                    UPDATE_COUNT,
                    desync_seconds
                );
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn when_client_connects_then_client_calculates_correct_initial_clock_offset() {
    const TIMESTEP_SECONDS: f64 = 1.0 / 64.0;

    for desync_seconds in &[
        0.0f64,
        0.5f64,
        1.0f64,
        100.0f64,
        1000.0f64,
        10000.0f64,
        -0.5f64,
        -1.0f64,
        -100.0f64,
        -1000.0f64,
        -10000.0f64,
    ] {
        // GIVEN a server and client in a perfect network.
        let mut mock_client_server = MockClientServer::new(Config {
            lag_compensation_latency: TIMESTEP_SECONDS * 16.0,
            blend_latency: 0.2,
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

        // GIVEN that the client and server clocks initially disagree.
        mock_client_server.client_1_clock_offset = *desync_seconds;

        // WHEN the client initially connects.
        mock_client_server.update_until_clients_ready(TIMESTEP_SECONDS);

        // THEN the client should accurately offset its own clock to agree with the server.
        match mock_client_server.client_1.stage() {
            ClientStage::Ready(client) => {
                assert_eq!(
                    client.last_completed_timestamp(),
                    mock_client_server
                        .server
                        .estimated_client_last_completed_timestamp()
                        // Note: + 1 since client is overshooting.
                        + 1,
                    "Precondition: clocks are initially synced up"
                );
            }
            _ => unreachable!(),
        }
    }
}
