use crate::{
    network_resource::NetworkResource,
    timestamp::{Timestamp, Timestamped},
    world::{Tweened, World},
};
use std::{
    borrow::{Borrow, BorrowMut},
    marker::PhantomData,
};

use super::{super::ReconciliationStatus, ActiveClient};

/// The client interface once the client is in the [ready
/// stage](super#stage-3---ready-stage).
#[derive(Debug)]
pub struct Ready<WorldType, NetworkResourceType, ActiveClientRefType>(
    ActiveClientRefType,
    PhantomData<WorldType>,
    PhantomData<NetworkResourceType>,
)
where
    ActiveClientRefType: Borrow<ActiveClient<WorldType, NetworkResourceType>>,
    WorldType: World,
    NetworkResourceType: NetworkResource;

impl<'a, WorldType: World, NetworkResourceType: NetworkResource>
    From<&'a ActiveClient<WorldType, NetworkResourceType>>
    for Ready<WorldType, NetworkResourceType, &'a ActiveClient<WorldType, NetworkResourceType>>
{
    fn from(active_client: &'a ActiveClient<WorldType, NetworkResourceType>) -> Self {
        Self(active_client, PhantomData, PhantomData)
    }
}

impl<'a, WorldType: World, NetworkResourceType: NetworkResource>
    From<&'a mut ActiveClient<WorldType, NetworkResourceType>>
    for Ready<WorldType, NetworkResourceType, &'a mut ActiveClient<WorldType, NetworkResourceType>>
{
    fn from(active_client: &'a mut ActiveClient<WorldType, NetworkResourceType>) -> Self {
        Self(active_client, PhantomData, PhantomData)
    }
}

impl<WorldType, NetworkResourceType, ActiveClientRefType>
    Ready<WorldType, NetworkResourceType, ActiveClientRefType>
where
    ActiveClientRefType: Borrow<ActiveClient<WorldType, NetworkResourceType>>,
    WorldType: World,
    NetworkResourceType: NetworkResource,
{
    /// The timestamp of the most recent frame that has completed its simulation.
    /// This is typically one less than [`Ready::simulating_timestamp`].
    pub fn last_completed_timestamp(&self) -> Timestamp {
        self.0.borrow().last_completed_timestamp()
    }

    /// The timestamp of the frame that is *in the process* of being simulated.
    /// This is typically one more than [`Ready::simulating_timestamp`].
    ///
    /// This is also the timestamp that gets attached to the command when you call
    /// [`Ready::issue_command`].
    pub fn simulating_timestamp(&self) -> Timestamp {
        self.0.borrow().simulating_timestamp()
    }

    /// A number that is used to identify the client among all the clients connected to the server.
    /// This number may be useful, for example, to identify which piece of world state belongs to
    /// which player.
    pub fn client_id(&self) -> NetworkResourceType::ConnectionHandleType {
        self.0
            .borrow()
            .clocksyncer
            .client_id()
            .expect("Client should be connected by the time it is ready")
    }

    /// Iterate through the commands that are being kept around. This is intended to be for
    /// diagnostic purposes.
    pub fn buffered_commands(
        &self,
    ) -> impl Iterator<Item = (Timestamp, &Vec<WorldType::CommandType>)> {
        self.0
            .borrow()
            .timekeeping_simulations
            .base_command_buffer
            .iter()
    }

    /// Get the current display state that can be used to render the client's screen.
    pub fn display_state(&self) -> &Tweened<WorldType::DisplayStateType> {
        &self
            .0
            .borrow()
            .timekeeping_simulations
            .display_state
            .as_ref()
            .expect("Client should be initialised")
    }

    /// The timestamp used to test whether the next snapshot to be received is newer or older, and
    /// therefore should be discarded or queued.
    ///
    /// This value gets updated if it gets too old, even if there hasn't been any newer snapshot
    /// received. This is because we need to compare newly-received snapshots with this value, but
    /// we can't compare Timestamps if they are outside the
    /// [comparable range](Timestamp::comparable_range_with_midpoint).
    ///
    /// None if no snapshots have been received yet.
    pub fn last_queued_snapshot_timestamp(&self) -> &Option<Timestamp> {
        &self
            .0
            .borrow()
            .timekeeping_simulations
            .last_queued_snapshot_timestamp
    }

    /// The timestamp of the most recently received snapshot, regardless of whether it got queued
    /// or discarded.
    ///
    /// Unlike [`Ready::last_queued_snapshot_timestamp`], this does not get updated when it
    /// becomes too old to be compared with the current timestamp. This is primarily used for
    /// diagnostic purposes.
    ///
    /// None if no spashots have been received yet.
    pub fn last_received_snapshot_timestamp(&self) -> &Option<Timestamp> {
        &self
            .0
            .borrow()
            .timekeeping_simulations
            .last_received_snapshot_timestamp
    }

    /// Useful diagnostic to see what stage of the server reconciliation process that the
    /// client is currently at. For more information, refer to [`ReconciliationStatus`].
    pub fn reconciliation_status(&self) -> ReconciliationStatus {
        self.0
            .borrow()
            .timekeeping_simulations
            .infer_current_reconciliation_status()
    }
}

impl<WorldType, NetworkResourceType, ActiveClientRefType>
    Ready<WorldType, NetworkResourceType, ActiveClientRefType>
where
    ActiveClientRefType: Borrow<ActiveClient<WorldType, NetworkResourceType>>
        + BorrowMut<ActiveClient<WorldType, NetworkResourceType>>,
    WorldType: World,
    NetworkResourceType: NetworkResource,
{
    /// Issue a command from this client's player to the world. The command will be scheduled
    /// to the current simulating timestamp (the previously completed timestamp + 1).
    pub fn issue_command(
        &mut self,
        command: WorldType::CommandType,
        net: &mut NetworkResourceType,
    ) {
        let timestamped_command = Timestamped::new(command, self.simulating_timestamp());
        self.0
            .borrow_mut()
            .timekeeping_simulations
            .receive_command(&timestamped_command);
        net.broadcast_message(timestamped_command);
    }
}
