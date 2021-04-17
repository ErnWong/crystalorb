#[test]
#[ignore = "not yet implemented"]
fn when_server_lags_behind_clock_then_server_timeskips() {
    // GIVEN a server with pending commands issued for the simulating timestamp.
    // WHEN the clock advances too far before updating the server.
    // THEN the timestamp drift error still remains zero.
    // THEN all issued commands should have been applied.
    unimplemented!();
}

#[test]
#[ignore = "not yet implemented"]
fn when_client_lags_behind_clock_then_server_timeskips() {
    // GIVEN a client with pending commands issued for the simulating timestamp.
    // WHEN the clock advances too far before updating the client.
    // THEN the timestamp drift error still remains zero.
    // THEN all issued commands should have been applied.
    unimplemented!();
}
