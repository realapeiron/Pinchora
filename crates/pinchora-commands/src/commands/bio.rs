use pinchora_core::t;
use twilight_interactions::command::{CommandModel, CreateCommand};

use crate::Cx;

#[derive(CommandModel, CreateCommand)]
#[command(name = "bio", desc = "Set the about-me text on your profile card")]
pub struct Bio {
    /// Up to 160 characters; send "-" to clear it
    #[command(max_length = 160)]
    text: String,
}

impl Bio {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let text = self.text.split_whitespace().collect::<Vec<_>>().join(" ");
        let text = if text == "-" { String::new() } else { text };
        cx.app.db.set_bio(cx.author()?.id.get(), &text).await?;
        let reply = if text.is_empty() {
            t!(cx.lang(), "bio.cleared")
        } else {
            t!(cx.lang(), "bio.updated")
        };
        cx.reply_ephemeral(reply).await
    }
}
