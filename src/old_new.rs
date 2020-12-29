/// Like a circular buffer of size 2, aka a double buffer.
pub struct OldNew<T>
where
    T: Default,
{
    left: T,
    right: T,
    state: OldNewState,
}

pub enum OldNewState {
    LeftOldRightNew,
    LeftNewRightOld,
}

impl<T> OldNew<T>
where
    T: Default,
{
    pub fn new() -> Self {
        Self {
            left: Default::default(),
            right: Default::default(),
            state: OldNewState::LeftNewRightOld,
        }
    }

    pub fn set_old(&mut self, value: T) {
        match &self.state {
            OldNewState::LeftNewRightOld => self.right = value,
            OldNewState::LeftOldRightNew => self.left = value,
        }
    }

    pub fn set_new(&mut self, value: T) {
        match &self.state {
            OldNewState::LeftNewRightOld => self.left = value,
            OldNewState::LeftOldRightNew => self.right = value,
        }
    }

    pub fn get(&self) -> (&T, &T) {
        match &self.state {
            OldNewState::LeftNewRightOld => (&self.right, &self.left),
            OldNewState::LeftOldRightNew => (&self.left, &self.right),
        }
    }

    pub fn get_mut(&mut self) -> (&mut T, &mut T) {
        match &self.state {
            OldNewState::LeftNewRightOld => (&mut self.right, &mut self.left),
            OldNewState::LeftOldRightNew => (&mut self.left, &mut self.right),
        }
    }

    pub fn swap(&mut self) {
        self.state = match &self.state {
            OldNewState::LeftNewRightOld => OldNewState::LeftOldRightNew,
            OldNewState::LeftOldRightNew => OldNewState::LeftNewRightOld,
        };
    }
}
