//! Shared limits for starboards and vote channels.

/// A server can run this many distinct boards.
pub const MAX_BOARDS: usize = 3;
/// A board needs at least one and at most this many weighted stars.
pub const MAX_THRESHOLD: i32 = 50;
/// A star may count for this many votes at most when a member holds a weighted role.
pub const MAX_ROLE_WEIGHT: i64 = 50;
/// Maximum source channels on each allow or deny list.
pub const MAX_SOURCE_CHANNELS: usize = 50;
/// Maximum roles with a custom weight.
pub const MAX_WEIGHTED_ROLES: usize = 25;
