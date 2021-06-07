use crate::{
    client::{simulator::Simulator, stage::Active},
    network_resource::NetworkResource,
    timestamp::Timestamp,
    world::{Tweened, World},
};
use std::{
    borrow::{Borrow, BorrowMut},
    marker::PhantomData,
};

/// The client interface once the client is in the [ready
/// stage](super#stage-3---ready-stage).
#[derive(Debug)]
pub struct Ready<SimulatorType, RefType>(RefType, PhantomData<SimulatorType>)
where
    SimulatorType: Simulator,
    RefType: Borrow<Active<SimulatorType>>;

impl<'a, SimulatorType: Simulator> From<&'a Active<SimulatorType>>
    for Ready<SimulatorType, &'a Active<SimulatorType>>
{
    fn from(active_client: &'a Active<SimulatorType>) -> Self {
        Self(active_client, PhantomData)
    }
}

impl<'a, SimulatorType: Simulator> From<&'a mut Active<SimulatorType>>
    for Ready<SimulatorType, &'a mut Active<SimulatorType>>
{
    fn from(active_client: &'a mut Active<SimulatorType>) -> Self {
        Self(active_client, PhantomData)
    }
}

impl<SimulatorType, RefType> Ready<SimulatorType, RefType>
where
    SimulatorType: Simulator,
    RefType: Borrow<Active<SimulatorType>>,
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
    pub fn client_id(&self) -> usize {
        self.0.borrow().client_id()
    }

    /// Iterate through the commands that are being kept around. This is intended to be for
    /// diagnostic purposes.
    pub fn buffered_commands(
        &self,
    ) -> impl Iterator<
        Item = (
            Timestamp,
            &Vec<<SimulatorType::WorldType as World>::CommandType>,
        ),
    > {
        self.0.borrow().buffered_commands()
    }

    /// Get the current display state that can be used to render the client's screen.
    pub fn display_state(&self) -> &Tweened<<SimulatorType::WorldType as World>::DisplayStateType> {
        self.0
            .borrow()
            .display_state()
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
        &self.0.borrow().last_queued_snapshot_timestamp()
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
        &self.0.borrow().last_received_snapshot_timestamp()
    }
}

impl<SimulatorType, RefType> Ready<SimulatorType, RefType>
where
    SimulatorType: Simulator,
    RefType: Borrow<Active<SimulatorType>> + BorrowMut<Active<SimulatorType>>,
{
    /// Issue a command from this client's player to the world. The command will be scheduled
    /// to the current simulating timestamp (the previously completed timestamp + 1).
    pub fn issue_command<NetworkResourceType: NetworkResource>(
        &mut self,
        command: <SimulatorType::WorldType as World>::CommandType,
        net: &mut NetworkResourceType,
    ) {
        self.0.borrow_mut().issue_command(command, net);
    }
}
