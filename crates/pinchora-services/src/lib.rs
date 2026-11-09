//! Shared infrastructure; domain services join this root when their use cases move.
pub mod infra;

use crate::infra::Infrastructure;
use std::sync::Arc;

pub struct Services {
    pub infrastructure: Arc<Infrastructure>,
}

impl Services {
    pub fn new(infrastructure: Arc<Infrastructure>) -> Self {
        Self { infrastructure }
    }
}
