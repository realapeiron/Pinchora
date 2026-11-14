    use crate::commands::community::trackers::*;
    #[test]
    fn tracker_icons_bound_pixels_and_compressed_metadata() {
        let mut png = std::io::Cursor::new(Vec::new());
        image::RgbaImage::new(64, 64)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        assert!(tracker_icon(png.get_ref()).is_ok());
        let mut oversized = std::io::Cursor::new(Vec::new());
        image::RgbaImage::new(512, 512)
            .write_to(&mut oversized, image::ImageFormat::Png)
            .unwrap();
        assert!(tracker_icon(oversized.get_ref()).is_err());
        // Text metadata is never expanded by the pixel decoder and is discarded on
        // re-encoding; the resulting card attachment cannot carry the compressed bomb.
        let safe = tracker_icon(include_bytes!("../../../../tests/fixtures/tracker-icon-metadata-bomb.png")).unwrap();
        assert!(safe.len() < 16_384);
        assert!(
            !safe
                .windows(4)
                .any(|chunk| matches!(chunk, b"zTXt" | b"iTXt" | b"iCCP"))
        );
    }

    #[test]
    fn tracker_destinations_must_be_text_channels_in_the_invoking_guild() {
        for (guild, kind, expected) in [
            (123, 0, true),
            (123, 5, true),
            (456, 0, false),
            (123, 1, false),
            (123, 11, false),
        ] {
            let channel =
                serde_json::from_value(json!({"id":"789", "guild_id":guild.to_string(), "type":kind})).unwrap();
            assert_eq!(posting_channel(123, &channel), expected);
        }
    }

    #[tokio::test]
    async fn riot_request_uses_header_and_mock_http() {
        use axum::{Router, http::HeaderMap, routing::get};
        let app = Router::new()
            .route(
                "/limit",
                get(|| async { ([("retry-after", "7")], axum::http::StatusCode::TOO_MANY_REQUESTS) }),
            )
            .route(
                "/lol/league/v4/entries/by-puuid/mock-puuid",
                get(|| async {
                    axum::Json(json!([{"queueType":"RANKED_SOLO_5x5","tier":"GOLD","rank":"II","leaguePoints":45}]))
                }),
            )
            .route(
                "/riot/account/v1/accounts/by-riot-id/A/B",
                get(|headers: HeaderMap| async move {
                    assert_eq!(headers.get("x-riot-token").unwrap(), "test-key");
                    axum::Json(json!({"puuid":"mock-puuid","gameName":"A","tagLine":"B"}))
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let url = url::Url::parse(&format!("http://{address}/riot/account/v1/accounts/by-riot-id/A/B")).unwrap();
        let value = riot_http_get(&crate::community::trackers::http_client().unwrap(), url, "test-key")
            .await
            .unwrap();
        assert_eq!(value["puuid"], "mock-puuid");
        let mut url = riot_url("euw1", &league_path("mock-puuid")).unwrap();
        url.set_scheme("http").unwrap();
        url.set_host(Some("127.0.0.1")).unwrap();
        url.set_port(Some(address.port())).unwrap();
        let rank = riot_http_get(&crate::community::trackers::http_client().unwrap(), url, "test-key")
            .await
            .unwrap();
        assert_eq!(rank[0]["tier"], "GOLD");
        let limited = riot_http_get(
            &crate::community::trackers::http_client().unwrap(),
            url::Url::parse(&format!("http://{address}/limit")).unwrap(),
            "test-key",
        )
        .await
        .unwrap_err();
        assert_eq!(limited.downcast_ref::<RiotRateLimited>().unwrap().0, 7);
    }
    #[test]
    fn riot_ids_retry_after_and_regions() {
        assert_eq!(riot_id("  Çağrı 猫 # TR1  "), Some(("Çağrı 猫", "TR1")));
        for id in ["a#b#c", "a#", "#b", "a\n#b", "a#b\u{202e}"] {
            assert!(riot_id(id).is_none(), "{id}");
        }
        assert_eq!(retry_after("600"), 600);
        assert_eq!(retry_after("0"), 1);
        assert_eq!(retry_after("invalid"), 60);
        let later = (chrono::Utc::now() + chrono::Duration::seconds(600)).to_rfc2822();
        assert!((599..=600).contains(&retry_after(&later)));
        let url = riot_url(
            "europe",
            &["riot", "account", "v1", "accounts", "by-riot-id", "Çağrı/猫", "TR#1"],
        )
        .unwrap();
        assert!(url.path().ends_with("%C3%87a%C4%9Fr%C4%B1%2F%E7%8C%AB/TR%231"));
        assert!(url.query().is_none() && url.fragment().is_none());
        assert_eq!(LolRegion::Tr1.routing(), "europe");
        assert_eq!(LolRegion::Br1.routing(), "americas");
        assert_eq!(LolRegion::Jp1.routing(), "asia");
    }

    #[tokio::test]
    async fn riot_errors_never_include_credentials_or_follow_redirects() {
        use axum::{Router, routing::get};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new()
            .route(
                "/redirect",
                get(|| async { (axum::http::StatusCode::FOUND, [("location", "/leak")]) }),
            )
            .route(
                "/leak",
                get(|| async {
                    panic!("token redirect followed");
                    #[allow(unreachable_code)]
                    ""
                }),
            )
            .route(
                "/error",
                get(|| async { (axum::http::StatusCode::BAD_REQUEST, "secret-key") }),
            );
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        for path in ["redirect", "error"] {
            let error = riot_http_get(
                &crate::community::trackers::http_client().unwrap(),
                url::Url::parse(&format!("http://{addr}/{path}")).unwrap(),
                "secret-key",
            )
            .await
            .unwrap_err();
            assert!(!format!("{error:?}").contains("secret-key"));
        }
        server.abort();
    }

    #[test]
    fn data_dragon_maps_champion_id() {
        let data = json!({"data":{"Ahri":{"key":"103","name":"Ahri","image":{"full":"Ahri.png"}}}});
        assert_eq!(champion(&data, 103), Some(("Ahri", "Ahri.png")));
    }
