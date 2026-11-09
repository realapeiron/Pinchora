//! The collection games: each server's chat-drop settings, the Tide Pool Dex, critter drops and
//! bottles, beachcombs and crab pots.
//!
//! Every payment happens in the same transaction as the thing that earns it (a card taken, a bottle
//! opened, a walk, a haul), so a double click, a race between pods or a restart can never pay twice.
//! The rules (what an entry pays, which entry turned up) belong to `pinchora-game`; this module only
//! applies them.

use chrono::{DateTime, Duration, Utc};
use pinchora_core::games::items::soulbound;
use sqlx::{Postgres, Transaction};

use crate::Db;
use crate::economy::legacy::ledger;
use crate::economy::player::movement;
use crate::house::house::add_inventory;
use crate::infra::locks::Retry;
use crate::infra::locks::lock_users;
use crate::infra::locks::retry;
use crate::sql_id;

type Tx<'a> = Transaction<'a, Postgres>;

pub const CRITTERS: &str = "critters";
pub const BOTTLE: &str = "bottle";
pub const CLAIMS_PER_DAY: i32 = 10;
pub const PROTECTION_SECS: i64 = 15;

/// A critter drop or a bottle on its message.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DropRow {
    pub id: i64,
    pub kind: String,
    pub guild_id: i64,
    pub channel_id: i64,
    pub message_id: Option<i64>,
    pub host_id: Option<i64>,
    pub cards: Vec<String>,
    /// Who holds each slot; 0 while it is free.
    pub takers: Vec<i64>,
    pub status: String,
    pub expires_at: DateTime<Utc>,
    pub mode: String,
    pub challenge_word: Option<String>,
    pub vote_winner: Option<i16>,
    pub created_at: DateTime<Utc>,
    pub protected_user_id: Option<i64>,
    pub protected_slot: Option<i16>,
    pub protected_until: Option<DateTime<Utc>>,
}

impl DropRow {
    pub fn is_open(&self) -> bool {
        self.status == "open"
    }

    /// Whether `user` holds a slot.
    pub fn taken_by(&self, user: u64) -> bool {
        self.takers.contains(&sql_id(user))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct NewDrop<'a> {
    pub kind: &'a str,
    pub guild_id: u64,
    pub channel_id: u64,
    pub host: Option<u64>,
    pub cards: &'a [String],
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub enum Grab {
    /// The card is theirs.
    Taken {
        granted: Granted,
        drop: Box<DropRow>,
        /// False when the day's paying plays were used up: the card is theirs, nothing was paid.
        paid: bool,
    },
    /// The drop is over, or the slot is not what the message showed.
    Gone,
    /// They already took a card from this drop.
    Have {
        slot: usize,
    },
    /// Somebody else got there first.
    Lost {
        by: u64,
    },
    Protected,
    Limit,
    Voting,
    Challenge,
}

/// What is in a bottle besides its message.
#[derive(Debug, Clone, Copy)]
pub enum BottlePrize<'a> {
    Shells(i64),
    Fragments(i64),
    Treasure(i64),
    Find(Award<'a>),
}

#[derive(Debug, Clone)]
pub enum BottleOpen {
    Opened {
        drop: Box<DropRow>,
        shells: i64,
        fragments: i64,
        /// The find, if it held one.
        granted: Option<Granted>,
        paid: bool,
    },
    Gone,
    /// Somebody opened it first.
    Lost {
        by: u64,
    },
}

macro_rules! drop_columns {
    () => {
        "id, kind, guild_id, channel_id, message_id, host_id, cards, takers, status, expires_at, mode, challenge_word, vote_winner, created_at, protected_user_id, protected_slot, protected_until"
    };
}

impl Db {
    // /Drops and bottles

    pub async fn create_drop(&self, new: NewDrop<'_>) -> anyhow::Result<DropRow> {
        let mut tx = self.pool().begin().await?;
        let mut row = sqlx::query_as::<_, DropRow>(concat!(
            "INSERT INTO collection_drops (kind, guild_id, channel_id, host_id, cards, takers, expires_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING ",
            drop_columns!()
        ))
        .bind(new.kind)
        .bind(sql_id(new.guild_id))
        .bind(sql_id(new.channel_id))
        .bind(new.host.map(sql_id))
        .bind(new.cards.to_vec())
        .bind(vec![0_i64; new.cards.len()])
        .bind(new.expires_at)
        .fetch_one(&mut *tx)
        .await?;
        if new.kind == CRITTERS {
            let ids: Vec<&str> = new.cards.iter().map(|id| id.strip_prefix("g:").unwrap_or(id)).collect();
            let wanted: Option<(i64, String)> = sqlx::query_as(
                "SELECT w.user_id, w.entry_id FROM collection_wishlist w
                 JOIN members m ON m.user_id = w.user_id AND m.guild_id = $2
                 WHERE w.ping_on_spawn AND w.entry_id = ANY($1)
                   AND NOT EXISTS (SELECT 1 FROM users u WHERE u.id = w.user_id AND u.dm_opt_out)
                   AND NOT EXISTS (SELECT 1 FROM guestbook_blocks b WHERE
                     (b.host_id = w.user_id AND b.blocked_id = $3) OR (b.blocked_id = w.user_id AND b.host_id = $3))
                   AND NOT EXISTS (SELECT 1 FROM collection_daily d WHERE d.user_id = w.user_id
                     AND d.game = 'critter_claim' AND d.day = floor(extract(epoch FROM now()) / 86400)::integer AND d.plays >= 10)
                   AND NOT EXISTS (SELECT 1 FROM deleted_users u WHERE u.user_id=w.user_id AND u.collection_ping_until>now())
                   AND NOT EXISTS (SELECT 1 FROM collection_drops d WHERE d.protected_user_id = w.user_id
                     AND d.created_at > now() - interval '1 hour')
                 ORDER BY random() LIMIT 1",
            )
            .bind(&ids)
            .bind(sql_id(new.guild_id))
            .bind(new.host.map(sql_id))
            .fetch_optional(&mut *tx)
            .await?;
            if let Some((user, entry)) = wanted
                && let Some(slot) = ids.iter().position(|id| *id == entry)
            {
                let until = Utc::now() + Duration::seconds(PROTECTION_SECS);
                sqlx::query("UPDATE collection_drops SET protected_user_id = $2, protected_slot = $3, protected_until = $4 WHERE id = $1")
                    .bind(row.id).bind(user).bind(slot as i16).bind(until)
                    .execute(&mut *tx).await?;
                row.protected_user_id = Some(user);
                row.protected_slot = Some(slot as i16);
                row.protected_until = Some(until);
            }
        }
        tx.commit().await?;
        Ok(row)
    }

    /// Revalidate consent immediately before delivery; only the initial delivery arms the window.
    pub async fn prepare_collection_ping(&self, id: i64, member_present: bool) -> anyhow::Result<DropRow> {
        let mut tx = self.pool().begin().await?;
        let candidate: Option<(Option<i64>,)> =
            sqlx::query_as("SELECT protected_user_id FROM collection_drops WHERE id=$1")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
        if let Some((Some(user),)) = candidate {
            lock_users(&mut tx, &[user]).await?;
        }
        let row: DropRow = sqlx::query_as(concat!(
            "SELECT ",
            drop_columns!(),
            " FROM collection_drops WHERE id=$1 FOR UPDATE"
        ))
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        if row.message_id.is_some() {
            return Ok(row);
        }
        let eligible: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM collection_wishlist w WHERE w.user_id=$1 AND w.ping_on_spawn
            AND w.entry_id = $2 AND NOT EXISTS (SELECT 1 FROM users u WHERE u.id=w.user_id AND u.dm_opt_out)
            AND NOT EXISTS (SELECT 1 FROM guestbook_blocks b WHERE (b.host_id=w.user_id AND b.blocked_id=$3) OR (b.blocked_id=w.user_id AND b.host_id=$3))
            AND NOT EXISTS (SELECT 1 FROM collection_daily d WHERE d.user_id=w.user_id AND d.game='critter_claim'
                AND d.day=floor(extract(epoch FROM now())/86400)::integer AND d.plays>=10)
            AND NOT EXISTS (SELECT 1 FROM deleted_users u WHERE u.user_id=w.user_id AND u.collection_ping_until>now())
            AND NOT EXISTS (SELECT 1 FROM collection_drops d WHERE d.id<>$4 AND d.protected_user_id=w.user_id
                AND d.protected_until > now() - interval '1 hour' AND (d.message_id IS NOT NULL OR d.id<$4)))")
            .bind(row.protected_user_id)
            .bind(row.protected_slot.and_then(|i| row.cards.get(i as usize)).map(|s| s.strip_prefix("g:").unwrap_or(s)))
            .bind(row.host_id).bind(id).fetch_one(&mut *tx).await?;
        let active = member_present && eligible && row.mode != "hidden" && row.mode != "vote";
        let row = sqlx::query_as(concat!(
            "UPDATE collection_drops SET protected_user_id=CASE WHEN $2 THEN protected_user_id END,
            protected_slot=CASE WHEN $2 THEN protected_slot END,
            protected_until=CASE WHEN $2 THEN now() + interval '15 seconds' END WHERE id=$1 RETURNING ",
            drop_columns!()
        ))
        .bind(id)
        .bind(active)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(row)
    }

    /// Remembers which message shows the drop, so it can be edited when it ends.
    pub async fn set_drop_message(&self, id: i64, channel: u64, message: u64) -> anyhow::Result<()> {
        sqlx::query("UPDATE collection_drops SET channel_id = $2, message_id = $3 WHERE id = $1")
            .bind(id)
            .bind(sql_id(channel))
            .bind(sql_id(message))
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn set_drop_mode(&self, id: i64, mode: &str, word: Option<&str>) -> anyhow::Result<DropRow> {
        Ok(sqlx::query_as::<_, DropRow>(concat!(
            "UPDATE collection_drops SET mode = $2, challenge_word = $3 WHERE id = $1 RETURNING ",
            drop_columns!()
        ))
        .bind(id)
        .bind(mode)
        .bind(word)
        .fetch_one(self.pool())
        .await?)
    }

    pub async fn vote_card(
        &self,
        drop_id: i64,
        slot: usize,
        user: u64,
        now: DateTime<Utc>,
    ) -> anyhow::Result<VoteCard> {
        if slot >= 3 {
            return Ok(VoteCard::Gone);
        }
        retry("vote_card", || async {
            let mut tx = self.pool().begin().await?;
            lock_users(&mut tx, &[sql_id(user)]).await?;
            let row: Option<DropRow> = sqlx::query_as::<_, DropRow>(concat!(
                "SELECT ",
                drop_columns!(),
                " FROM collection_drops WHERE id = $1 FOR UPDATE"
            ))
            .bind(drop_id)
            .fetch_optional(&mut *tx)
            .await?;
            let Some(_row) = row.filter(|r| {
                r.kind == CRITTERS
                    && r.mode == "vote"
                    && r.is_open()
                    && r.vote_winner.is_none()
                    && slot < r.cards.len()
                    && r.expires_at - Duration::seconds(60) > now
            }) else {
                return Ok(VoteCard::Gone);
            };
            let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM collection_votes WHERE drop_id = $1")
                .bind(drop_id)
                .fetch_one(&mut *tx)
                .await?;
            let previous: Option<(i16,)> =
                sqlx::query_as("SELECT slot FROM collection_votes WHERE drop_id = $1 AND user_id = $2")
                    .bind(drop_id)
                    .bind(sql_id(user))
                    .fetch_optional(&mut *tx)
                    .await?;
            if count >= 100 && previous.is_none() {
                return Ok(VoteCard::Full);
            }
            sqlx::query(
                "INSERT INTO collection_votes (drop_id, user_id, slot) VALUES ($1, $2, $3)
                ON CONFLICT (drop_id, user_id) DO NOTHING",
            )
            .bind(drop_id)
            .bind(sql_id(user))
            .bind(slot as i16)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            Ok(VoteCard::Cast)
        })
        .await
    }

    /// Reveal one winning card after the thirty-second vote. A tie chooses the lowest slot.
    pub async fn reveal_due_votes(&self, now: DateTime<Utc>, limit: i64) -> anyhow::Result<Vec<DropRow>> {
        Ok(sqlx::query_as::<_, DropRow>(concat!(
            "WITH due AS MATERIALIZED (
                SELECT id FROM collection_drops WHERE mode = 'vote' AND status = 'open' AND vote_winner IS NULL
                    AND expires_at - interval '60 seconds' <= $1
                ORDER BY created_at LIMIT $2 FOR UPDATE SKIP LOCKED)
             UPDATE collection_drops d SET vote_winner = COALESCE((
                SELECT v.slot FROM collection_votes v WHERE v.drop_id = d.id
                GROUP BY v.slot ORDER BY count(*) DESC, v.slot LIMIT 1), 0)
             WHERE d.id IN (SELECT id FROM due) RETURNING ",
            drop_columns!()
        ))
        .bind(now)
        .bind(limit)
        .fetch_all(self.pool())
        .await?)
    }

    /// Takes a drop back that never made it to its channel.
    pub async fn abort_drop(&self, id: i64) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM collection_drops WHERE id = $1 AND message_id IS NULL")
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn drop_row(&self, id: i64) -> anyhow::Result<Option<DropRow>> {
        Ok(sqlx::query_as::<_, DropRow>(concat!(
            "SELECT ",
            drop_columns!(),
            " FROM collection_drops WHERE id = $1"
        ))
        .bind(id)
        .fetch_optional(self.pool())
        .await?)
    }

    /// Whether something still floats or lies in the channel, waiting for somebody.
    pub async fn drop_waiting_in(&self, channel: u64, now: DateTime<Utc>) -> anyhow::Result<bool> {
        let (waiting,): (bool,) = sqlx::query_as(
            "SELECT EXISTS (SELECT 1 FROM collection_drops
                            WHERE channel_id = $1 AND status = 'open' AND expires_at > $2)",
        )
        .bind(sql_id(channel))
        .bind(now)
        .fetch_one(self.pool())
        .await?;
        Ok(waiting)
    }

    /// `user` takes card `slot` of the critter drop. `award` is what the card's critter pays; the drop's
    /// card in that slot must be `award.entry`. Takes the card, files it in the dex and pays, all
    /// or nothing.
    pub async fn grab_card(
        &self,
        drop_id: i64,
        slot: usize,
        user: u64,
        now: DateTime<Utc>,
        award: Award<'_>,
        daily: Daily<'_>,
    ) -> anyhow::Result<Grab> {
        retry("grab_card", || {
            self.grab_card_once(drop_id, slot, user, now, award, daily, None)
        })
        .await
    }

    #[allow(clippy::too_many_arguments)] // Mirrors grab_card with the challenge answer checked under the row lock.
    pub async fn grab_word_card(
        &self,
        drop_id: i64,
        slot: usize,
        user: u64,
        now: DateTime<Utc>,
        award: Award<'_>,
        daily: Daily<'_>,
        answer: &str,
    ) -> anyhow::Result<Grab> {
        retry("grab_word_card", || {
            self.grab_card_once(drop_id, slot, user, now, award, daily, Some(answer))
        })
        .await
    }

    /// One attempt of [`Db::grab_card`]: the grabber's lock, then the drop's row.
    #[allow(clippy::too_many_arguments)] // Award, limit and challenge all belong to one atomic claim.
    async fn grab_card_once(
        &self,
        drop_id: i64,
        slot: usize,
        user: u64,
        now: DateTime<Utc>,
        award: Award<'_>,
        daily: Daily<'_>,
        answer: Option<&str>,
    ) -> anyhow::Result<Grab> {
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(user)]).await?;
        let drop: Option<DropRow> = sqlx::query_as(concat!(
            "SELECT ",
            drop_columns!(),
            " FROM collection_drops WHERE id = $1 FOR UPDATE"
        ))
        .bind(drop_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(mut drop) = drop.filter(|d| d.kind == CRITTERS && d.is_open() && d.expires_at > now) else {
            return Ok(Grab::Gone);
        };
        // Erasure removes the old taker's identity and attempt rows. Do not let that reset
        // this player's one-card/three-attempt budget on an already running drop.
        let erased_during_drop: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM deleted_users WHERE user_id=$1 AND deleted_at >= $2)")
                .bind(sql_id(user))
                .bind(drop.created_at)
                .fetch_one(&mut *tx)
                .await?;
        if erased_during_drop {
            return Ok(Grab::Gone);
        }
        let Some(card) = drop.cards.get(slot) else {
            return Ok(Grab::Gone);
        };
        let glimmer = card.starts_with("g:");
        if card.strip_prefix("g:").unwrap_or(card) != award.entry {
            return Ok(Grab::Gone);
        }
        if let Some(have) = drop.takers.iter().position(|t| *t == sql_id(user)) {
            return Ok(Grab::Have { slot: have });
        }
        if drop.takers[slot] < 0 {
            return Ok(Grab::Gone);
        }
        if drop.takers[slot] != 0 {
            return Ok(Grab::Lost {
                by: drop.takers[slot] as u64,
            });
        }
        if drop.mode == "word" {
            let attempts: Option<(i16,)> = sqlx::query_as(
                "INSERT INTO collection_word_attempts (drop_id, user_id, attempts) VALUES ($1,$2,1)
                ON CONFLICT (drop_id,user_id) DO UPDATE SET attempts = collection_word_attempts.attempts + 1
                WHERE collection_word_attempts.attempts < 3 RETURNING attempts",
            )
            .bind(drop_id)
            .bind(sql_id(user))
            .fetch_optional(&mut *tx)
            .await?;
            if attempts.is_none()
                || !drop
                    .challenge_word
                    .as_deref()
                    .is_some_and(|word| answer.is_some_and(|typed| word.eq_ignore_ascii_case(typed.trim())))
            {
                tx.commit().await?;
                return Ok(Grab::Challenge);
            }
        }
        if drop.mode == "vote" && drop.vote_winner != Some(slot as i16) {
            return Ok(Grab::Voting);
        }
        if drop.protected_user_id.is_some()
            && drop.protected_slot == Some(slot as i16)
            && drop.protected_user_id != Some(sql_id(user))
            && drop.protected_until.is_some_and(|until| until > now)
        {
            return Ok(Grab::Protected);
        }
        let (claims,): (i32,) = sqlx::query_as(
            "INSERT INTO collection_daily (user_id, day, game, plays) VALUES ($1, $2, 'critter_claim', 1)
             ON CONFLICT (user_id, day, game) DO UPDATE SET plays = collection_daily.plays + 1
             RETURNING plays",
        )
        .bind(sql_id(user))
        .bind(now.timestamp().div_euclid(86_400) as i32)
        .fetch_one(&mut *tx)
        .await?;
        if claims > CLAIMS_PER_DAY {
            return Ok(Grab::Limit);
        }
        drop.takers[slot] = sql_id(user);
        sqlx::query("UPDATE collection_drops SET takers = $2 WHERE id = $1")
            .bind(drop_id)
            .bind(&drop.takers)
            .execute(&mut *tx)
            .await?;
        let paid = paid_play(&mut tx, user, daily).await?;
        let granted = grant(&mut tx, user, award, paid, "critter_drop").await?;
        if glimmer {
            sqlx::query("UPDATE dex_entries SET glimmer = glimmer + 1 WHERE user_id = $1 AND entry_id = $2")
                .bind(sql_id(user))
                .bind(award.entry)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(Grab::Taken {
            granted,
            drop: Box::new(drop),
            paid,
        })
    }

    /// `user` opens the bottle and finds `prize`. The prize pays in the same transaction that marks the
    /// bottle opened, so two openers never both get it.
    pub async fn open_bottle(
        &self,
        drop_id: i64,
        user: u64,
        now: DateTime<Utc>,
        prize: BottlePrize<'_>,
        daily: Daily<'_>,
    ) -> anyhow::Result<BottleOpen> {
        retry("open_bottle", || {
            self.open_bottle_once(drop_id, user, now, prize, daily)
        })
        .await
    }

    /// One attempt of [`Db::open_bottle`]: the opener's lock, then the bottle's row.
    async fn open_bottle_once(
        &self,
        drop_id: i64,
        user: u64,
        now: DateTime<Utc>,
        prize: BottlePrize<'_>,
        daily: Daily<'_>,
    ) -> anyhow::Result<BottleOpen> {
        let mut tx = self.pool().begin().await?;
        lock_users(&mut tx, &[sql_id(user)]).await?;
        let drop: Option<DropRow> = sqlx::query_as(concat!(
            "SELECT ",
            drop_columns!(),
            " FROM collection_drops WHERE id = $1 FOR UPDATE"
        ))
        .bind(drop_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(mut drop) = drop.filter(|d| d.kind == BOTTLE) else {
            return Ok(BottleOpen::Gone);
        };
        if drop.takers.first().is_some_and(|t| *t < 0) {
            return Ok(BottleOpen::Gone);
        }
        // Somebody opened it first: opening closes the bottle, so this must be told before it is called over.
        if drop.takers.first().is_some_and(|t| *t != 0) {
            return Ok(BottleOpen::Lost {
                by: drop.takers[0] as u64,
            });
        }
        if !drop.is_open() || drop.expires_at <= now {
            return Ok(BottleOpen::Gone);
        }
        drop.takers[0] = sql_id(user);
        sqlx::query("UPDATE collection_drops SET takers = $2, status = 'closed' WHERE id = $1")
            .bind(drop_id)
            .bind(&drop.takers)
            .execute(&mut *tx)
            .await?;
        drop.status = "closed".to_owned();
        let paid = paid_play(&mut tx, user, daily).await?;
        let (mut shells, mut fragments, mut granted) = (0, 0, None);
        match prize {
            BottlePrize::Shells(n) | BottlePrize::Treasure(n) if paid => shells = n,
            BottlePrize::Fragments(n) if paid => fragments = n,
            BottlePrize::Find(award) => granted = Some(grant(&mut tx, user, award, paid, "bottle").await?),
            _ => {}
        }
        if shells > 0 {
            earn(&mut tx, user, shells, "bottle").await?;
        }
        if fragments > 0 {
            add_fragments(&mut tx, user, fragments).await?;
        }
        tx.commit().await?;
        Ok(BottleOpen::Opened {
            drop: Box::new(drop),
            shells,
            fragments,
            granted,
            paid,
        })
    }

    /// Closes the drops whose time is up (once each: whoever gets the row back tells the channel).
    pub async fn close_due_drops(&self, now: DateTime<Utc>, limit: i64) -> anyhow::Result<Vec<DropRow>> {
        Ok(sqlx::query_as::<_, DropRow>(concat!(
            "UPDATE collection_drops SET status = 'closed'
             WHERE id IN (SELECT id FROM collection_drops WHERE status = 'open' AND expires_at <= $1
                          ORDER BY expires_at LIMIT $2 FOR UPDATE SKIP LOCKED)
             RETURNING ",
            drop_columns!()
        ))
        .bind(now)
        .bind(limit)
        .fetch_all(self.pool())
        .await?)
    }

    /// Forgets drops that closed before `before`, and play counts of days before `day`.
    ///
    /// A sweep over the rows of many players cannot take their locks first, so it locks the rows it deletes
    /// with `SKIP LOCKED`: one somebody holds is left for the next sweep (see the `locks` module).
    pub async fn forget_old_collection(&self, before: DateTime<Utc>, day: i32) -> anyhow::Result<u64> {
        retry("forget_old_collection", || async move {
            let drops = sqlx::query(
                "WITH old AS MATERIALIZED (
                     SELECT id FROM collection_drops WHERE status = 'closed' AND expires_at < $1
                     FOR UPDATE SKIP LOCKED)
                 DELETE FROM collection_drops WHERE id IN (SELECT id FROM old)",
            )
            .bind(before)
            .execute(self.pool())
            .await?
            .rows_affected();
            let days = sqlx::query(
                "WITH old AS MATERIALIZED (
                     SELECT user_id, day, game FROM collection_daily WHERE day < $1 FOR UPDATE SKIP LOCKED)
                 DELETE FROM collection_daily d USING old
                 WHERE d.user_id = old.user_id AND d.day = old.day AND d.game = old.game",
            )
            .bind(day)
            .execute(self.pool())
            .await?
            .rows_affected();
            Ok(drops + days)
        })
        .await
    }
}

/// Makes sure the player has a row and locks it; the first column is `comb_ready_at`, or the wait the
/// player had left when they deleted their data if that ends later.
async fn lock_player(tx: &mut Tx<'_>, user: u64) -> anyhow::Result<Option<(Option<DateTime<Utc>>,)>> {
    sqlx::query("INSERT INTO collection_players (user_id) VALUES ($1) ON CONFLICT (user_id) DO NOTHING")
        .bind(sql_id(user))
        .execute(&mut **tx)
        .await?;
    Ok(sqlx::query_as(
        "SELECT GREATEST(comb_ready_at, (SELECT comb_ready_at FROM deleted_users WHERE user_id = $1))
         FROM collection_players WHERE user_id = $1 FOR UPDATE",
    )
    .bind(sql_id(user))
    .fetch_optional(&mut **tx)
    .await?)
}

/// Counts a play of a game today and says whether it still pays.
async fn paid_play(tx: &mut Tx<'_>, user: u64, daily: Daily<'_>) -> anyhow::Result<bool> {
    let (plays,): (i32,) = sqlx::query_as(
        "INSERT INTO collection_daily (user_id, day, game, plays) VALUES ($1, $2, $3, 1)
         ON CONFLICT (user_id, day, game) DO UPDATE SET plays = collection_daily.plays + 1
         RETURNING plays",
    )
    .bind(sql_id(user))
    .bind(daily.day)
    .bind(daily.game)
    .fetch_one(&mut **tx)
    .await?;
    Ok(plays <= daily.cap)
}

/// Files `award.entry` in the dex and, when `paid`, pays what it is worth: the discovery bonus the first
/// time, the recycled spare after that.
async fn grant(tx: &mut Tx<'_>, user: u64, award: Award<'_>, paid: bool, reason: &str) -> anyhow::Result<Granted> {
    let (caught,): (i32,) = sqlx::query_as(
        "INSERT INTO dex_entries (user_id, entry_id) VALUES ($1, $2)
         ON CONFLICT (user_id, entry_id) DO UPDATE SET caught = dex_entries.caught + 1, kept = dex_entries.kept + 1
         RETURNING caught",
    )
    .bind(sql_id(user))
    .bind(award.entry)
    .fetch_one(&mut **tx)
    .await?;
    let new = caught == 1;
    movement(
        tx,
        sql_id(user),
        &format!("collection:{}", award.entry),
        1,
        "collection_reward",
    )
    .await?;
    let (shells, fragments) = match (paid, new) {
        (false, _) => (0, 0),
        (true, true) => (award.new_shells, award.bonus_fragments),
        (true, false) => (award.dup_shells, award.dup_fragments + award.bonus_fragments),
    };
    if shells > 0 {
        earn(tx, user, shells, reason).await?;
    }
    if fragments > 0 {
        add_fragments(tx, user, fragments).await?;
    }
    Ok(Granted {
        entry: award.entry.to_owned(),
        new,
        caught,
        shells,
        fragments,
    })
}

async fn earn(tx: &mut Tx<'_>, user: u64, shells: i64, reason: &str) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO users (id, credits) VALUES ($1, $2)
         ON CONFLICT (id) DO UPDATE SET credits = users.credits + EXCLUDED.credits",
    )
    .bind(sql_id(user))
    .bind(shells)
    .execute(&mut **tx)
    .await?;
    ledger(tx, sql_id(user), shells, reason).await?;
    movement(tx, sql_id(user), "shells", shells, "collection_reward").await
}

async fn add_fragments(tx: &mut Tx<'_>, user: u64, fragments: i64) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO collection_players (user_id, fragments) VALUES ($1, $2)
         ON CONFLICT (user_id) DO UPDATE SET fragments = collection_players.fragments + EXCLUDED.fragments",
    )
    .bind(sql_id(user))
    .bind(fragments)
    .execute(&mut **tx)
    .await?;
    movement(tx, sql_id(user), "fragments", fragments, "collection_reward").await
}

mod dex;
mod rewards;
mod shore;

pub use dex::{CollectionRoll, CollectionSettings, DexRow, RollClaim, RollStart, VoteCard, WishlistRow};
pub use rewards::{Award, Daily, Granted};
pub use shore::{Comb, PotHaul, PotRow, PotUpgrade};
