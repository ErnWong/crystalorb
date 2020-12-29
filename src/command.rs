use serde::{de::DeserializeOwned, Serialize};
use std::fmt::Debug;

pub trait Command: Clone + Sync + Send + 'static + Serialize + DeserializeOwned + Debug {}
