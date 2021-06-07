use super::{Timestamp, Timestamped};
use tracing::warn;

#[derive(Debug, Default)]
pub struct AlwaysIncreasingFilter {
    current_timestamp: Option<Timestamp>,
    last_received_timestamp: Option<Timestamp>,
    last_accepted_timestamp: Option<Timestamp>,
}

#[derive(Debug)]
pub enum FilterError {
    Stale,
    FromTheFuture,
}

impl AlwaysIncreasingFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn try_apply<T>(&mut self, item: Timestamped<T>) -> Result<Timestamped<T>, FilterError> {
        self.last_received_timestamp = Some(item.timestamp());
        self.ensure_not_stale(item.timestamp())?;
        self.ensure_not_from_the_future(item.timestamp())?;
        self.last_accepted_timestamp = Some(item.timestamp());
        Ok(item)
    }

    pub fn update(&mut self, current_timestamp: Timestamp, oldest_expected_timestamp: Timestamp) {
        self.current_timestamp = Some(current_timestamp);
        if let Some(last_accepted_timestamp) = self.last_accepted_timestamp {
            let comparable_range = Timestamp::comparable_range_with_midpoint(current_timestamp);
            if !comparable_range.contains(&last_accepted_timestamp)
                || last_accepted_timestamp > current_timestamp
            {
                warn!("AlwaysIncreasingFilter has not received a new item in a long time, and its last accepted timestamp is no longer comparable with the current timestamp. Resetting this timestamp.");

                // This is done to prevent new snapshots from being discarded due
                // to a very old snapshot that was last applied. This can happen
                // after a very long pause (e.g.  browser tab sleeping).  We don't
                // jump to `comparable_range.start` since it will be invalid
                // immediately in the next iteration.
                self.last_accepted_timestamp = Some(oldest_expected_timestamp);
            }
        }
    }

    fn ensure_not_stale(&self, item_timestamp: Timestamp) -> Result<(), FilterError> {
        if let Some(last_received_timestamp) = self.last_received_timestamp {
            if item_timestamp < last_received_timestamp {
                return Err(FilterError::Stale);
            }
        }
        Ok(())
    }

    fn ensure_not_from_the_future(&self, item_timestamp: Timestamp) -> Result<(), FilterError> {
        if let Some(current_timestamp) = self.current_timestamp {
            if item_timestamp > current_timestamp {
                return Err(FilterError::FromTheFuture);
            }
        }
        Ok(())
    }
}
