#[cfg(test)]
mod tests {
    use super::super::fixture::context;
    use crate::admin::command_control::component_owner;
    use crate::infra::defer::Deferral;

    #[tokio::test]
    async fn payload_selected_owners_and_segment_boundaries_are_preserved() {
        let (cx, _response) = context().await;
        for (id, expected) in [
            ("sl:eco:step:123", Some("economyv2 jobs")),
            ("sl:eco:review:123", Some("economyv2 trade-review")),
            ("sl:eco:step", Some("economyv2 trade-review")),
            ("sl:ecox:step:123", Some("stall sell")),
            ("dw:123", Some("house decorate")),
            ("df:123", Some("house decorate")),
            ("dn:123", Some("house decorate")),
            ("dwx:123", None),
            ("vc:rn:123", Some("voice rename")),
            ("vo:t:123", Some("voice transfer")),
        ] {
            assert_eq!(component_owner(&cx, id).await.unwrap(), expected, "{id}");
        }
        for (payload, expected) in [
            (Some("nonce:reset:user"), "xp reset"),
            (Some("nonce:all:user"), "xp reset-all"),
            (Some("nonce:go:user"), "xp import mee6"),
            (Some("nonce:unknown:user"), "xp"),
            (Some("malformed"), "xp"),
            (None, "xp"),
        ] {
            let key = "xp:confirm:0:1:registration-owner";
            cx.app.cache.forget(key).await.unwrap();
            if let Some(payload) = payload {
                cx.app
                    .cache
                    .put(key, payload, std::time::Duration::from_secs(60))
                    .await
                    .unwrap();
            }
            assert_eq!(
                component_owner(&cx, "xi:confirm:registration-owner").await.unwrap(),
                Some(expected)
            );
            // Ownership lookup must not consume a confirmation before the handler runs.
            assert_eq!(cx.app.cache.get(key).await.unwrap().as_deref(), payload);
            cx.app.cache.forget(key).await.unwrap();
        }
    }

    #[tokio::test]
    async fn defer_respects_modal_component_autocomplete_and_segment_boundaries() {
        let (cx, _response) = context().await;
        for (id, expected) in [
            ("gb:s:123", Deferral::Never),
            ("gb:sx:123", Deferral::Public),
            ("gbx:s:123", Deferral::Public),
            ("wo:g:123", Deferral::Never),
            ("wf:run:123", Deferral::Private),
            ("wfx:run:123", Deferral::Public),
            ("dw:123", Deferral::Public),
            ("sl:eco:step:123", Deferral::Public),
        ] {
            let mut interaction = serde_json::to_value(&cx.interaction).unwrap();
            interaction["type"] = 3.into();
            interaction["data"] = serde_json::json!({"custom_id":id,"component_type":2});
            assert_eq!(
                Deferral::of(&serde_json::from_value(interaction.clone()).unwrap()),
                expected,
                "{id}"
            );
            interaction["type"] = 5.into();
            interaction["data"] = serde_json::json!({"custom_id":id,"components":[]});
            assert_eq!(
                Deferral::of(&serde_json::from_value(interaction).unwrap()),
                Deferral::Public,
                "modal {id}"
            );
        }
        let mut interaction = cx.interaction.clone();
        interaction.kind = twilight_model::application::interaction::InteractionType::ApplicationCommandAutocomplete;
        assert_eq!(Deferral::of(&interaction), Deferral::Never);
        for (path, expected) in [
            ("house edit", Deferral::Private),
            ("house edit nested", Deferral::Private),
            ("house editor", Deferral::Public),
            ("customcommand create", Deferral::Never),
            ("customcommand create nested", Deferral::Never),
            ("customcommand createx", Deferral::Public),
        ] {
            assert_eq!(Deferral::for_path(path), expected, "{path}");
        }
    }
}
