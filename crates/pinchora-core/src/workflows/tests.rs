use super::*;
mod evaluation {
    use super::*;
    fn def() -> Definition {
        Definition {
            name: "hello".into(),
            enabled: true,
            trigger: Trigger::Button {},
            conditions: vec![],
            actions: vec![StepAction::Reply {
                text: "Hi".into(),
                embed: None,
            }],
        }
    }
    #[test]
    fn budgets_and_unknown_actions_fail_closed() {
        let mut d = def();
        d.actions = vec![d.actions[0].clone(); 6];
        assert_eq!(d.validate(), Err("writes"));
        d.actions = vec![StepAction::Log { text: "x".into() }; 11];
        assert_eq!(d.validate(), Err("actions"));
        d = def();
        d.conditions = vec![Condition::Chance { percent: 100 }; 6];
        assert_eq!(d.validate(), Err("conditions"));
        for kind in ["wait", "give_shells", "run_workflow", "execute", "http"] {
            assert!(serde_json::from_value::<StepAction>(serde_json::json!({"kind":kind})).is_err());
        }
        d = def();
        d.actions = vec![StepAction::RandomBranch {
            branches: vec![vec![StepAction::RandomBranch {
                branches: vec![d.actions.clone()],
            }]],
        }];
        assert!(d.validate().is_err());
    }
    #[test]
    fn conditions_and_depth() {
        let mut d = def();
        d.conditions = vec![
            Condition::HasRole { role_id: "42".into() },
            Condition::VarGreater {
                scope: Scope::Server,
                key: "visits".into(),
                value: 2,
            },
        ];
        let mut e = Event {
            kind: "button".into(),
            roles: vec!["42".into()],
            ..Default::default()
        };
        e.variables.insert("server:visits".into(), "3".into());
        assert_eq!(plan(&d, &e, Utc::now()).outcome, "ready");
        e.depth = 1;
        assert_eq!(plan(&d, &e, Utc::now()).outcome, "refused");
        e.depth = 0;
        e.roles.clear();
        assert_eq!(plan(&d, &e, Utc::now()).outcome, "filtered");
    }
    #[test]
    fn schedule_dst_and_intervals() {
        let now = "2026-03-08T06:59:00Z".parse().unwrap();
        assert_eq!(
            Schedule::Daily { hour: 2, minute: 30 }
                .next(now, "America/New_York")
                .unwrap()
                .to_rfc3339(),
            "2026-03-09T06:30:00+00:00"
        );
        assert_eq!(
            Schedule::Weekly {
                weekday: 6,
                hour: 2,
                minute: 30
            }
            .next("2026-03-01T07:30:00Z".parse().unwrap(), "America/New_York")
            .unwrap()
            .to_rfc3339(),
            "2026-03-15T06:30:00+00:00"
        );
        assert!(Schedule::Interval { minutes: 0 }.next(now, "UTC").is_none());
        assert!(
            Schedule::Weekly {
                weekday: 7,
                hour: 1,
                minute: 0
            }
            .next(now, "UTC")
            .is_none()
        );
    }
}

mod contract_tests {
    use super::*;
    #[test]
    fn validation_table_and_write_accounting() {
        let sample = serde_json::json!({"name":"workflow","enabled":true,"trigger":{"kind":"command"},"conditions":[],"actions":[{"kind":"log","text":"ok"}]});
        for condition in [
            serde_json::json!({"kind":"chance","percent":101}),
            serde_json::json!({"kind":"cooldown","scope":"server","seconds":0}),
            serde_json::json!({"kind":"has_role","role_id":"0"}),
            serde_json::json!({"kind":"var_equals","scope":"member","key":"bad.key","value":"x"}),
            serde_json::json!({"kind":"time_window","start":1440,"end":1,"tz":"UTC"}),
        ] {
            let mut value = sample.clone();
            value["conditions"] = serde_json::json!([condition]);
            assert_eq!(
                serde_json::from_value::<Definition>(value).unwrap().validate(),
                Err("conditions")
            );
        }
        assert_eq!(
            StepAction::Tag {
                action: Action::Dm { text: "hi".into() }
            }
            .writes(),
            2
        );
        let mut value = sample;
        value["actions"] = serde_json::json!([{"kind":"random_branch","branches":[[{"kind":"log","text":"ok"}],[{"kind":"open_ticket","panel_id":1,"topic":0},{"kind":"reply","text":"extra"}]]}]);
        assert_eq!(
            serde_json::from_value::<Definition>(value).unwrap().validate(),
            Err("writes")
        );
    }
    #[test]
    fn preview_evaluates_variables_sequentially_without_mutating_fake_event() {
        let def:Definition=serde_json::from_value(serde_json::json!({"name":"preview","enabled":false,"trigger":{"kind":"command"},"conditions":[],"actions":[{"kind":"set_var","scope":"server","key":"visits","value":"2"},{"kind":"incr_var","scope":"server","key":"visits","amount":3},{"kind":"reply","text":"Visits: {var.server.visits}"}]})).unwrap();
        let event = Event {
            kind: "command".into(),
            ..Default::default()
        };
        let result = plan(&def, &event, Utc::now());
        assert_eq!(result.steps[3].detail["text"], "Visits: 5");
        assert!(event.variables.is_empty());
    }
    #[test]
    fn timezone_window_cooldown_and_random_boundaries() {
        let now = "2026-11-08T21:30:00Z".parse().unwrap();
        let event = Event {
            cooldowns: vec![2],
            roll: 99,
            ..Default::default()
        };
        assert!(condition_passes(
            &Condition::TimeWindow {
                start: 1380,
                end: 60,
                tz: "Europe/Istanbul".into()
            },
            0,
            &event,
            now
        ));
        assert!(!condition_passes(
            &Condition::Cooldown {
                scope: Scope::Member,
                seconds: 60
            },
            2,
            &event,
            now
        ));
        assert!(!condition_passes(&Condition::Chance { percent: 99 }, 0, &event, now));
        assert!(condition_passes(&Condition::Chance { percent: 100 }, 0, &event, now));
    }
}

#[test]
fn custom_emoji_ids_cannot_panic_or_overflow_discord_ids() {
    for emoji in ["<:xx:00000000000000000>", "<:xx:18446744073709551616>", "not emoji"] {
        for kind in ["add_reaction", "tag"] {
            let action = if kind == "tag" {
                serde_json::json!({"kind":"tag","action":{"kind":"react","emoji":emoji}})
            } else {
                serde_json::json!({"kind":"add_reaction","emoji":emoji})
            };
            let d: super::Definition = serde_json::from_value(serde_json::json!({"name":"emoji","enabled":true,"trigger":{"kind":"button"},"conditions":[],"actions":[action]})).unwrap();
            assert!(d.validate().is_err(), "{emoji}");
        }
    }
}

#[test]
fn schedules_skip_repeated_dst_minute_and_keep_local_weekday() {
    use super::Schedule;
    let daily = Schedule::Daily { hour: 1, minute: 30 };
    let first = daily
        .next("2026-11-01T05:29:59Z".parse().unwrap(), "America/New_York")
        .unwrap();
    assert_eq!(first.to_rfc3339(), "2026-11-01T05:30:00+00:00");
    assert_eq!(
        daily.next(first, "America/New_York").unwrap().to_rfc3339(),
        "2026-11-02T06:30:00+00:00"
    );
    let weekly = Schedule::Weekly {
        weekday: 0,
        hour: 0,
        minute: 15,
    };
    assert_eq!(
        weekly
            .next("2026-11-08T20:00:00Z".parse().unwrap(), "Europe/Istanbul")
            .unwrap()
            .to_rfc3339(),
        "2026-11-08T21:15:00+00:00"
    );
}

#[test]
fn variable_preview_refuses_the_same_value_failures_as_storage() {
    let mut def: Definition = serde_json::from_value(serde_json::json!({"name":"vars","enabled":true,"trigger":{"kind":"button"},"conditions":[],"actions":[{"kind":"incr_var","scope":"server","key":"count","amount":1}]})).unwrap();
    let mut event = Event {
        kind: "button".into(),
        ..Default::default()
    };
    for value in ["not a number", "9223372036854775807"] {
        event.variables.insert("server:count".into(), value.into());
        let plan = plan(&def, &event, Utc::now());
        assert_eq!(plan.outcome, "failed");
        assert!(plan.actions.is_empty());
        assert_eq!(plan.steps[1].status, "failed");
        assert_eq!(event.variables["server:count"], value);
    }
    def.actions = vec![StepAction::SetVar {
        scope: Scope::Server,
        key: "text".into(),
        value: "{event}".into(),
    }];
    event.value = "x".repeat(201);
    assert_eq!(plan(&def, &event, Utc::now()).outcome, "failed");
}

#[test]
fn every_shape_rejects_unknown_fields_and_out_of_range_values() {
    let sample = serde_json::json!({"name":"shape","enabled":true,"trigger":{"kind":"button"},"conditions":[],"actions":[{"kind":"log","text":"ok"}]});
    let invalid_conditions = [
        serde_json::json!({"kind":"has_role","role_id":"0"}),
        serde_json::json!({"kind":"lacks_role","role_id":"18446744073709551615"}),
        serde_json::json!({"kind":"in_channel","channel_id":"no"}),
        serde_json::json!({"kind":"min_level","level":100001}),
        serde_json::json!({"kind":"account_age","days":36501}),
        serde_json::json!({"kind":"chance","percent":101}),
        serde_json::json!({"kind":"cooldown","scope":"member","seconds":86401}),
        serde_json::json!({"kind":"var_equals","scope":"server","key":"x","value":"x".repeat(201)}),
        serde_json::json!({"kind":"var_greater","scope":"server","key":"x".repeat(33),"value":1}),
        serde_json::json!({"kind":"time_window","start":0,"end":60,"tz":"not/a/zone"}),
    ];
    for condition in invalid_conditions {
        let mut value = sample.clone();
        value["conditions"] = serde_json::json!([condition]);
        assert_eq!(
            serde_json::from_value::<Definition>(value).unwrap().validate(),
            Err("conditions")
        );
    }
    for action in [
        serde_json::json!({"kind":"reply","text":"x".repeat(2001)}),
        serde_json::json!({"kind":"send_message","channel_id":"0","text":"hello"}),
        serde_json::json!({"kind":"tag","action":{"kind":"add_role","role_id":"18446744073709551615"}}),
        serde_json::json!({"kind":"tag","action":{"kind":"dm","text":" "}}),
        serde_json::json!({"kind":"tag","action":{"kind":"give_points","amount":1001}}),
        serde_json::json!({"kind":"take_points","amount":0}),
        serde_json::json!({"kind":"set_var","scope":"member","key":"bad.key","value":"x"}),
        serde_json::json!({"kind":"incr_var","scope":"server","key":"x","amount":1000000001i64}),
        serde_json::json!({"kind":"create_thread","channel_id":"1","name":"x".repeat(101)}),
        serde_json::json!({"kind":"open_ticket","panel_id":0,"topic":0}),
        serde_json::json!({"kind":"open_ticket","panel_id":1,"topic":25}),
        serde_json::json!({"kind":"random_branch","branches":[[]]}),
        serde_json::json!({"kind":"log","text":""}),
        serde_json::json!({"kind":"reply","text":"hello","embed":{"title":"x".repeat(257)}}),
    ] {
        let mut value = sample.clone();
        value["actions"] = serde_json::json!([action]);
        assert!(serde_json::from_value::<Definition>(value).unwrap().validate().is_err());
    }
    for trigger in [
        serde_json::json!({"kind":"schedule","spec":{"kind":"interval","minutes":525601},"tz":"UTC"}),
        serde_json::json!({"kind":"schedule","spec":{"kind":"daily","hour":24,"minute":0},"tz":"UTC"}),
        serde_json::json!({"kind":"role_removed","role_id":"0"}),
        serde_json::json!({"kind":"message_keyword","keyword":"x".repeat(201)}),
        serde_json::json!({"kind":"reaction_added","channel_id":"1","emoji":"not emoji"}),
    ] {
        let mut value = sample.clone();
        value["trigger"] = trigger;
        assert_eq!(
            serde_json::from_value::<Definition>(value).unwrap().validate(),
            Err("trigger")
        );
    }
    for field in ["trigger", "actions", "conditions"] {
        let mut value = sample.clone();
        if field == "conditions" {
            value[field] = serde_json::json!([{"kind":"chance","percent":100}]);
        }
        let node = if field == "trigger" {
            &mut value[field]
        } else {
            &mut value[field][0]
        };
        node["unexpected"] = true.into();
        assert!(serde_json::from_value::<Definition>(value).is_err());
    }
}

#[test]
fn fieldless_triggers_and_pin_reject_extra_fields() {
    for kind in [
        "command",
        "button",
        "member_join",
        "member_leave",
        "level_up",
        "voice_join",
        "birthday",
        "achievement",
        "ticket_opened",
        "ticket_closed",
        "starboard_post",
        "feed_item",
    ] {
        assert!(
            serde_json::from_value::<Trigger>(serde_json::json!({"kind":kind,"unexpected":true})).is_err(),
            "{kind}"
        );
    }
    assert!(serde_json::from_value::<StepAction>(serde_json::json!({"kind":"pin","unexpected":true})).is_err());
}
