use twilight_interactions::command::{CommandModel, CreateCommand};

#[derive(CommandModel, CreateCommand)]
#[command(name = "registration-fixture", desc = "Registration test command")]
pub struct Fixture;

impl Fixture {
    pub async fn run(self, cx: &crate::Cx) -> anyhow::Result<()> {
        cx.reply_text("command dispatched").await
    }
}

commands! {
    typed { Fixture, }
    context {}
}

compatibility! {
    components { Some("fixture") => button(), }
    owners { "fixture" => "registration-fixture", }
    component_gates { "fixture" => "house", }
}

fn button() -> &'static str {
    "button dispatched"
}

include!("fixture_output.rs");

#[tokio::test]
async fn one_domain_registration_reaches_every_surface() {
    let entries = super::ordered(COMMANDS).collect::<Vec<_>>();
    assert_eq!(entries.len(), 1);
    assert_eq!((entries[0].definition)().name, "registration-fixture");
    assert_eq!(entries[0].name, "registration-fixture");
    let (cx, response) = context().await;
    let data = serde_json::from_value(serde_json::json!({
        "id": "1", "name": "registration-fixture", "type": 1
    }))
    .unwrap();
    (entries[0].dispatch)(&cx, data).await.unwrap();
    assert_eq!(
        response.await.unwrap().data.unwrap().content.as_deref(),
        Some("command dispatched")
    );
    assert_eq!(component("fixture:open:123"), "button dispatched");
    assert_eq!(component("fixture-extra:open"), "unknown");
    assert_eq!(owner("fixture"), "registration-fixture");
    assert_eq!(module("fixture"), "house");
    assert_eq!(PRIVATE, ["registration-fixture"]);
    assert_eq!(OPENS_MODAL, ["fixture:open"]);
    assert_eq!(HELP, ["registration-fixture"]);
    assert_eq!(SLASH_ONLY, ["registration-fixture"]);
    assert_eq!(ALIASES, [("fixture", "registration-fixture")]);
}

pub(crate) async fn context() -> (
    crate::Cx,
    tokio::sync::oneshot::Receiver<twilight_model::http::interaction::InteractionResponse>,
) {
    use std::sync::Arc;
    let database = std::env::var("PINCHORA_TEST_DATABASE_URL").expect("test database configured");
    let redis = std::env::var("PINCHORA_TEST_REDIS_URL").expect("test Redis configured");
    assert!(url::Url::parse(&database).unwrap().path().starts_with("/pinchora_test"));
    let redis_database: u32 = url::Url::parse(&redis)
        .unwrap()
        .path()
        .trim_start_matches('/')
        .parse()
        .unwrap();
    assert_ne!(redis_database, 0);
    let db = pinchora_db::Db::connect(&database, 1).await.unwrap();
    let cache = pinchora_cache::Cache::connect_scoped(&redis, "registration-fixture")
        .await
        .unwrap();
    let http = Arc::new(twilight_http::Client::new("fixture".into()));
    let app = Arc::new(crate::App::new(
        http.clone(),
        http,
        twilight_model::id::Id::new(1),
        db,
        cache,
        "fixture".into(),
    ));
    let interaction = serde_json::from_value(serde_json::json!({
        "id": "1", "application_id": "1", "type": 2, "token": "fixture", "version": 1,
        "user": {"id":"1", "username":"fixture", "discriminator":"0001", "avatar":null},
        "authorizing_integration_owners": {}, "attachment_size_limit": 10,
        "data": { "id": "1", "name": "registration-fixture", "type": 1 }
    }))
    .unwrap();
    let (send, receive) = tokio::sync::oneshot::channel();
    let responder = Arc::new(crate::Responder::http(app.clone(), &interaction, send));
    (
        crate::Cx {
            app,
            interaction,
            responder,
            lang: pinchora_core::infra::i18n::Lang::En,
        },
        receive,
    )
}
