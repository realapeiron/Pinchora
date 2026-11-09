use super::*;

// /Export

/// What an export file holds: the commands (or responders) of one server, to be imported into another.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Export<T> {
    pub version: u32,
    pub items: Vec<T>,
}

impl<T> Export<T> {
    pub const VERSION: u32 = 1;

    pub fn new(items: Vec<T>) -> Self {
        Self {
            version: Self::VERSION,
            items,
        }
    }
}
