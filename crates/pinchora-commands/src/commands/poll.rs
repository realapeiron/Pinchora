//! `/poll`: a question with two to five answers, one vote per member, results drawn as a pixel
//! bar chart when it closes (or on demand while it is open, at most every ten seconds).

use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, Mutex},
};

use pinchora_core::server_events::{POLL_DEFAULT, POLL_MAX, POLL_MIN, parse_duration};
use pinchora_db::{MAX_OPEN_POLLS, NewPoll, PollOpen, PollRow, PollVote};
use pinchora_render::{PollCard, PollOption, Theme};
use twilight_interactions::command::{CommandModel, CreateCommand};
use twilight_model::{
    channel::message::{Component, Embed, component::ButtonStyle},
    http::attachment::Attachment,
    id::Id,
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFooterBuilder, ImageSource};

use crate::{
    App, Cx,
    commands::games::{edit_message, quiet},
    server_events::{button, is_permanent, message_link, post_refusal, row, tell},
    util::{BRAND, SUCCESS, relative, single_line, strip_mentions},
};

/// Longest answer, as Discord's button label allows.
const OPTION_MAX: usize = 80;
/// The Results button renders a poll at most this often (seconds).
const RESULTS_EVERY: i64 = 10;
/// Result cards kept at once; more than this and the oldest are dropped.
const KEPT_CARDS: usize = 200;

/// A card drawn at a time (unix seconds).
type Drawn = (i64, Arc<Vec<u8>>);

/// Result cards rendered for open polls, by poll id.
static CARDS: LazyLock<Mutex<HashMap<i64, Drawn>>> = LazyLock::new(Mutex::default);

#[derive(CommandModel, CreateCommand)]
#[command(
    name = "poll",
    desc = "Ask the server a question with up to five answers",
    contexts = "guild"
)]
pub struct Poll {
    /// The question
    #[command(min_length = 1, max_length = 200)]
    question: String,
    /// Two to five answers, separated by |
    #[command(min_length = 3, max_length = 450)]
    options: String,
    /// How long it stays open: 30m, 6h, 1d (default 1 day, up to 14 days)
    #[command(max_length = 20)]
    duration: Option<String>,
}

/// The answers a member typed, separated by `|`: mentions stripped, blanks dropped.
fn parse_options(raw: &str) -> Result<Vec<String>, String> {
    let options: Vec<String> = raw
        .split('|')
        .map(|o| single_line(&strip_mentions(o)))
        .filter(|o| !o.is_empty())
        .collect();
    if !(2..=5).contains(&options.len()) {
        return Err("A poll needs two to five answers, separated by `|`, like `Pizza | Tacos | Sushi`.".to_owned());
    }
    if options.iter().any(|o| o.chars().count() > OPTION_MAX) {
        return Err(format!("Each answer can be up to {OPTION_MAX} characters."));
    }
    let lower: Vec<String> = options.iter().map(|o| o.to_lowercase()).collect();
    if lower.iter().enumerate().any(|(i, o)| lower[..i].contains(o)) {
        return Err("Every answer needs to be different.".to_owned());
    }
    Ok(options)
}

fn vote_id(poll: i64, choice: usize) -> String {
    format!("pl:v:{poll}:{choice}")
}

fn results_id(poll: i64) -> String {
    format!("pl:r:{poll}")
}

/// What a `pl:` button asks for.
#[derive(Debug, PartialEq, Eq)]
enum Click {
    Vote(i64, usize),
    Results(i64),
}

fn parse_button<'a>(mut parts: impl Iterator<Item = &'a str>) -> Option<Click> {
    let click = match parts.next()? {
        "v" => Click::Vote(parts.next()?.parse().ok()?, parts.next()?.parse().ok()?),
        "r" => Click::Results(parts.next()?.parse().ok()?),
        _ => return None,
    };
    parts.next().is_none().then_some(click)
}

/// The poll as its message shows it: the question, the answers and when it closes. Votes stay
/// hidden until it closes.
fn poll_embed(p: &PollRow) -> Embed {
    let mut lines: Vec<String> = p
        .options
        .iter()
        .enumerate()
        .map(|(i, o)| format!("**{}** · {o}", i + 1))
        .collect();
    lines.push(String::new());
    lines.push(format!("Asked by <@{}>", p.author_id));
    if p.is_open() {
        lines.push(format!("Closes {}", relative(p.ends_at)));
    } else {
        lines.push("This poll is closed.".to_owned());
    }
    EmbedBuilder::new()
        .color(if p.is_open() { BRAND } else { SUCCESS })
        .title(&p.question)
        .description(lines.join("\n"))
        .footer(EmbedFooterBuilder::new(format!(
            "Poll #{} · one vote each; press your answer again to take it back",
            p.id
        )))
        .build()
}

/// An answer button each, then Results; none once the poll is closed.
fn poll_components(p: &PollRow) -> Vec<Component> {
    if !p.is_open() {
        return Vec::new();
    }
    let mut buttons: Vec<Component> = p
        .options
        .iter()
        .enumerate()
        .map(|(i, o)| button(vote_id(p.id, i), o, None, ButtonStyle::Secondary, false))
        .collect();
    buttons.push(button(
        results_id(p.id),
        "Results",
        Some("📊"),
        ButtonStyle::Primary,
        false,
    ));
    buttons.chunks(5).map(|chunk| row(chunk.to_vec())).collect()
}

impl Poll {
    pub async fn run(self, cx: &Cx) -> anyhow::Result<()> {
        let Some(guild) = cx.guild_id() else {
            return tell(cx, "Polls only exist inside a server.").await;
        };
        let question = single_line(&strip_mentions(&self.question));
        if question.is_empty() {
            return tell(cx, "Ask a question.").await;
        }
        let options = match parse_options(&self.options) {
            Ok(options) => options,
            Err(why) => return tell(cx, why).await,
        };
        let span = match self.duration.as_deref() {
            None => POLL_DEFAULT,
            Some(text) => match parse_duration(text) {
                Some(span) => span,
                None => return tell(cx, "I couldn't read that duration. Try `30m`, `6h` or `1d`.").await,
            },
        };
        if span < POLL_MIN || span > POLL_MAX {
            return tell(cx, "A poll stays open from 1 minute to 14 days.").await;
        }
        let channel = cx
            .interaction
            .channel
            .as_ref()
            .map(|c| c.id)
            .ok_or_else(|| anyhow::anyhow!("poll without a channel"))?;
        let author = cx.author()?.id.get();

        let opened = cx
            .app
            .db
            .create_poll(NewPoll {
                guild_id: guild,
                channel_id: channel.get(),
                author,
                question: &question,
                options: &options,
                ends_at: cx.app.now() + span,
            })
            .await?;
        let poll = match opened {
            PollOpen::Started(poll) => poll,
            PollOpen::TooMany => {
                return tell(
                    cx,
                    format!("You already have {MAX_OPEN_POLLS} polls open here. Wait for one to close."),
                )
                .await;
            }
        };
        let posted = cx
            .app
            .http
            .create_message(channel)
            .embeds(&[poll_embed(&poll)])
            .components(&poll_components(&poll))
            .allowed_mentions(quiet().as_ref())
            .await;
        let message = match posted {
            Ok(response) => response.model().await?,
            Err(error) => {
                tracing::warn!(guild, poll = poll.id, %error, "could not post a poll");
                cx.app.db.abort_poll(poll.id).await?;
                return tell(cx, post_refusal(channel, &error)).await;
            }
        };
        cx.app
            .db
            .set_poll_message(poll.id, channel.get(), message.id.get())
            .await?;
        tell(
            cx,
            format!(
                "Poll **#{}** is open in <#{channel}>: {}",
                poll.id,
                message_link(guild, channel.get(), message.id.get())
            ),
        )
        .await
    }
}

/// `pl:v:<poll>:<option>` votes, `pl:r:<poll>` shows the results so far.
pub(crate) async fn on_component<'a>(cx: &Cx, parts: impl Iterator<Item = &'a str>) -> anyhow::Result<()> {
    let Some(click) = parse_button(parts) else {
        return tell(cx, "This button has expired.").await;
    };
    let (Click::Vote(id, _) | Click::Results(id)) = click;
    let guild = cx.guild_id();
    let poll = cx.app.db.poll(id).await?.filter(|p| Some(p.guild_id as u64) == guild);
    let Some(poll) = poll else {
        return tell(cx, "That poll no longer exists.").await;
    };
    match click {
        Click::Vote(_, choice) => vote(cx, &poll, choice).await,
        Click::Results(_) => results(cx, &poll).await,
    }
}

async fn vote(cx: &Cx, poll: &PollRow, choice: usize) -> anyhow::Result<()> {
    let Some(answer) = poll.options.get(choice) else {
        return tell(cx, "This button has expired.").await;
    };
    let user = cx.author()?.id.get();
    let outcome = cx.app.db.vote_poll(poll.id, user, choice, cx.app.now()).await?;
    tell(
        cx,
        match outcome {
            PollVote::Cast => format!("Your vote for **{answer}** is in."),
            PollVote::Changed => format!("Moved your vote to **{answer}**."),
            PollVote::Withdrawn => "Took your vote back.".to_owned(),
            PollVote::Closed => "This poll has closed.".to_owned(),
        },
    )
    .await
}

/// Draws the card for `poll` with the votes in `tally`.
async fn render(app: &App, poll: &PollRow, tally: &[i64]) -> anyhow::Result<Vec<u8>> {
    let card = PollCard {
        number: poll.id,
        question: poll.question.clone(),
        options: poll
            .options
            .iter()
            .zip(tally)
            .map(|(label, votes)| PollOption {
                label: label.clone(),
                votes: *votes,
            })
            .collect(),
        closed: !poll.is_open(),
        theme: Theme::reef(),
    };
    app.render(move |r| r.poll_card(&card)).await
}

/// The Results button: a card of the votes so far, only the member sees it. The card is drawn once
/// per ten seconds per poll; presses in between show that same card.
async fn results(cx: &Cx, poll: &PollRow) -> anyhow::Result<()> {
    let now = cx.app.now().timestamp();
    let cached = CARDS
        .lock()
        .ok()
        .and_then(|cards| cards.get(&poll.id).cloned())
        .filter(|(drawn, _)| (0..RESULTS_EVERY).contains(&(now - drawn)))
        .map(|(_, png)| png);
    let png = match cached {
        Some(png) => png,
        None => {
            let tally = cx.app.db.poll_tally(poll.id, poll.options.len()).await?;
            let png = Arc::new(render(&cx.app, poll, &tally).await?);
            if let Ok(mut cards) = CARDS.lock() {
                cards.retain(|_, (drawn, _)| (0..RESULTS_EVERY).contains(&(now - *drawn)));
                if cards.len() >= KEPT_CARDS {
                    cards.clear();
                }
                cards.insert(poll.id, (now, png.clone()));
            }
            png
        }
    };
    let note = if poll.is_open() {
        format!(
            "Closes {}. Results refresh every {RESULTS_EVERY} seconds.",
            relative(poll.ends_at)
        )
    } else {
        "This poll is closed. These are the final results.".to_owned()
    };
    let embed = EmbedBuilder::new().color(BRAND).description(note);
    let mut data = Cx::card(embed, "poll.png", png.to_vec(), Vec::new(), true);
    data.allowed_mentions = quiet();
    cx.reply(data).await
}

/// Closes poll `id` if it is due, and tells the channel. False when it was not open or not due
/// (somebody closed it first).
pub(crate) async fn close(app: &Arc<App>, id: i64) -> anyhow::Result<bool> {
    let Some(poll) = app.db.close_poll(id, app.now()).await? else {
        return Ok(false);
    };
    metrics::counter!("pinchora_polls_closed_total").increment(1);
    if let Err(error) = announce(app, &poll).await {
        // The poll is closed and saved; the ticker tries the announcement again.
        tracing::warn!(%error, poll = id, "poll closed but not announced");
    }
    Ok(true)
}

/// Who won, in words: the leader, a tie, or nobody.
fn verdict(poll: &PollRow, tally: &[i64]) -> String {
    let best = tally.iter().copied().max().unwrap_or(0);
    let total: i64 = tally.iter().sum();
    if best == 0 {
        return "Nobody voted.".to_owned();
    }
    let leaders: Vec<String> = poll
        .options
        .iter()
        .zip(tally)
        .filter(|(_, votes)| **votes == best)
        .map(|(o, _)| format!("**{o}**"))
        .collect();
    let votes = |n: i64| format!("{n} vote{}", if n == 1 { "" } else { "s" });
    match leaders.as_slice() {
        [only] => format!(
            "{only} wins with {} ({}%).",
            votes(best),
            PollCard::percent(best, total)
        ),
        _ => format!("A tie between {} with {} each.", leaders.join(" and "), votes(best)),
    }
}

/// Posts the final card in the channel and closes the poll's own message, once, whichever pod gets
/// there first. A refusal that will never change (the channel is gone) is given up on.
async fn announce(app: &Arc<App>, poll: &PollRow) -> anyhow::Result<()> {
    if !app.db.claim_poll_announcement(poll.id, app.now()).await? {
        return Ok(());
    }
    let tally = app.db.poll_tally(poll.id, poll.options.len()).await?;
    let png = render(app, poll, &tally).await?;
    let channel = Id::new((poll.channel_id as u64).max(1));

    let embed = EmbedBuilder::new()
        .color(SUCCESS)
        .title(format!("Poll closed: {}", poll.question))
        .description(verdict(poll, &tally));
    let embed = match ImageSource::attachment("poll.png") {
        Ok(source) => embed.image(source),
        Err(_) => embed,
    }
    .build();
    let embeds = [embed];
    let attachments = [Attachment::from_bytes("poll.png".to_owned(), png, 1)];
    let no_pings = quiet();
    let mut post = app
        .http
        .create_message(channel)
        .embeds(&embeds)
        .attachments(&attachments)
        .allowed_mentions(no_pings.as_ref());
    if let Some(message) = poll.message_id {
        post = post.reply(Id::new((message as u64).max(1))).fail_if_not_exists(false);
    }
    match post.await {
        Ok(_) => {}
        Err(error) if is_permanent(&error) => {
            tracing::warn!(poll = poll.id, %error, "poll result could not be posted; giving up");
        }
        Err(error) => return Err(error.into()),
    }
    if let Some(message) = poll.message_id {
        let closed = PollRow {
            status: "closed".to_owned(),
            ..poll.clone()
        };
        edit_message(
            app,
            poll.channel_id as u64,
            message as u64,
            None,
            Some(&[poll_embed(&closed)]),
            Some(&poll_components(&closed)),
        )
        .await;
    }
    app.db.mark_poll_announced(poll.id).await
}

/// Tells the channels of polls that closed without being announced, once their claim has lapsed.
pub(crate) async fn announce_pending(app: &Arc<App>) {
    let pending = match app.db.unannounced_polls(20).await {
        Ok(pending) => pending,
        Err(error) => return tracing::warn!(%error, "unannounced polls not loaded"),
    };
    for poll in pending {
        if let Err(error) = announce(app, &poll).await {
            tracing::warn!(%error, poll = poll.id, "poll result not announced");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn poll(open: bool) -> PollRow {
        PollRow {
            id: 9,
            guild_id: 1,
            channel_id: 2,
            message_id: Some(3),
            author_id: 4,
            question: "Pizza or tacos?".to_owned(),
            options: vec!["Pizza".to_owned(), "Tacos".to_owned(), "Sushi".to_owned()],
            ends_at: chrono::Utc::now(),
            status: if open { "open" } else { "closed" }.to_owned(),
            announced: false,
        }
    }

    #[test]
    fn answers_are_split_cleaned_and_counted() {
        assert_eq!(
            parse_options("Pizza | <@1> Tacos |  | @everyone Sushi ").unwrap(),
            ["Pizza", "Tacos", "everyone Sushi"]
        );
        assert!(parse_options("Only one").is_err());
        assert!(parse_options("a|b|c|d|e|f").is_err());
        assert!(parse_options("a| |<@1>").is_err(), "blanks do not count");
        assert!(parse_options(&format!("a|{}", "x".repeat(81))).is_err());
        assert!(parse_options(&format!("a|{}", "x".repeat(80))).is_ok());
        assert!(parse_options("Same|same").is_err());
    }

    #[test]
    fn buttons_round_trip() {
        for (id, expected) in [
            (vote_id(12, 3), Click::Vote(12, 3)),
            (results_id(12), Click::Results(12)),
        ] {
            let mut parts = id.split(':');
            assert_eq!(parts.next(), Some("pl"));
            assert_eq!(parse_button(parts), Some(expected));
            assert!(id.len() <= 100);
        }
        for bad in ["", "v", "v:1", "v:1:x", "v:x:1", "r", "r:x", "r:1:2", "v:1:2:3", "z:1"] {
            assert_eq!(parse_button(bad.split(':')), None, "{bad:?}");
        }
    }

    #[test]
    fn an_open_poll_has_a_button_per_answer_and_results() {
        let rows = poll_components(&poll(true));
        let widths: Vec<usize> = rows
            .iter()
            .map(|r| match r {
                Component::ActionRow(r) => r.components.len(),
                _ => 0,
            })
            .collect();
        assert_eq!(widths, [4], "three answers and Results");
        let mut five = poll(true);
        five.options = (1..=5).map(|n| format!("Answer {n}")).collect();
        let rows = poll_components(&five);
        assert_eq!(rows.len(), 2, "five answers fill a row; Results goes on the next");
        assert!(poll_components(&poll(false)).is_empty(), "no buttons once closed");
    }

    #[test]
    fn the_verdict_names_a_winner_a_tie_or_nobody() {
        let p = poll(false);
        assert_eq!(verdict(&p, &[0, 0, 0]), "Nobody voted.");
        assert_eq!(verdict(&p, &[3, 1, 0]), "**Pizza** wins with 3 votes (75%).");
        assert_eq!(verdict(&p, &[1, 0, 0]), "**Pizza** wins with 1 vote (100%).");
        assert_eq!(
            verdict(&p, &[2, 2, 1]),
            "A tie between **Pizza** and **Tacos** with 2 votes each."
        );
    }
}
