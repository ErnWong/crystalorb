pub struct DisplayStateSelector<'a, ChainType: Chain>(&'a ChainType);

impl<'a, ChainType: Chain> DisplayStateSelector<'a, ChainType> {
    pub fn of<SimulatorType: Simulator>(&self) -> &DisplayState
    where
        Self: Contains<Simulator> + GetFirst<Simulator>,
    {
        self.get_first().display_state()
    }
}

mod private {
    /// Your fate is
    pub trait Sealed {}
}

/// Marker trait. Impl it for your [`Simulator`] if you want to add it to a [`Chain`].
///
/// Note: We do *not* implement `ChainElement` for `Chain` to prevent a nested `Chain` appearing
/// first.
///
/// ```compile_fail
/// struct MySimulator1;
/// impl Simulator for MySimulator1 {}
/// impl Element for MySimulator1 {}
///
/// struct MySimulator2;
/// impl Simulator for MySimulator2 {}
/// impl Element for MySimulator2 {}
///
/// let nonlinear_chain = Chain(Chain(MySimulator1, End), Chain(MySimulator2, End));
/// ```
pub trait Element: Simulator {}

pub trait Next: Simulator + private::Sealed {}

pub struct End;
impl Simulator for End {}
impl Next for End {}

pub struct Chain<ElementType: Element, NextType: Next>(ElementType, NextType);

impl<ElementType: Element, NextType: Next> Simulator for Chain<ElementType, NextType> {
    fn receive_command(&mut self, command: &Timestamped<WorldType::CommandType>) {
        self.0.receive_command(command);
        self.1.receive_command(command);
    }

    fn receive_snapshot(&mut self, snapshot: Timestamped<WorldType::SnapshotType>) {
        self.0.receive_snapshot(snapshot);
        self.1.receive_snapshot(snapshot);
    }
}

impl<L: Simulator, R: Simulator> ChainNext for Chain<L, R> {}

pub trait Contains<DesiredSimulatorType> {
    fn get_first(&self) -> &DesiredSimulatorType;
}

impl<DesiredSimulatorType, NextType> Contains<DesiredSimulatorType>
    for Chain<DesiredSimulatorType, NextType>
{
    fn get_first(&self) -> &DesiredSimulatorType {
        &self.0
    }
}

impl<DesiredSimulatorType, ElementType, NextType> Contains<DesiredSimulatorType>
    for Chain<ElementType, NextType>
where
    NextType: Contains<DesiredSimulatorType>,
{
    fn get_first(&self) -> &DesiredSimulatorType {
        &self.1.get_first()
    }
}
