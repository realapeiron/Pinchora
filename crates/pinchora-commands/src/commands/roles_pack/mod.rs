//! The roles pack: auto roles, role panels in three styles (buttons, a menu, reactions), roles that
//! expire, roles members keep when they leave and come back, and colour roles. `/roles` itself and its
//! `rr`/`rb` buttons live in `role_panels.rs`; this folder holds everything around them.

mod autoroles;
mod color;
mod give;
mod join;
mod palette;
mod panel;
mod reactions;
mod timed;

pub use autoroles::AutoRolesCommand;
pub(crate) use color::on_component as on_color_component;
pub use color::{
    ColorCommand, ColorRoleReport, ColorStop, create_palette_roles, delete_color_role, stop_text as color_stop_text,
};
pub(crate) use give::configure_refusal;
pub use join::{give_after_verify, start_sticky_snapshot, sticky_changed};
pub(crate) use join::{on_member_join, on_member_leave, on_member_update};
pub use palette::{PaletteColor, palette};
pub use panel::{PostRefusal, PostedPanel, post_panel};
pub(crate) use panel::{on_select, on_toggle};
pub(crate) use reactions::on_reaction;
pub use timed::{spawn_ticker, tick};
