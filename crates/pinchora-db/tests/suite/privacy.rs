//! Runs against a real PostgreSQL when `PINCHORA_TEST_DATABASE_URL` is set; skipped otherwise.
//!
//! Privacy choices, "delete my data", the weekly and monthly rewards, reminders and the monthly
//! numbers behind Wrapped.

use chrono::{Duration, NaiveDate, Utc};
use pinchora_core::{
    economy::{Period, daily_reward},
    events::default_options,
};
use pinchora_db::{
    ANONYMISED, Award, CLEARED_COLUMNS, Comb, Counter, DAILY_LIMITS, DELETED_NAME, Daily, DailyOutcome, Db, DerbyJoin,
    DuelStart, EventDraft, GiveawayOpen, Hide, ID_LISTS, ListEdit, NewGame, NewGiveaway, OTHER_TABLES, PENDING,
    PeriodOutcome, RepOutcome, RiotLink, USER_COLUMNS, VisitOutcome, XpGrant, ZEROED_COLUMNS,
};

use crate::support::db_eight as db;

/// Ids unique per run, and larger than any earlier run's, so the suite can run repeatedly
/// against one database.
use crate::support::ids;

/// Postgres keeps microseconds, so a time that is compared with one it stored is cut to them.
fn micros(t: chrono::DateTime<Utc>) -> chrono::DateTime<Utc> {
    chrono::DateTime::from_timestamp_micros(t.timestamp_micros()).expect("a time")
}

async fn run(db: &Db, sql: &'static str, user: u64, other: u64) {
    sqlx::query(sql)
        .bind(user as i64)
        .bind(other as i64)
        .execute(db.pool())
        .await
        .unwrap_or_else(|e| panic!("{sql}: {e}"));
}

/// How many rows of `table` name `user` in `column`.
async fn count(db: &Db, table: &str, column: &str, user: u64) -> i64 {
    let sql = format!("SELECT count(*) FROM {table} WHERE {column} = $1");
    let (n,): (i64,) = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(user as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
    n
}

/// Fills every table that can name `user` (with `other` on the far side of the ones that name two
/// players), deletes `user`, and checks nothing is left. Returns the columns it filled.
async fn fill_and_delete(db: &Db, user: u64, other: u64, guild: u64) -> Vec<(&'static str, &'static str)> {
    db.tracker_user_cache_put(
        user,
        &format!("riot:privacy:{user}"),
        &serde_json::json!({"title":"Private#TR"}),
    )
    .await
    .unwrap();
    db.guild_settings(user).await.unwrap();
    db.save_riot_link(
        user,
        "lol",
        &RiotLink {
            puuid: format!("privacy-puuid-{user}"),
            game_name: "Privacy".into(),
            tag_line: "TEST".into(),
            region: "euw1".into(),
        },
    )
    .await
    .unwrap();
    // Through the game's own calls: the account, the ledger, chat XP and the monthly numbers.
    db.earn_shells(user, 500, "test").await.unwrap();
    db.earn_shells(other, 500, "test").await.unwrap();
    let grant = XpGrant {
        server_xp: 20,
        global_xp: 20,
        points: 2,
    };
    db.add_xp(guild, user, grant, NaiveDate::from_ymd_opt(2026, 1, 5).unwrap())
        .await
        .unwrap();
    db.guild_settings(guild).await.unwrap();
    sqlx::query("INSERT INTO member_voice_levels (guild_id,user_id,xp) VALUES ($1,$2,1)")
        .bind(guild as i64)
        .bind(user as i64)
        .execute(db.pool())
        .await
        .unwrap();
    db.add_reaction_xp(guild, user, user, 5, NaiveDate::from_ymd_opt(2026, 1, 5).unwrap())
        .await
        .unwrap();
    db.schedule_level_reset(guild, user, Utc::now() + Duration::minutes(5))
        .await
        .unwrap();
    db.set_birthday(
        user,
        &pinchora_db::BirthdayInput {
            month: 3,
            day: 9,
            year: Some(1999),
            tz: "UTC",
            public: true,
            show_age: true,
        },
    )
    .await
    .unwrap();
    db.claim_birthday(guild, user, 2026, 1, true, None).await.unwrap();
    db.claim_period(user, Period::Weekly, Utc::now()).await.unwrap();
    db.set_optin(user, "daily", true).await.unwrap();
    db.add_text_reminder(user, "water the plants", Utc::now() + Duration::hours(1))
        .await
        .unwrap();
    db.set_privacy(user, Hide::Profile, true).await.unwrap();
    db.set_dm_opt_out(user, true).await.unwrap();
    db.log_catch(user, "minnow", 4.0).await.unwrap();
    db.remember_name(user, "Pinch").await.unwrap();
    db.remember_name(other, "Other").await.unwrap();
    db.pair_partners(user, other).await.unwrap();
    db.befriend(user, other).await.unwrap();
    let start = Utc::now() + Duration::hours(2);
    let event = db
        .create_event(
            guild,
            &EventDraft {
                title: "Privacy test".into(),
                description: String::new(),
                starts_at: start,
                ends_at: start + Duration::hours(1),
                tz: "UTC".into(),
                location: String::new(),
                channel_id: None,
                host_id: Some(user as i64),
                options: default_options(),
                required_role_id: None,
                reminders: vec![60, 15],
                recurrence: None,
            },
        )
        .await
        .unwrap();
    db.rsvp_event(guild, event.id, user, "going").await.unwrap();
    sqlx::query("INSERT INTO event_reminder_deliveries(event_id,user_id,minutes) VALUES ($1,$2,60)")
        .bind(event.id)
        .bind(user as i64)
        .execute(db.pool())
        .await
        .unwrap();

    // A wager both players put money on: deleting one player must not cost the other their bet.
    let game = |host: u64, guest: u64, bet: i64| NewGame {
        kind: "connect4",
        guild_id: guild,
        channel_id: crate::support::id(),
        host,
        guest: Some(guest),
        bet,
        status: PENDING,
        state: "start",
        ttl_secs: 600,
    };
    let session = db.create_game(game(user, other, 100)).await.unwrap();
    let DuelStart::Started(_) = db.start_duel(session.id, other, (8, 9), "go", 600).await.unwrap() else {
        panic!("the guest accepts");
    };
    assert_eq!(db.user(other).await.unwrap().credits, 400, "the bet is held");
    // And a challenge the other player sent them.
    db.create_game(game(other, user, 0)).await.unwrap();
    // A giveaway they host, with its prize held from them.
    let before = db.user(user).await.unwrap().credits;
    let GiveawayOpen::Started(hosted) = db
        .create_giveaway(NewGiveaway {
            guild_id: guild,
            channel_id: 1,
            host: user,
            prize: "Shells",
            shells: 50,
            winners: 2,
            required_role: None,
            ends_at: Utc::now() + Duration::hours(1),
        })
        .await
        .unwrap()
    else {
        panic!("the giveaway starts");
    };
    assert_eq!(db.user(user).await.unwrap().credits, before - 100, "the prize is held");
    // A derby they race in for fun, and one they host where the other player staked shells.
    let derby = |host: u64, channel: u64, bet: i64| NewGame {
        kind: "derby",
        guild_id: guild,
        channel_id: channel,
        host,
        guest: None,
        bet,
        status: PENDING,
        state: "7",
        ttl_secs: 600,
    };
    let theirs = db
        .open_party_game(derby(other, guild, 0))
        .await
        .unwrap()
        .expect("a lobby");
    let joined = db.derby_join(theirs.id, user, "Pinch", 0, 8, 1_000).await.unwrap();
    assert_eq!(joined, DerbyJoin::Joined);
    let hosted_derby = db
        .open_party_game(derby(user, guild + 1, 20))
        .await
        .unwrap()
        .expect("a lobby");
    let joined = db
        .derby_join(hosted_derby.id, other, "Other", 1, 8, 1_000)
        .await
        .unwrap();
    assert_eq!(joined, DerbyJoin::Joined);
    assert_eq!(db.user(other).await.unwrap().credits, 380, "the stake is held");
    sqlx::query("INSERT INTO game_answers (session_id, user_id, choice) VALUES ($1, $2, 1), ($1, $3, 2)")
        .bind(session.id)
        .bind(user as i64)
        .bind(other as i64)
        .execute(db.pool())
        .await
        .unwrap();

    // Everything else, with the player on each side of every table that names two players.
    // ($1 is the player, $2 someone else.)
    let inserts: &[&str] = &[
        // Moderation evidence and restriction records cover each affected member, author and reviewer.
        "INSERT INTO mod_notes(guild_id,user_id,author_id,body) VALUES($1,$1,$2,'Note'),($1,$2,$1,'Authored note')",
        "INSERT INTO mod_reports(guild_id,user_id,reporter_id,reviewer_id,body) VALUES($1,$1,$2,$2,'Target'),($1,$2,$1,$2,'Reporter'),($1,$2,$2,$1,'Reviewer')",
        "INSERT INTO mod_appeals(guild_id,case_number,user_id,reviewer_id,body) VALUES($1,1,$1,$2,'Own appeal body'),($1,1,$2,$1,'Reviewed appeal body')",
        "INSERT INTO mod_restrictions(guild_id,user_id,role_id,role_ids,kind) VALUES($1,$1,$2,ARRAY[$2::BIGINT],'mute')",
        "INSERT INTO mod_voice_bans(guild_id,user_id) VALUES($2,$1)",
        "INSERT INTO mod_report_blocks(guild_id,user_id) VALUES($2,$1)",
        "INSERT INTO mod_booster_roles(guild_id,user_id,role_id) VALUES($1,$1,$2)",
        "INSERT INTO roblox_links(user_id,roblox_id,roblox_name) VALUES($1,$1,'Crab')",
        "INSERT INTO roblox_pending(user_id,roblox_id,roblox_name,code,expires_at) VALUES($1,$1,'Crab','pinchora-crab-ab12',now()+interval '10 minutes')",
        "INSERT INTO roblox_sync(guild_id,user_id) VALUES($1,$1)",
        "INSERT INTO afk (guild_id, user_id, message) VALUES ($1, $1, 'away')",
        "INSERT INTO sticky_messages (channel_id, guild_id, content, updated_by) VALUES ($1, $1, 'note', $1)",
        "INSERT INTO scheduled_messages (guild_id, channel_id, content, schedule, next_run_at, created_by)
         VALUES ($1, $1, 'note', '{\"kind\":\"once\"}', now() + INTERVAL '1 hour', $1)",
        "INSERT INTO inventory (user_id, item_id, quantity) VALUES ($1, 'chair', 2)",
        "INSERT INTO houses (user_id) VALUES ($1)",
        "INSERT INTO rooms (user_id, idx, room_type, wallpaper, floor) VALUES ($1, 0, 'cottage', 'plain', 'oak')",
        "INSERT INTO placements (user_id, room_idx, item_id, x, y) VALUES ($1, 0, 'chair', 1, 1)",
        "INSERT INTO visits (host_id, guest_id, day) VALUES ($1, $2, current_date), ($2, $1, current_date)",
        "INSERT INTO stall_listings (seller_id, item_id, price) VALUES ($1, 'chair', 50)",
        "INSERT INTO user_backgrounds (user_id, background_id) VALUES ($1, 'beach')",
        "INSERT INTO user_badges (user_id, badge_id) VALUES ($1, 'star')",
        "INSERT INTO user_titles (user_id, title_id) VALUES ($1, 'angler')",
        "INSERT INTO guestbook_notes (host_id, author_id, author_name, day, body)
         VALUES ($1, $2, 'Other', current_date, 'hi'), ($2, $1, 'Pinch', current_date, 'hello')",
        "INSERT INTO guestbook_blocks (host_id, blocked_id, blocked_name)
         VALUES ($1, $2, 'Other'), ($2, $1, 'Pinch')",
        "INSERT INTO house_likes (host_id, liker_id, day) VALUES ($1, $2, current_date), ($2, $1, current_date)",
        "INSERT INTO gifts (from_id, to_id, item_id) VALUES ($1, $2, 'chair'), ($2, $1, 'lamp')",
        "INSERT INTO interactions (kind, from_user, to_user, times) VALUES ('hug', $1, $2, 3), ('hug', $2, $1, 1)",
        "INSERT INTO pets (owner_id, species, name) VALUES ($1, 'crab', 'Pinchy')",
        "INSERT INTO game_stats (user_id, game, played, won) VALUES ($1, 'connect4', 2, 1)",
        "INSERT INTO wordle_games (user_id, mode, day, word) VALUES ($1, 'daily', 1, 'coral')",
        "INSERT INTO contest_entries (guild_id, week, user_id, owner_name, room_idx, tier, room_type, wallpaper, floor)
         VALUES (1, 1, $1, 'Pinch', 0, 1, 'cottage', 'plain', 'oak'), (1, 1, $2, 'Other', 0, 1, 'cottage', 'plain', 'oak')",
        "INSERT INTO contest_pieces (guild_id, week, user_id, n, item_id, x, y, flipped)
         VALUES (1, 1, $1, 0, 'chair', 1, 1, false)",
        "INSERT INTO contest_votes (guild_id, week, entry_id, voter_id) VALUES (1, 1, $2, $1), (1, 1, $1, $2)",
        "INSERT INTO contest_results (guild_id, week, winner_ids, winner_votes, entries)
         VALUES (1, 1, ARRAY[$1, $2], ARRAY[3, 2], 2)
         ON CONFLICT (guild_id, week) DO UPDATE SET winner_ids = EXCLUDED.winner_ids",
        "INSERT INTO activity_monthly (user_id, month, visits) VALUES ($1, '2025-01-01', 4) ON CONFLICT DO NOTHING",
        "INSERT INTO fishing_gear (user_id, spot, rod, bait) VALUES ($1, 'shore', 'driftwood_rod', 'worm')",
        "INSERT INTO user_rods (user_id, rod_id) VALUES ($1, 'reed_rod')",
        "INSERT INTO user_baits (user_id, bait_id, qty) VALUES ($1, 'worm', 3)",
        "INSERT INTO fish_bucket (user_id, fish_id, size_cm) VALUES ($1, 'minnow', 4.5)",
        "INSERT INTO fish_milestones (user_id, species) VALUES ($1, 5)",
        // ($1 doubles as the server here too.)
        "INSERT INTO weekly_catch (guild_id, week, user_id, fish_id, size_cm, ratio)
         VALUES ($1, current_date, $1, 'minnow', 4.5, 1.1)",
        // Somebody else's giveaway the player entered and won ($1 doubles as the server).
        "INSERT INTO giveaways (guild_id, channel_id, host_id, prize, shells, winners, ends_at, status, winner_ids, announced)
         VALUES ($1, 2, $2, 'Hat', 0, 2, now(), 'ended', ARRAY[$1, $2], TRUE)",
        "INSERT INTO giveaway_entries (giveaway_id, user_id)
         SELECT id, $1 FROM giveaways WHERE guild_id = $1 AND prize = 'Hat'
         UNION ALL SELECT id, $2 FROM giveaways WHERE guild_id = $1 AND prize = 'Hat'",
        "INSERT INTO suggestions (guild_id, number, author_id, body, status, decided_by)
         VALUES ($1, 1, $1, 'Add a garden', 'approved', $2), ($1, 2, $2, 'Remove it', 'denied', $1)",
        "INSERT INTO suggestion_votes (suggestion_id, user_id, up)
         SELECT id, $1, TRUE FROM suggestions WHERE guild_id = $1 AND number = 2",
        "INSERT INTO polls (guild_id, channel_id, author_id, question, options, ends_at)
         VALUES ($1, 2, $1, 'Mine?', ARRAY['a', 'b'], now() + interval '1 day'),
                ($1, 2, $2, 'Theirs?', ARRAY['a', 'b'], now() + interval '1 day')",
        "INSERT INTO poll_votes (poll_id, user_id, choice)
         SELECT id, $1, 0 FROM polls WHERE guild_id = $1 AND author_id = $2",
        "INSERT INTO poll_extra_votes (poll_id, user_id, choice)
         SELECT id, $1, 1 FROM polls WHERE guild_id = $1 AND author_id = $2",
        "INSERT INTO personal_recurring_reminders(user_id,body,interval_minutes,next_at)
         VALUES($1,'Remember privately',60,now()+interval '1 hour')",
        "INSERT INTO personal_checklist_items(user_id,body) VALUES($1,'Private task')",
        "INSERT INTO keyword_highlights(guild_id,user_id,keyword) VALUES($1,$1,'coral')",
        "INSERT INTO member_anniversaries(guild_id,user_id,joined_at) VALUES($1,$1,now()-interval '1 year')",
        "INSERT INTO anniversary_sent(guild_id,user_id,year) VALUES($1,$1,2026)",
        "INSERT INTO confessions(guild_id,author_id,body,reviewer_id)
         VALUES($1,$1,'My private confession',$2),($1,$2,'Someone else wrote this',$1)",
        "INSERT INTO confession_bans(guild_id,user_id) VALUES($1,$1)",
        "INSERT INTO confession_reports(confession_id,reporter_id) SELECT id,$1 FROM confessions WHERE guild_id=$1 AND author_id=$2",
        "INSERT INTO confession_moderation(guild_id,confession_id,moderator_id,banned) SELECT guild_id,id,$1,true FROM confessions WHERE guild_id=$1 AND author_id=$2",
        "INSERT INTO confession_reveals(guild_id,confession_id,owner_id)
         SELECT guild_id,id,$1 FROM confessions WHERE guild_id=$1 AND author_id=$2",
        // What roles they held when they left, a role that lapses, and one that waits for them ($1 doubles as the server).
        "INSERT INTO sticky_roles (guild_id, user_id, role_ids, left_at) VALUES ($1, $1, ARRAY[5, 6], now())",
        "INSERT INTO timed_roles (guild_id, user_id, role_id, remove_at) VALUES ($1, $1, 5, now() + interval '1 day')",
        "INSERT INTO delayed_roles (guild_id, user_id, role_id, grant_at) VALUES ($1, $1, 6, now() + interval '1 day')",
        // Closed counterpart records exercise every privacy column without live escrow.
        "WITH m AS MATERIALIZED (SELECT nextval('economy_movement_ids') AS id)
         INSERT INTO economy_ledger(movement,user_id,account,asset,delta,reason)
         SELECT id,$1,'wallet','shells',1,'fixture' FROM m UNION ALL SELECT id,$1,'source','shells',-1,'fixture' FROM m",
        "INSERT INTO economy_players(user_id) VALUES($1)",
        "INSERT INTO economy_requests(user_id,request_id,result) VALUES($1,'privacy-fixture','{}')",
        "INSERT INTO economy_auctions(seller_id,bidder_id,item_id,start_price,bid,ends_at,hard_end,status)
         VALUES($1,$2,'test-chair',1,1,now(),now(),'sold'),($2,$1,'test-chair',1,1,now(),now(),'sold')",
        "INSERT INTO economy_trades(user_a,user_b,offer_a,offer_b,expires_at,status)
         VALUES($1,$2,'{}','{}',now(),'completed'),($2,$1,'{}','{}',now(),'completed')",
        // The character, the collection and the solo boards.
        "INSERT INTO user_accessories (user_id, accessory_id) VALUES ($1, 'cap')",
        "INSERT INTO character_outfits (user_id, slot, name, code) VALUES ($1, 1, 'Beach', 'x')",
        "INSERT INTO dex_entries (user_id, entry_id) VALUES ($1, 'shrimp')",
        "INSERT INTO collection_wishlist (user_id, entry_id, ping_on_spawn) VALUES ($1, 'shrimp', true)",
        "INSERT INTO collection_shard_ledger (user_id, request_id, kind, entry_id, delta, balance) VALUES ($1, 'privacy', 'recycle', 'shrimp', 2, 2)",
        "INSERT INTO collection_rolls (user_id, entry_id, rolled_at) VALUES ($1, 'shrimp', now())",
        "INSERT INTO collection_players (user_id, fragments, comb_ready_at)
         VALUES ($1, 5, now() + interval '10 minutes')",
        "INSERT INTO picross_games (user_id, mode, day, picture, variant, grid, status)
         VALUES ($1, 'daily', 1, 0, 0, '.', 'solved'), ($1, 'practice', 0, 0, 0, '.', 'playing')",
        "INSERT INTO solo_games (user_id, game, state) VALUES ($1, '2048', '{}')",
        "INSERT INTO fleet_drafts (session_id, user_id, layout)
         SELECT id, $1, 'x' FROM game_sessions WHERE host_id = $1 AND kind = 'connect4' AND guest_id = $2",
        // A critter drop they called and took a card from ($1 doubles as the server).
        "INSERT INTO collection_drops (kind, guild_id, channel_id, host_id, cards, takers, expires_at)
         VALUES ('critters', $1, 1, $1, ARRAY['a', 'b', 'c'], ARRAY[$1, $2, 0], now() + interval '1 minute')",
        "INSERT INTO collection_word_attempts (drop_id, user_id, attempts)
         SELECT id, $1, 1 FROM collection_drops WHERE host_id = $1",
        "INSERT INTO collection_votes (drop_id, user_id, slot)
         SELECT id, $1, 0 FROM collection_drops WHERE guild_id = $1 AND host_id = $1",
        // Room ownership, hosting, participation and exact request receipts.
        "INSERT INTO reef_games(room_key,owner_id,host_id,kind,request_id,state,finished,deadline)
         VALUES ('house:'||$1::bigint||':0',$1,$2,'chess','privacy-owner','{}',true,now()),
                ('house:'||$2::bigint||':0',$2,$1,'chess','privacy-host','{}',true,now())",
        "INSERT INTO reef_game_players(game_id,user_id,side)
         SELECT id,$1,0 FROM reef_games WHERE owner_id=$1 OR host_id=$1",
        "INSERT INTO reef_game_actions(game_id,user_id,request_id,payload)
         SELECT id,$1,'privacy','{}' FROM reef_games WHERE owner_id=$1 OR host_id=$1",
        "INSERT INTO game_reward_daily(user_id,day,game,wins)
         VALUES ($1,floor(extract(epoch FROM now())/86400)::int,'chess',2),($1,1,'chess',1)",
        // The day's limits, and an older day's.
        "INSERT INTO collection_daily (user_id, day, game, plays)
         VALUES ($1, floor(extract(epoch FROM now()) / 86400)::int, 'beachcomb', 3), ($1, 1, 'beachcomb', 1)",
        "INSERT INTO party_daily (user_id, day, game, plays, paid)
         VALUES ($1, floor(extract(epoch FROM now()) / 86400)::int, 'heist', 2, 80), ($1, 1, 'heist', 1, 40)",
        // A word game they host, and one of the other player's they play in ($1 doubles as the server).
        "INSERT INTO party_games (kind, guild_id, channel_id, host_id, deadline)
         VALUES ('hotpearl', $1, 1, $1, now() + interval '1 hour'), ('acronym', $1, 1, $2, now() + interval '1 hour')",
        "INSERT INTO party_players (game_id, user_id, name, seat)
         SELECT id, $1, 'Pinch', 0 FROM party_games WHERE guild_id = $1
         UNION ALL SELECT id, $2, 'Other', 1 FROM party_games WHERE guild_id = $1",
        "INSERT INTO party_entries (game_id, round, user_id, text)
         SELECT id, 0, $1, 'pearl' FROM party_games WHERE guild_id = $1 AND host_id = $2
         UNION ALL SELECT id, 0, $2, 'shell' FROM party_games WHERE guild_id = $1 AND host_id = $2",
        "INSERT INTO party_votes (game_id, round, voter_id, pick)
         SELECT id, 0, $1, $2 FROM party_games WHERE guild_id = $1 AND host_id = $2
         UNION ALL SELECT id, 1, $2, $1 FROM party_games WHERE guild_id = $1 AND host_id = $2",
        // A change they made on the dashboard ($1 doubles as the server).
        "INSERT INTO dashboard_audit (guild_id, actor_id, actor_name, source, action)
         VALUES ($1, $1, 'Pinch', 'web', 'settings.patch')",
        // A warn they gave as a moderator, its staff log line, and a server log that leaves them alone.
        "INSERT INTO mod_cases (guild_id, number, kind, target_id, target_name, moderator_id, moderator_name)
         VALUES ($1, 1, 'warn', $2, 'Other', $1, 'Pinch')",
        "INSERT INTO staff_actions (guild_id, actor_id, actor_name, source, action, case_number)
         VALUES ($1, $1, 'Pinch', 'command', 'warn', 1)",
        "INSERT INTO log_settings (guild_id, ignored_users) VALUES ($1, ARRAY[$1, $2])",
        // Player-owned history and counters are forgotten with the player.
        "INSERT INTO automod_hits (guild_id,user_id,channel_id,action) VALUES ($1,$1,1,'log_only')",
        "INSERT INTO automod_strikes (guild_id,user_id,window_start,count) VALUES ($1,$1,now(),1)",
        "INSERT INTO workflows (guild_id,name,enabled,trigger,conditions,actions,created_by) VALUES ($1,'privacy',true,'{\"kind\":\"button\"}','[]','[{\"kind\":\"log\",\"text\":\"ok\"}]',$1)",
        "INSERT INTO workflow_vars(guild_id,scope,subject_id,key,value) VALUES($1,'member',$1,'example','private')",
        "INSERT INTO workflow_role_changes(guild_id,user_id,role_id,added) VALUES($1,$1,1,true)",
        "INSERT INTO workflow_runs(guild_id,workflow_id,trigger_summary,user_id,outcome) SELECT $1,id,'button',$1,'completed' FROM workflows WHERE guild_id=$1 AND name='privacy'",
        // A custom command and an auto-responder they wrote ($1 doubles as the server).
        "INSERT INTO custom_commands (guild_id, name, responses, created_by) VALUES ($1, 'hello', ARRAY['hi'], $1)",
        "INSERT INTO custom_responders (guild_id, name, trigger_kind, responses, created_by)
         VALUES ($1, 'welcome', 'member_join', ARRAY['hi'], $1)",
        // What they did in a server, counted for its stats ($1 doubles as the server).
        "INSERT INTO member_activity_daily (guild_id, user_id, day, messages) VALUES ($1, $1, current_date, 3)",
        // A server achievement the player earned ($1 doubles as the server).
        "INSERT INTO server_achievements (guild_id,name,description,icon,kind,target)
         VALUES ($1,'Privacy prize','','star','custom',1)",
        "INSERT INTO member_achievements (guild_id,user_id,achievement_id)
         SELECT guild_id,$1,id FROM server_achievements WHERE guild_id=$1 AND name='Privacy prize'",
        // Invite history, a link they created, and a bonus ($1 doubles as the server).
        "INSERT INTO guild_invites (guild_id, code, inviter_id) VALUES ($1, 'private', $1)",
        "INSERT INTO invite_joins (guild_id, user_id, inviter_id, source)
         VALUES ($1, $1, $2, 'invite'), ($1, $2, $1, 'invite')",
        "INSERT INTO invite_bonus (guild_id, user_id, bonus) VALUES ($1, $1, 2)",
        "INSERT INTO invite_rewards (guild_id, invites, role_id, grantor_id) VALUES ($1, 1, 123, $1)",
        "INSERT INTO invite_role_grants (guild_id, user_id, role_id) VALUES ($1, $1, 123)",
        // A ticket they opened, and the other player's one they claimed, closed and wrote in, with its
        // transcript ($1 doubles as the server).
        "INSERT INTO tickets (guild_id, number, opener_id, status, claimed_by, closed_by)
         VALUES ($1, 1, $1, 'open', NULL, NULL), ($1, 2, $2, 'closed', $1, $1)",
        "INSERT INTO ticket_panels (guild_id,name,channel_id,title) VALUES ($1,'Privacy',1,'Privacy')",
        "INSERT INTO ticket_form_drafts (guild_id,user_id,panel_id,topic_index,answers,step,expires_at)
         SELECT $1,$1,id,0,'[]',1,now()+interval '1 hour' FROM ticket_panels WHERE guild_id=$1 AND name='Privacy'",
        "INSERT INTO ticket_staff_notes (ticket_id,author_id,body)
         SELECT id,$1,'private' FROM tickets WHERE guild_id=$1 AND number=2",
        "INSERT INTO ticket_operations (ticket_id,transferred_to_user,close_requested_by,force_opened_by)
         SELECT id,$1,$1,$1 FROM tickets WHERE guild_id=$1 AND number=2",
        "INSERT INTO ticket_transcripts (ticket_id, json, messages, with_content)
         SELECT id, '[]', 0, TRUE FROM tickets WHERE guild_id = $1",
        "INSERT INTO ticket_transcript_authors (ticket_id, idx, user_id, name)
         SELECT id, 0, $1, 'Pinch' FROM tickets WHERE guild_id = $1 AND number = 2
         UNION ALL SELECT id, 1, $2, 'Other' FROM tickets WHERE guild_id = $1 AND number = 2",
        // A feed they set up ($1 doubles as the server).
        "INSERT INTO feed_sources (kind, source) VALUES ('rss', 'https://privacy.example/' || $1::text)",
        "INSERT INTO feeds (guild_id, kind, source, channel_id, created_by)
         VALUES ($1, 'rss', 'https://privacy.example/' || $1::text, 1, $1)",
        "INSERT INTO feed_channel_links (user_id, kind, source, verified_at)
         VALUES ($1, 'twitch', 'privacy' || $1::text, now())",
        // A temp voice room they own, and the members kept out of it ($1 doubles as the room and the server).
        "INSERT INTO temp_voice_channels (channel_id, guild_id, source_id, owner_id) VALUES ($1, $1, $2, $1)",
        "INSERT INTO temp_voice_bans (channel_id, user_id) VALUES ($1, $1), ($1, $2)",
        // Stars they gave and a post they wrote, with the other player on the opposite side.
        "INSERT INTO starboards (guild_id, name, channel_id) VALUES ($1, 'Privacy', 4)",
        "INSERT INTO starboard_posts (starboard_id, message_id, channel_id, author_id)
         SELECT id, $1, 5, $1 FROM starboards WHERE guild_id=$1
         UNION ALL SELECT id, $2, 5, $2 FROM starboards WHERE guild_id=$1",
        "INSERT INTO starboard_stars (starboard_id, message_id, user_id, weight)
         SELECT id, $2, $1, 1 FROM starboards WHERE guild_id=$1
         UNION ALL SELECT id, $1, $2, 1 FROM starboards WHERE guild_id=$1",
        // Gate progress, recent joins, exemptions and the moderator who ended a raid.
        "INSERT INTO verification_pending (guild_id,user_id,joined_at) VALUES ($1,$1,now())",
        "INSERT INTO raid_joins (guild_id,user_id,joined_at,name_family,young) VALUES ($1,$1,now(),'pinch',false)",
        "INSERT INTO verification (guild_id,exempt_users) VALUES ($1,ARRAY[$1,$2])",
        "INSERT INTO raid_events (guild_id,started_at,ended_at,joins,action,ended_by,expires_at)
         VALUES ($1,now(),now(),3,'alert',$1,now())",
        "INSERT INTO antinuke_events (id,guild_id,actor_id,target_user_id,target_id,kind) VALUES ($1,$1,$1,$1,$2,'ban')",
        "INSERT INTO antinuke_role_backups (guild_id,user_id,role_ids) VALUES ($1,$1,ARRAY[$2::BIGINT])",
        "INSERT INTO antinuke_snapshots (guild_id,kind,object_id,data) VALUES ($1,'channel',$2,'{\"permission_overwrites\":[]}')",
        "INSERT INTO antinuke_overwrites (guild_id,channel_id,user_id,allow_bits,deny_bits) VALUES ($1,$2,$1,'1024','0')",
        "INSERT INTO antinuke (guild_id,trusted_users) VALUES ($1,ARRAY[$1,$2])",
        // Community pack: suggestions, counting, applications and private staff mail.
        "INSERT INTO qotd_questions(guild_id,text,suggested_by) VALUES($1,'Privacy question?',$1)",
        "INSERT INTO counting_channels(channel_id,guild_id,current,last_user_id) VALUES($1,$1,1,$1)",
        "INSERT INTO counting_stats(guild_id,user_id,correct) VALUES($1,$1,1)",
        "INSERT INTO forms(guild_id,name,questions) VALUES($1,'Privacy','[\"Why?\"]')",
        "INSERT INTO form_submissions(form_id,user_id,answers,status,decided_by)
         SELECT id,$1,'[\"Because\"]','accepted',$1 FROM forms WHERE guild_id=$1 AND name='Privacy'",
        "INSERT INTO modmail_threads(guild_id,user_id,thread_id,status,closed_at,closed_by)
         VALUES($1,$1,$1,'closed',now(),$1)",
        "INSERT INTO modmail_blocks(guild_id,user_id) VALUES($1,$1)",
        "INSERT INTO modmail_pending(user_id,body) VALUES($1,'Private text')",
        "INSERT INTO card_design_history(guild_id,kind,design,delivery,actor_id) VALUES($2,'welcome',NULL,'text_only',$1)",
        "INSERT INTO message_kit_history(guild_id,key,name,payload,version,actor_id) VALUES($2,'privacy','Privacy','{}',1,$1)",
        "INSERT INTO card_boost_state(guild_id,user_id) VALUES($2,$1)",
        "INSERT INTO card_member_visits(guild_id,user_id,visits) VALUES($2,$1,1)",

    ];
    for sql in [
        "INSERT INTO reef_room_leases(namespace,owner_id,room_idx,expires_at) VALUES('privacy-fixture',$1,0,now()+interval '3 seconds')",
        "INSERT INTO reef_consents(user_id,version,age_band,accepted) VALUES($1,1,'13_plus',true)",
        "INSERT INTO reef_chat_evidence(id,owner_id,room_idx,sender_id,text) VALUES($1,$1,0,$1,'privacy fixture')",
        "INSERT INTO reef_chat_recipients(message_id,user_id) VALUES($1,$1)",
        "INSERT INTO reef_reports(id,message_id,reporter_id,reason) VALUES($1,$1,$1,'other')",
        "INSERT INTO reef_moderator_audit(actor_id,target_user_id,action) VALUES($1,$1,'report')",
        "INSERT INTO reef_sanctions(user_id,actor_id,owner_id,kind,expires_at) VALUES($2,$2,$1,'mute',now()+interval '1 hour'),($1,$1,NULL,'ban',now()+interval '1 hour')",
    ] {
        run(db, sql, user, other).await;
    }
    for sql in inserts {
        run(db, sql, user, other).await;
    }
    for sql in [
        "INSERT INTO reef_edit_receipts(actor_id,request_id,owner_id,room_idx,payload) VALUES($1,'privacy',$1,0,'{}')",
        "INSERT INTO reef_space_preferences(user_id,presence,visits,doorbell) VALUES($1,true,'friends',true)",
        "INSERT INTO reef_doorbells(host_id,guest_id) VALUES($1,$2),($2,$1)",
        "INSERT INTO reef_loop(user_id) VALUES($1) ON CONFLICT DO NOTHING",
        "INSERT INTO reef_loop_daily(user_id,day,requests,dives,pet_reward,partner_reward)
         VALUES($1,floor(extract(epoch FROM now()) / 86400)::int,3,1,true,true),($1,1,3,1,true,true)",
        "INSERT INTO reef_aquarium(user_id,slot,fish_id,size_cm) VALUES($1,1,'toaststripe_minnow',5)",
        "INSERT INTO reef_requests(user_id,rotation) VALUES($1,1)",
        "INSERT INTO reef_catches(guild_id,day,user_id,fish_id,size_cm,score) VALUES($2,CURRENT_DATE,$1,'toaststripe_minnow',5,1)",
        "INSERT INTO reef_season_claims(user_id,season) VALUES($1,'2026-03-01')",
        "INSERT INTO reef_partner_streaks(user_a,user_b,since) SELECT user_a,user_b,since FROM partners WHERE user_a=$1 OR user_b=$1",
    ] {
        run(db, sql, user, other).await;
    }
    let mut filled = Vec::new();
    for (table, column) in USER_COLUMNS {
        if count(db, table, column, user).await > 0 {
            filled.push((*table, *column));
        }
    }

    db.roblox_cache_put(&format!("groups:{user}"), &serde_json::json!([[42, 100]]))
        .await
        .unwrap();
    let deleted = db.delete_user_data(user, Utc::now()).await.unwrap();
    assert!(db.roblox_cache(&format!("groups:{user}")).await.unwrap().is_none());
    assert!(deleted.rows >= filled.len() as u64, "{deleted:?}");
    let mut tables: Vec<&str> = filled.iter().map(|(t, _)| *t).collect();
    tables.dedup();
    assert!(deleted.tables >= tables.len(), "{deleted:?} for {tables:?}");

    for (table, column) in USER_COLUMNS {
        assert_eq!(
            count(db, table, column, user).await,
            0,
            "{table}.{column} still names the deleted player"
        );
    }
    for (table, column, _) in ID_LISTS {
        let sql = format!("SELECT count(*) FROM {table} WHERE $1 = ANY({column})");
        let (left,): (i64,) = sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(user as i64)
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(left, 0, "{table}.{column} still lists the deleted player");
    }
    let (ids, votes): (Vec<i64>, Vec<i32>) =
        sqlx::query_as("SELECT winner_ids, winner_votes FROM contest_results WHERE guild_id = 1 AND week = 1")
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(
        (ids, votes),
        (vec![0, other as i64], vec![3, 2]),
        "the podium keeps its places"
    );
    for (table, column) in CLEARED_COLUMNS {
        assert_eq!(
            count(db, table, column, user).await,
            0,
            "{table}.{column} still names the player"
        );
    }
    for (table, column, name) in ANONYMISED {
        assert_eq!(
            count(db, table, column, user).await,
            0,
            "{table}.{column} still names the player"
        );
        let sql = format!("SELECT count(*) FROM {table} WHERE guild_id = $1 AND {column} = 0 AND {name} = $2");
        let (kept,): (i64,) = sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(user as i64)
            .bind(DELETED_NAME)
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(kept, 1, "the server's record of {table} stays, without the player");
    }
    // A server's custom commands and responders stay, without the player who wrote them.
    for (table, column) in ZEROED_COLUMNS {
        assert_eq!(
            count(db, table, column, user).await,
            0,
            "{table}.{column} still names the player"
        );
        let scope = if *table == "reef_sanctions" {
            "user_id"
        } else {
            "guild_id"
        };
        let sql = format!("SELECT count(*) FROM {table} WHERE {scope} = $1 AND {column} = 0");
        let (kept,): (i64,) = sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(user as i64)
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(kept, 1, "the server's record in {table} stays, without the player");
    }
    // Their choice about direct messages from servers went with the account.
    assert!(!db.dm_opt_out(user).await.unwrap());
    // The day's limits stay, the older days' go.
    for (table, column, day) in DAILY_LIMITS {
        let sql = format!(
            "SELECT count(*) FILTER (WHERE {day} = floor(extract(epoch FROM now()) / 86400)::int), count(*)
             FROM {table} WHERE {column} = $1"
        );
        let (today, all): (i64, i64) = sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(user as i64)
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!((today, all), (1, 1), "{table} keeps today's limits only");
    }
    let (takers,): (Vec<i64>,) = sqlx::query_as("SELECT takers FROM collection_drops WHERE guild_id = $1")
        .bind(user as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(takers, [-1, other as i64, 0], "the other taker keeps their card");
    assert_eq!(
        count(db, "party_players", "user_id", other).await,
        1,
        "the other player's game goes on without them; the one they hosted is gone"
    );
    let (winners,): (Vec<i64>,) =
        sqlx::query_as("SELECT winner_ids FROM giveaways WHERE host_id = $1 AND prize = 'Hat'")
            .bind(other as i64)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(
        winners,
        [other as i64],
        "the player is taken out of the winners, not blanked"
    );
    let (decided,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM suggestions WHERE guild_id = $1 AND decided_by IS NULL")
            .bind(user as i64)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(
        decided, 1,
        "the suggestion the player decided stays, undecided by anyone"
    );
    let (kept,): (i64,) = sqlx::query_as("SELECT count(*) FROM suggestions WHERE guild_id = $1")
        .bind(user as i64)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(kept, 1, "only the other player's suggestion is left");
    let (tickets, authors): (i64, i64) = sqlx::query_as(
        "SELECT count(*), (SELECT count(*) FROM ticket_transcript_authors a JOIN tickets t ON t.id = a.ticket_id
                           WHERE t.guild_id = $1)
         FROM tickets WHERE guild_id = $1 AND claimed_by IS NULL AND closed_by IS NULL",
    )
    .bind(user as i64)
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(
        (tickets, authors),
        (1, 1),
        "their own ticket is gone; the other player's stays, its transcript without their name"
    );

    // The other player keeps their own things, and their bet is back.
    assert_eq!(
        db.user(other).await.unwrap().credits,
        500,
        "the held wager and the derby stake went back to the player left"
    );
    assert_eq!(
        db.party_members(theirs.id).await.unwrap().len(),
        0,
        "they left the other player's derby"
    );
    assert_eq!(count(db, "social_names", "user_id", other).await, 1);
    assert!(db.partner(other).await.unwrap().is_none(), "the pair is gone");
    assert_eq!(count(db, "contest_entries", "user_id", other).await, 1);
    assert!(
        db.giveaway(hosted.id).await.unwrap().is_none(),
        "the giveaway they hosted is called off"
    );
    assert_eq!(count(db, "giveaway_entries", "user_id", other).await, 1);
    assert_eq!(count(db, "polls", "author_id", other).await, 1, "their polls stay");
    assert_eq!(
        count(db, "contest_votes", "voter_id", other).await,
        0,
        "the vote on the deleted entry went with it"
    );

    // Deleting again is quiet, and the player starts over as a new one.
    assert_eq!(db.delete_user_data(user, Utc::now()).await.unwrap().rows, 0);
    assert_eq!(db.user(user).await.unwrap().credits, 0);
    filled
}

async fn futures_join(db: &Db, user: u64, now: chrono::DateTime<Utc>) -> Vec<PeriodOutcome> {
    let (a, b, c) = tokio::join!(
        db.claim_period(user, Period::Weekly, now),
        db.claim_period(user, Period::Weekly, now),
        db.claim_period(user, Period::Weekly, now),
    );
    vec![a.unwrap(), b.unwrap(), c.unwrap()]
}

/// Gives everything the player has to `alt` and deletes the player's data.
async fn hand_over(db: &Db, user: u64, alt: u64, at: chrono::DateTime<Utc>) {
    let balance = db.user(user).await.unwrap().credits;
    if balance > 0 {
        db.transfer_credits(user, alt, balance).await.unwrap();
    }
    db.delete_user_data(user, at).await.unwrap();
}

mod cooldowns;
mod data;
mod races;
