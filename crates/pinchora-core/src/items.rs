//! Item ids as inventories and houses store them.
//!
//! A piece id is `sofa` or `sofa__mint`. A **soulbound** piece (what fragments buy) carries
//! [`SOULBOUND`] on the end: `sofa__mint~bound`. It is a row of its own in the inventory and a placement
//! of its own in a room, so the mark travels with every piece through placing, storing, undoing and
//! clearing, and the three ways a piece leaves its owner (a stall, a gift or plaza donation, a partner's
//! house) only have to look at the id.

/// What ends the id of a soulbound piece.
pub const SOULBOUND: &str = "~bound";

/// Whether `item_id` is a soulbound piece.
pub fn is_soulbound(item_id: &str) -> bool {
    item_id.ends_with(SOULBOUND)
}

/// `item_id` as a soulbound piece; one that already is stays as it is.
pub fn soulbound(item_id: &str) -> String {
    if is_soulbound(item_id) {
        item_id.to_owned()
    } else {
        format!("{item_id}{SOULBOUND}")
    }
}

/// `item_id` without the soulbound mark: the piece the catalogue knows.
pub fn plain(item_id: &str) -> &str {
    item_id.strip_suffix(SOULBOUND).unwrap_or(item_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mark_goes_on_once_and_comes_off() {
        assert_eq!(soulbound("sofa__mint"), "sofa__mint~bound");
        assert_eq!(soulbound("sofa__mint~bound"), "sofa__mint~bound");
        assert!(is_soulbound("table~bound") && !is_soulbound("table"));
        assert_eq!(plain("sofa__mint~bound"), "sofa__mint");
        assert_eq!(plain("sofa__mint"), "sofa__mint");
        assert_eq!(plain(&soulbound("lamp")), "lamp");
    }
}
