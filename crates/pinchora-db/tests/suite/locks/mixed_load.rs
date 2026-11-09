use super::*;

/// What players do at once in a busy server, deletes of their data among it, from a few workers for
/// a while (`PINCHORA_TEST_MIX_SECS`, `PINCHORA_TEST_MIX_WORKERS`). No call may fail (a deadlock
/// would reach the player as "something went wrong"), and every balance must be what its ledger
/// says, with no shells left held in a duel, a giveaway or a party game.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_mixed_load_with_deletes_fails_nobody_and_keeps_the_books() {
    let _isolation = crate::support::exclusive().await;
    let env = |name: &str, default: u64| std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default);
    let (secs, workers) = (env("PINCHORA_TEST_MIX_SECS", 15), env("PINCHORA_TEST_MIX_WORKERS", 8));
    let Some(db) = db_with(workers as u32 * 2).await else {
        return;
    };
    // Players who are deleted all the time, and a few house owners nobody deletes (placing a piece in
    // a house whose owner deleted it in between is a foreign key error, a different matter).
    let all = ids::<14>();
    let users: Arc<Vec<u64>> = Arc::new(all[..10].to_vec());
    let owners: Arc<Vec<u64>> = Arc::new(all[10..].to_vec());
    let [guild] = ids();
    for user in users.iter() {
        db.earn_shells(*user, 1_000_000, "test").await.unwrap();
    }
    let retries_before = pinchora_db::conflict_retries();
    let deadline = Instant::now() + StdDuration::from_secs(secs);
    let mut handles = Vec::new();
    for worker in 0..workers {
        let db = db.clone();
        let (users, owners) = (users.clone(), owners.clone());
        handles.push(tokio::spawn(async move {
            let mut tally = vec![Tally::default(); OPS.len()];
            let mut seed = 0x9E37_79B9_7F4A_7C15u64 ^ (worker + 1).wrapping_mul(0x2545_F491_4F6C_DD1D);
            let mut roll = move || {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                seed
            };
            let mut calls = 0u64;
            while Instant::now() < deadline {
                calls += 1;
                let i = (roll() % users.len() as u64) as usize;
                let a = users[i];
                let b = users[(i + 1 + (roll() % (users.len() as u64 - 1)) as usize) % users.len()];
                // One worker deletes every tenth call, so deletes always run, whatever the dice say.
                let op = match roll() % 200 {
                    _ if worker == 0 && calls.is_multiple_of(10) => 9,
                    0..=7 => 0,
                    8..=13 => 1,
                    14..=19 => 2,
                    20..=31 => 3,
                    32..=39 => 4,
                    40..=47 => 5,
                    48..=59 => 6,
                    60..=67 => 7,
                    68..=73 => 8,
                    74..=75 => 9,
                    76..=81 => 10,
                    82..=87 => 11,
                    88..=93 => 12,
                    94..=99 => 13,
                    100..=105 => 14,
                    106..=111 => 15,
                    112..=119 => 16,
                    120..=134 => 17,
                    135..=144 => 18,
                    145..=154 => 19,
                    155..=164 => 20,
                    165..=179 => 21,
                    180..=189 => 22,
                    _ => 23,
                };
                let day = Utc::now().date_naive();
                let result: anyhow::Result<()> = async {
                    match op {
                        0 => db.claim_daily(a).await.map(drop),
                        1 => db.claim_period(a, Period::Weekly, Utc::now()).await.map(drop),
                        2 => db.give_rep(a, b).await.map(drop),
                        3 => db.transfer_credits(a, b, 1).await.map(drop),
                        4 => {
                            sqlx::query("INSERT INTO houses (user_id) VALUES ($1) ON CONFLICT DO NOTHING")
                                .bind(b as i64)
                                .execute(db.pool())
                                .await?;
                            db.like_house(b, a, day).await.map(drop)
                        }
                        5 => db.record_visit(b, a, day, 25, 3).await.map(drop),
                        6 => {
                            let grant = XpGrant {
                                server_xp: 3,
                                global_xp: 3,
                                points: 1,
                            };
                            db.add_xp(guild, a, grant, NaiveDate::from_ymd_opt(2026, 11, 2).unwrap())
                                .await
                                .map(drop)
                        }
                        7 => {
                            let session = db.create_game(duel(a, b, 2, guild)).await?;
                            db.start_duel(session.id, b, (8, 9), "go", 600).await?;
                            db.settle_game(session.id, None, "won", Payout::Winner(a), false)
                                .await
                                .map(drop)
                        }
                        8 => {
                            let new = NewGiveaway {
                                guild_id: guild + roll() % 50,
                                channel_id: 1,
                                host: a,
                                prize: "Shells",
                                shells: 2,
                                winners: 2,
                                required_role: None,
                                ends_at: Utc::now() + Duration::hours(1),
                            };
                            let guild_id = new.guild_id;
                            if let GiveawayOpen::Started(g) = db.create_giveaway(new).await? {
                                db.toggle_giveaway_entry(g.id, b, Utc::now()).await?;
                                db.toggle_giveaway_entry(g.id, a, Utc::now()).await?;
                                db.end_giveaway(g.id, Utc::now(), true, |e, n| e.iter().copied().take(n).collect())
                                    .await?;
                                db.reroll_giveaway(guild_id, g.id, 1, |e, n| e.iter().copied().take(n).collect())
                                    .await?;
                                db.mark_giveaway_announced(g.id).await?;
                            }
                            Ok(())
                        }
                        9 => {
                            db.delete_user_data(a, Utc::now()).await?;
                            db.earn_shells(a, 1_000_000, "test").await.map(drop)
                        }
                        10 => db.user(a).await.map(drop),
                        11 => db.earn_shells(a, 3, "test").await.map(drop),
                        12 => {
                            db.grant_item(a, "chair", 1).await?;
                            db.list_item(a, "chair", 2).await?;
                            if let Some(listing) = db.listings(a).await?.first() {
                                db.buy_listing(b, listing.id).await?;
                            }
                            Ok(())
                        }
                        14 => {
                            db.befriend(a, b).await?;
                            db.record_interaction("hug", a, b).await?;
                            db.block_signer(a, b, "Somebody").await.map(drop)
                        }
                        15 => {
                            let room = NewRoom {
                                room_type: "living".into(),
                                wallpaper: "plain".into(),
                                floor: "oak".into(),
                            };
                            let owner = owners[(roll() % owners.len() as u64) as usize];
                            db.house(owner, &room).await?;
                            db.grant_item(owner, "chair", 2).await?;
                            db.place_item(
                                owner,
                                0,
                                "chair",
                                (roll() % 8) as i16,
                                (roll() % 8) as i16,
                                false,
                                |_| Ok(()),
                            )
                            .await?;
                            db.clear_room(owner, 0).await.map(drop)
                        }
                        16 => {

                            db.forget_old_giveaways(Utc::now() + Duration::days(1)).await?;
                            db.forget_old_games().await?;
                            db.forget_old_parties(Utc::now() + Duration::days(1), 0).await?;
                            db.forget_old_collection(Utc::now() + Duration::days(1), 0).await?;
                            db.forget_weeks_before(NaiveDate::from_ymd_opt(2001, 1, 2).unwrap())
                                .await
                                .map(drop)
                        }
                        17 => {
                            // A staked derby: two racers, one leaves and comes back, the host runs it.
                            let channel = crate::support::id();
                            if let Some(derby) = db
                                .open_party_game(lobby_of("derby", guild, channel, a, 3, PENDING))
                                .await?
                            {
                                db.derby_join(derby.id, a, "A", 0, 30, LOSS).await?;
                                db.derby_join(derby.id, b, "B", 1, 30, LOSS).await?;
                                if roll() % 2 == 0 {
                                    db.derby_leave(derby.id, b).await?;
                                    db.derby_join(derby.id, b, "B", 0, 30, LOSS).await?;
                                }
                                db.derby_settle(derby.id, Some(a), 2, (roll() % 2) as i16).await?;
                            }
                            Ok(())
                        }
                        18 => {
                            let channel = crate::support::id();
                            if let Some(heist) = db
                                .open_party_game(lobby_of("heist", guild, channel, a, 0, PENDING))
                                .await?
                            {
                                db.heist_join(heist.id, a, "A", 0, 5).await?;
                                db.heist_join(heist.id, b, "B", 1, 5).await?;
                                db.heist_settle(heist.id, Some(a), 2, 1_000_000, |_, _| 2).await?;
                            }
                            Ok(())
                        }
                        19 => {
                            let storm_guild = crate::support::id();
                            if let Some(storm) = db
                                .open_party_game(lobby_of("surge", storm_guild, 5, a, 0, ACTIVE))
                                .await?
                            {
                                db.surge_help(storm.id, a, "A", 3).await?;
                                db.surge_help(storm.id, b, "B", 3).await?;
                                db.surge_settle(storm.id, true, false, 1_000_000, |_, helps| helps)
                                    .await?;
                            }
                            Ok(())
                        }
                        20 => {
                            let limits = ShellLimits {
                                plays: 1_000_000,
                                loss: 1_000_000,
                                win: 1_000_000,
                            };
                            let table = NewGame {
                                state: "1",
                                ttl_secs: 45,
                                ..lobby_of("shellgame", guild, crate::support::id(), a, 2, ACTIVE)
                            };
                            if let ShellStart::Started(game) = db.shell_start(table, limits).await? {
                                if roll() % 2 == 0 {
                                    let picked = db.shell_pick(game.id, a, (roll() % 3) as u8, |stake| stake * 2).await?;
                                    anyhow::ensure!(!matches!(picked, ShellPick::NotYours), "the host's own pick");
                                } else {
                                    sqlx::query("UPDATE game_sessions SET expires_at = now() - interval '1 second' WHERE id = $1")
                                        .bind(game.id)
                                        .execute(db.pool())
                                        .await?;
                                    db.shell_refund(game.id).await?;
                                }
                            }
                            Ok(())
                        }
                        21 => {
                            let (award, daily) = (
                                Award {
                                    entry: "x_one",
                                    new_shells: 3,
                                    dup_shells: 2,
                                    dup_fragments: 1,
                                    bonus_fragments: 0,
                                },
                                Daily {
                                    game: "critterdrop",
                                    day: 20_000,
                                    cap: 1_000_000,
                                },
                            );
                            let cards: Vec<String> = ["x_one", "x_two", "x_three"].map(String::from).into();
                            let channel = (crate::support::id()) + 1;
                            let critters = db
                                .create_drop(NewDrop {
                                    kind: DROP_CRITTERS,
                                    guild_id: guild,
                                    channel_id: channel,
                                    host: None,
                                    cards: &cards,
                                    expires_at: Utc::now() + Duration::hours(1),
                                })
                                .await?;
                            db.grab_card(critters.id, 0, a, Utc::now(), award, daily).await?;
                            db.grab_card(critters.id, 0, b, Utc::now(), award, daily).await?;
                            let bottle = db
                                .create_drop(NewDrop {
                                    kind: DROP_BOTTLE,
                                    guild_id: guild,
                                    channel_id: channel + 1,
                                    host: None,
                                    cards: &["bottle_01".to_owned()],
                                    expires_at: Utc::now() + Duration::hours(1),
                                })
                                .await?;
                            db.open_bottle(bottle.id, a, Utc::now(), BottlePrize::Fragments(5), daily)
                                .await?;
                            db.exchange_fragments(a, 5, "chair").await?;
                            db.start_pot(b, 1, Utc::now() - Duration::hours(2)).await?;
                            db.haul_pot(b, Utc::now(), daily, |_, _, _| vec![award]).await?;
                            db.upgrade_pot(b, 1, 1).await.map(drop)
                        }
                        22 => {
                            let deadline = Utc::now() + Duration::hours(1);
                            let game = db
                                .create_party(NewParty {
                                    kind: "hotpearl",
                                    guild_id: guild,
                                    channel_id: crate::support::id(),
                                    host: a,
                                    host_name: "A",
                                    rounds: 1,
                                    status: "lobby",
                                    phase: "lobby",
                                    state: "{}",
                                    deadline,
                                })
                                .await?;
                            db.join_party(game.id, b, "B", 8, Utc::now()).await?;
                            if roll() % 2 == 0 {
                                db.leave_party(game.id, b, Utc::now()).await?;
                                db.join_party(game.id, b, "B", 8, Utc::now()).await?;
                            }
                            if let Some(game) = db.party(game.id).await? {
                                db.commit_party(
                                    game.id,
                                    game.seq,
                                    Commit {
                                        phase: "done",
                                        round: 1,
                                        state: "{}",
                                        deadline,
                                        status: "done",
                                        players: &[
                                            PlayerChange {
                                                user: a,
                                                score: 1,
                                                ..Default::default()
                                            },
                                            PlayerChange {
                                                user: b,
                                                score: 1,
                                                ..Default::default()
                                            },
                                        ],
                                        entries: &[],
                                        pay: Some(Pay {
                                            game: "hotpearl",
                                            day: 20_000,
                                            cap: 1_000_000,
                                            reason: "party_win",
                                            shells: &[(a, 2), (b, 1)],
                                        }),
                                    },
                                )
                                .await?;
                            }
                            Ok(())
                        }
                        23 => {
                            let contest_guild = crate::support::id();
                            for user in [a, b] {
                                sqlx::query(
                                    "INSERT INTO contest_entries (guild_id, week, user_id, owner_name, room_idx, tier, room_type, wallpaper, floor)
                                     VALUES ($1, 5, $2, 'Name', 0, 1, 'living', 'w', 'f') ON CONFLICT DO NOTHING",
                                )
                                .bind(contest_guild as i64)
                                .bind(user as i64)
                                .execute(db.pool())
                                .await?;
                            }
                            db.contest_vote(contest_guild, 5, a, b).await?;
                            db.contest_vote(contest_guild, 5, b, a).await?;
                            db.close_contest_weeks(contest_guild, 6, &[5, 3], "house_of_the_week")
                                .await
                                .map(drop)
                        }
                        _ => {
                            let pet = match db.active_pet(a).await? {
                                Some(pet) => pet,
                                None => db.adopt(a, "cat", "Tom").await?,
                            };
                            let back = Utc::now() - Duration::seconds(1);
                            db.start_trip(a, &pet, "beach", [50, 50, 50, 50], back).await?;
                            db.claim_trip(a, pet.id, Utc::now(), |p, _| TripPayout {
                                shells: 1,
                                item: None,
                                level: p.level,
                                xp: p.xp,
                            })
                            .await
                            .map(drop)
                        }
                    }
                }
                .await;
                let t = &mut tally[op];
                match result {
                    Ok(()) => t.ok += 1,
                    Err(error) => {
                        t.failed += 1;
                        if t.samples.len() < 3 {
                            t.samples.push(format!("{error:#}"));
                        }
                    }
                }
            }
            tally
        }));
    }
    let mut total = vec![Tally::default(); OPS.len()];
    for handle in handles {
        for (sum, t) in total.iter_mut().zip(handle.await.unwrap()) {
            sum.ok += t.ok;
            sum.failed += t.failed;
            sum.samples
                .extend(t.samples.into_iter().take(3 - sum.samples.len().min(3)));
        }
    }
    let retried = pinchora_db::conflict_retries() - retries_before;
    let mut report = format!("MIX secs={secs} workers={workers} retried={retried}\n");
    for (name, t) in OPS.iter().zip(&total) {
        report.push_str(&format!(
            "  {name:<9} ok={:<6} failed={:<4} {:?}\n",
            t.ok, t.failed, t.samples
        ));
    }
    println!("{report}");

    let mut books = Vec::new();
    for user in users.iter() {
        let (balance, booked) = (credits(&db, *user).await, ledger(&db, *user).await);
        if balance != booked {
            books.push(format!("{user}: balance {balance}, ledger {booked}"));
        }
    }
    let ids: Vec<i64> = users.iter().map(|u| *u as i64).collect();
    let (held,): (i64,) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM game_sessions WHERE status = 'active' AND host_id = ANY($1))
              + (SELECT count(*) FROM giveaways WHERE status = 'running' AND host_id = ANY($1))
              + (SELECT COALESCE(SUM(m.amount), 0) FROM party_members m JOIN game_sessions s ON s.id = m.session_id
                 WHERE s.status <> 'done' AND m.user_id = ANY($1))::BIGINT",
    )
    .bind(&ids)
    .fetch_one(db.pool())
    .await
    .unwrap();
    for user in users.iter().chain(owners.iter()) {
        db.delete_user_data(*user, Utc::now()).await.unwrap();
    }
    // The drops stay open for an hour, and the simulator's collection tick closes the oldest due ones first.
    sqlx::query("DELETE FROM collection_drops WHERE guild_id = $1")
        .bind(guild as i64)
        .execute(db.pool())
        .await
        .unwrap();
    let failed: u64 = total.iter().map(|t| t.failed).sum();
    assert_eq!(failed, 0, "calls failed under the mixed load:\n{report}");
    assert!(
        books.is_empty(),
        "balances that are not what the ledger says: {books:?}"
    );
    assert_eq!(held, 0, "shells left held in a duel, a giveaway or a game");
    assert!(total.iter().all(|t| t.ok > 0), "every kind of call ran:\n{report}");
}
