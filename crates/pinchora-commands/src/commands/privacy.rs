//! `/privacy`: a member's own choices about what servers may send them.

use twilight_interactions::command::{CommandModel, CreateCommand};

use crate::Cx;

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "privacy",
    desc = "Choose whether servers may send you direct messages through Pinchora"
)]
pub struct Privacy {
    /// Let servers' custom commands, auto-responders, level-ups and suggestion updates DM you
    server_dms: Option<bool>,
}

/// What `/privacy` says about the choice, whichever way it is.
fn wording(off: bool) -> &'static str {
    if off {
        "Direct messages from servers are **off**: custom commands, auto-responders, level-ups and suggestion \
         updates will not DM you. Turn them back on with `/privacy server_dms:True`."
    } else {
        "Direct messages from servers are **on**: a server's custom commands, auto-responders, level-ups and \
         suggestion updates may DM you. Turn them off with `/privacy server_dms:False`."
    }
}

impl Privacy {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let user = cx.author()?.id.get();
        let off = match self.server_dms {
            Some(on) => {
                cx.app.db.set_dm_opt_out(user, !on).await?;
                !on
            }
            None => cx.app.db.dm_opt_out(user).await?,
        };
        cx.reply_ephemeral(wording(off)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wording_names_the_way_back() {
        assert!(wording(true).contains("server_dms:True"));
        assert!(wording(false).contains("server_dms:False"));
    }
}
