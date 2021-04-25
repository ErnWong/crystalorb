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

pub struct OldNewResult<T> {
    pub old: T,
    pub new: T,
}

impl<T> Default for OldNew<T>
where
    T: Default,
{
    fn default() -> Self {
        Self::new()
    }
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

    pub fn get(&self) -> OldNewResult<&T> {
        match &self.state {
            OldNewState::LeftNewRightOld => OldNewResult {
                old: &self.right,
                new: &self.left,
            },
            OldNewState::LeftOldRightNew => OldNewResult {
                old: &self.left,
                new: &self.right,
            },
        }
    }

    pub fn get_mut(&mut self) -> OldNewResult<&mut T> {
        match &self.state {
            OldNewState::LeftNewRightOld => OldNewResult {
                old: &mut self.right,
                new: &mut self.left,
            },
            OldNewState::LeftOldRightNew => OldNewResult {
                old: &mut self.left,
                new: &mut self.right,
            },
        }
    }

    pub fn swap(&mut self) {
        self.state = match &self.state {
            OldNewState::LeftNewRightOld => OldNewState::LeftOldRightNew,
            OldNewState::LeftOldRightNew => OldNewState::LeftNewRightOld,
        };
    }
}
