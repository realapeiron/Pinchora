use super::*;

impl Db {
    /// Files what a player typed for a round, while the game is in `phase` of that round and the clock
    /// has not run out; a later entry replaces an earlier one. `Some(true)` for a first entry,
    /// `Some(false)` for a replacement, `None` when it was too late or the player is not in the game.
    pub async fn put_entry(
        &self,
        id: i64,
        round: i32,
        phase: &str,
        user: u64,
        text: &str,
        now: DateTime<Utc>,
    ) -> anyhow::Result<Option<bool>> {
        let row: Option<(bool,)> = sqlx::query_as(
            "INSERT INTO party_entries (game_id, round, user_id, text)
             SELECT g.id, $2, $4, $5 FROM party_games g
             WHERE g.id = $1 AND g.status = 'playing' AND g.phase = $3 AND g.round = $2 AND g.deadline > $6
               AND EXISTS (SELECT 1 FROM party_players p WHERE p.game_id = g.id AND p.user_id = $4)
             ON CONFLICT (game_id, round, user_id) DO UPDATE SET text = EXCLUDED.text
             RETURNING (xmax = 0)",
        )
        .bind(id)
        .bind(round)
        .bind(phase)
        .bind(sql_id(user))
        .bind(text)
        .bind(now)
        .fetch_optional(self.pool())
        .await?;
        Ok(row.map(|(fresh,)| fresh))
    }

    /// The entries of a round, in an order that does not tell who wrote what (it is stable, so numbers
    /// shown to voters mean the same thing every time). The game's creation time, to the microsecond,
    /// salts it: the game id and the players' ids are in plain sight, so they alone would let anyone work
    /// the order out.
    pub async fn party_entries(&self, id: i64, round: i32) -> anyhow::Result<Vec<PartyEntry>> {
        Ok(sqlx::query_as::<_, PartyEntry>(
            "SELECT e.round, e.user_id, e.text FROM party_entries e JOIN party_games g ON g.id = e.game_id
             WHERE e.game_id = $1 AND e.round = $2
             ORDER BY md5(g.created_at::text || ':' || e.round::text || ':' || e.user_id::text)",
        )
        .bind(id)
        .bind(round)
        .fetch_all(self.pool())
        .await?)
    }

    /// Every entry of a game, oldest round first: the words Hot Pearl has used, the phrases written.
    pub async fn party_all_entries(&self, id: i64) -> anyhow::Result<Vec<PartyEntry>> {
        Ok(sqlx::query_as::<_, PartyEntry>(
            "SELECT round, user_id, text FROM party_entries WHERE game_id = $1 ORDER BY round, user_id",
        )
        .bind(id)
        .fetch_all(self.pool())
        .await?)
    }

    /// Votes for the entry of `pick` in the voting phase of `round`.
    pub async fn put_vote(
        &self,
        id: i64,
        round: i32,
        voter: u64,
        pick: u64,
        now: DateTime<Utc>,
    ) -> anyhow::Result<PartyVote> {
        let Some(game) = self.party(id).await? else {
            return Ok(PartyVote::Closed);
        };
        if !game.is_playing() || game.phase != "vote" || game.round != round || game.late(now) {
            return Ok(PartyVote::Closed);
        }
        let (me, entry, previous): (bool, bool, Option<i64>) = sqlx::query_as(
            "SELECT EXISTS (SELECT 1 FROM party_players WHERE game_id = $1 AND user_id = $3),
                    EXISTS (SELECT 1 FROM party_entries WHERE game_id = $1 AND round = $2 AND user_id = $4),
                    (SELECT pick FROM party_votes WHERE game_id = $1 AND round = $2 AND voter_id = $3)",
        )
        .bind(id)
        .bind(round)
        .bind(sql_id(voter))
        .bind(sql_id(pick))
        .fetch_one(self.pool())
        .await?;
        if !me {
            return Ok(PartyVote::NotPlayer);
        }
        if voter == pick {
            return Ok(PartyVote::OwnEntry);
        }
        if !entry {
            return Ok(PartyVote::NoEntry);
        }
        if previous == Some(sql_id(pick)) {
            return Ok(PartyVote::Same);
        }
        sqlx::query(
            "INSERT INTO party_votes (game_id, round, voter_id, pick) VALUES ($1, $2, $3, $4)
             ON CONFLICT (game_id, round, voter_id) DO UPDATE SET pick = EXCLUDED.pick",
        )
        .bind(id)
        .bind(round)
        .bind(sql_id(voter))
        .bind(sql_id(pick))
        .execute(self.pool())
        .await?;
        Ok(if previous.is_some() {
            PartyVote::Changed
        } else {
            PartyVote::Counted
        })
    }

    /// Who voted for whose entry in a round, as (voter, author).
    pub async fn party_votes(&self, id: i64, round: i32) -> anyhow::Result<Vec<(u64, u64)>> {
        let rows: Vec<(i64, i64)> = sqlx::query_as(
            "SELECT voter_id, pick FROM party_votes WHERE game_id = $1 AND round = $2 ORDER BY voter_id",
        )
        .bind(id)
        .bind(round)
        .fetch_all(self.pool())
        .await?;
        Ok(rows.into_iter().map(|(v, p)| (v as u64, p as u64)).collect())
    }
}
