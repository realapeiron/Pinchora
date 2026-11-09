use serde_json::json;

use crate::admin::card_design::*;

fn no_art(_: &str) -> bool {
    false
}

fn some_art(id: &str) -> bool {
    matches!(id, "bg_starlit_jetty" | "p_drowsepatch_cat")
}

fn design(layers: Value) -> Value {
    json!({
        "version": 1,
        "width": 1100,
        "height": 500,
        "background": { "type": "gradient", "from": "#1D2B53", "to": "#0f1631", "pixel": true },
        "layers": layers,
    })
}

fn text_layer(text: &str) -> Value {
    json!({
        "id": "title", "type": "text", "x": 50, "y": 252, "w": 1000, "h": 10,
        "text": text, "font": "pixel", "size": 64, "color": "#FFF1E8", "align": "center",
        "max_lines": 1, "outline_color": "#000000",
    })
}

fn messages(errors: &[FieldError]) -> Vec<&str> {
    errors.iter().map(|e| e.message.as_str()).collect()
}

#[test]
fn a_good_design_is_read_and_tidied() {
    let avatar = json!({
        "id": "avatar", "type": "avatar", "x": 450, "y": 36, "w": 200, "h": 200,
        "shape": "notched", "border_color": "#ff6b4a", "border_width": 12, "pixelate": true,
    });
    let sticker = json!({
        "id": "cat", "type": "sprite", "x": 0, "y": 0, "w": 64, "h": 64,
        "sprite": "p_drowsepatch_cat", "color": "#ffffff", "opacity": 80,
    });
    let parsed = parse_design(
        &design(json!([avatar, text_layer("WELCOME {user}"), sticker])),
        &some_art,
    )
    .unwrap();
    assert_eq!(parsed.layers.len(), 3);
    assert!(matches!(&parsed.background, Background::Gradient { from, .. } if from == "#1d2b53"));
    let title = &parsed.layers[1];
    assert_eq!(title.h, text_height(64, 1), "a text box is as tall as its lines");
    assert!(matches!(&title.content, Content::Text { color, .. } if color == "#fff1e8"));
    assert_eq!((parsed.layers[0].opacity, parsed.layers[2].opacity), (100, 80));
    assert_eq!(parsed.avatar_size(), Some(176));
    assert!(parsed.pixelated_avatar());
    // Read, written and read again, it stays the same.
    let again = parse_design(&serde_json::to_value(&parsed).unwrap(), &some_art).unwrap();
    assert_eq!(again, parsed);
}

#[test]
fn unknown_settings_and_shapes_are_refused_by_name() {
    let mut layer = text_layer("Hi");
    layer["glow"] = json!(true);
    let errors = parse_design(&design(json!([layer])), &no_art).unwrap_err();
    assert!(errors[0].message.contains("`layers[0].glow`"), "{:?}", errors);
    let mut sent = design(json!([]));
    sent["background"]["shine"] = json!(1);
    assert!(
        parse_design(&sent, &no_art).unwrap_err()[0]
            .message
            .contains("`background.shine`")
    );
    let errors = parse_design(&json!({ "version": 1 }), &no_art).unwrap_err();
    assert!(errors[0].message.starts_with("The design could not be read"));
    let mut odd = text_layer("Hi");
    odd["type"] = json!("video");
    assert!(parse_design(&design(json!([odd])), &no_art).is_err());
}

#[test]
fn every_problem_is_listed_with_its_layer() {
    let mut big = text_layer("Hi {nickname}");
    big["size"] = json!(500);
    big["color"] = json!("red");
    let mut twin = text_layer("Hello");
    twin["max_lines"] = json!(9);
    let lopsided = json!({
        "id": "face", "type": "avatar", "x": 0, "y": 0, "w": 100, "h": 90,
        "shape": "circle", "border_color": "#000000", "border_width": 30,
    });
    let missing = json!({
        "id": "pet", "type": "sprite", "x": 0, "y": 0, "w": 32, "h": 32, "sprite": "p_unicorn", "color": "#ffffff",
    });
    let mut sent = design(json!([big, twin, lopsided, missing]));
    sent["width"] = json!(5000);
    sent["dim"] = json!(95);
    sent["background"] = json!({ "type": "sprite", "id": "p_drowsepatch_cat" });
    let errors = parse_design(&sent, &some_art).unwrap_err();
    let fields: Vec<_> = errors.iter().map(|e| e.field).collect();
    assert_eq!(fields[..3], ["width", "dim", "background"]);
    let text = messages(&errors).join("\n");
    for expected in [
        "Layer 1 (Text): {nickname} isn't a placeholder",
        "Layer 1 (Text): the font size goes from 8 to 200 px.",
        "Layer 1 (Text): colour: colours look like",
        "Layer 2 (Text): another layer has the same id.",
        "Layer 2 (Text): a text has 1 to 4 lines.",
        "Layer 3 (Avatar): an avatar is square",
        "Layer 3 (Avatar): the border is at most 40 px and a quarter",
        "Layer 4 (Sticker): that sticker doesn't exist.",
    ] {
        assert!(text.contains(expected), "{expected:?} in\n{text}");
    }
}

#[test]
fn placeholders_backgrounds_and_limits() {
    assert_eq!(
        unknown_placeholder("{user} joined {server} as #{members} ({ordinal})"),
        None
    );
    assert_eq!(
        unknown_placeholder("a { brace } and {count}"),
        Some("{count}".to_owned())
    );
    assert_eq!(unknown_placeholder("{"), None);
    let blank = parse_design(&design(json!([text_layer("   ")])), &no_art).unwrap_err();
    assert!(blank[0].message.ends_with("write something, or delete the layer."));
    let long = parse_design(&design(json!([text_layer(&"x".repeat(TEXT_MAX + 1))])), &no_art).unwrap_err();
    assert!(long[0].message.contains("this one has 201"));

    let mut sent = design(json!([]));
    sent["background"] = json!({ "type": "upload", "id": "0123456789abcdef0123456789abcdef" });
    let parsed = parse_design(&sent, &no_art).unwrap();
    assert_eq!(parsed.upload(), Some("0123456789abcdef0123456789abcdef"));
    sent["background"]["id"] = json!("../../etc/passwd");
    assert!(parse_design(&sent, &no_art).is_err());
    sent["background"] = json!({ "type": "sprite", "id": "bg_starlit_jetty" });
    assert!(parse_design(&sent, &some_art).is_ok());
    sent["background"] = json!({ "type": "transparent" });
    sent["version"] = json!(2);
    assert!(
        parse_design(&sent, &no_art).unwrap_err()[0]
            .message
            .contains("version 2")
    );

    let many: Vec<Value> = (0..=MAX_LAYERS)
        .map(|i| {
            let mut l = text_layer("Hi");
            l["id"] = json!(format!("t{i}"));
            l
        })
        .collect();
    let errors = parse_design(&design(Value::Array(many)), &no_art).unwrap_err();
    assert_eq!(errors[0].field, "layers");
    assert!(errors[0].message.contains("at most 20 layers"));
}

fn avatar_layer(id: &str, size: u32) -> Value {
    json!({
        "id": id, "type": "avatar", "x": 0, "y": 0, "w": size, "h": size,
        "shape": "circle", "border_color": "#000000", "border_width": 0,
    })
}

fn shape_layer(id: &str, w: u32, h: u32) -> Value {
    json!({
        "id": id, "type": "shape", "x": 0, "y": 0, "w": w, "h": h,
        "shape": "rect", "fill": "#336699", "border_color": "#000000",
    })
}

#[test]
fn a_layer_is_never_bigger_than_the_canvas() {
    // 1100 x 500 canvas.
    let ok = parse_design(&design(json!([shape_layer("whole", 1100, 500)])), &no_art);
    assert!(ok.is_ok(), "{ok:?}");
    for (w, h) in [(1101, 500), (1100, 501)] {
        let errors = parse_design(&design(json!([shape_layer("big", w, h)])), &no_art).unwrap_err();
        assert_eq!(
            messages(&errors),
            ["Layer 1 (Shape): it is bigger than the canvas (1100×500 px)."]
        );
    }
    // A text box is as tall as its lines, and that counts: 200 px, four lines is 1040 px.
    let mut tall = text_layer("Hi");
    tall["size"] = json!(200);
    tall["max_lines"] = json!(4);
    let errors = parse_design(&design(json!([tall])), &no_art).unwrap_err();
    assert!(messages(&errors)[0].ends_with("it is bigger than the canvas (1100×500 px)."));
    // Sitting partly off the canvas is fine.
    let mut off = shape_layer("off", 1100, 500);
    off["x"] = json!(-400);
    assert!(parse_design(&design(json!([off])), &no_art).is_ok());
}

#[test]
fn avatars_are_limited_in_size_and_number() {
    // The tallest canvas is 1000 px, which already keeps an avatar under its own 1024 px cap.
    let mut sent = design(json!([avatar_layer("a", 1000)]));
    sent["width"] = json!(1600);
    sent["height"] = json!(1000);
    assert!(parse_design(&sent, &no_art).is_ok());
    sent["layers"] = json!([avatar_layer("a", 1025)]);
    let errors = parse_design(&sent, &no_art).unwrap_err();
    assert!(
        messages(&errors).contains(&"Layer 1 (Avatar): an avatar is at most 1024 px across."),
        "{errors:?}"
    );
    // Four fit, a fifth does not, hidden or not.
    let four: Vec<Value> = (0..MAX_AVATARS).map(|i| avatar_layer(&format!("a{i}"), 200)).collect();
    sent["layers"] = json!(four);
    assert!(parse_design(&sent, &no_art).is_ok());
    let mut five = four.clone();
    let mut spare = avatar_layer("a9", 200);
    spare["hidden"] = json!(true);
    five.push(spare);
    sent["layers"] = json!(five);
    let errors = parse_design(&sent, &no_art).unwrap_err();
    assert_eq!(messages(&errors), ["A design has at most 4 avatars."]);
}

#[test]
fn what_the_layers_cost_to_draw_is_capped() {
    let shape = |id: &str| shape_layer(id, 1100, 500);
    let costs = |layers: &Value| {
        let parsed = parse_design(&design(layers.clone()), &no_art).unwrap();
        layers_cost(&parsed.layers)
    };
    // A shape costs its area, an avatar five times that, a text layer by the characters it can show.
    assert_eq!(costs(&json!([shape("s")])), 1100 * 500);
    assert_eq!(costs(&json!([avatar_layer("a", 200)])), 5 * 200 * 200);
    assert_eq!(costs(&json!([text_layer("Hi")])), 2 * TEXT_CHAR_COST);
    // The templates' kind of design sits far below the cap.
    let usual = json!([shape("s"), avatar_layer("a", 200), text_layer("WELCOME {user}")]);
    assert!(costs(&usual) < LAYER_COST_MAX / 4, "{}", costs(&usual));

    // 375 characters of text fit in the cap, a 376th does not.
    let texts = |lengths: &[usize]| -> Vec<Value> {
        lengths
            .iter()
            .enumerate()
            .map(|(i, n)| {
                let mut t = text_layer(&"x".repeat(*n));
                t["id"] = json!(format!("t{i}"));
                t
            })
            .collect()
    };
    let characters = (LAYER_COST_MAX / TEXT_CHAR_COST) as usize;
    assert_eq!(characters, 375);
    assert!(parse_design(&design(json!(texts(&[200, 175]))), &no_art).is_ok());
    let errors = parse_design(&design(json!(texts(&[200, 176]))), &no_art).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].field, "layers");
    assert!(
        errors[0]
            .message
            .starts_with("The visible layers are too heavy to draw quickly together (101% of what a card may use)"),
        "{}",
        errors[0].message
    );
    // Hiding a layer takes its cost away.
    let mut heavy = texts(&[200, 176]);
    heavy[0]["hidden"] = json!(true);
    assert!(parse_design(&design(json!(heavy)), &no_art).is_ok());
}

#[test]
fn placeholders_count_at_the_most_they_can_be() {
    assert_eq!(filled_length(""), 0);
    assert_eq!(filled_length("WELCOME"), 7);
    assert_eq!(filled_length("{user}"), NAME_MAX);
    assert_eq!(filled_length("a {server}!"), 3 + SERVER_MAX);
    assert_eq!(
        filled_length("{user}{username}{server}{members}{ordinal}"),
        2 * NAME_MAX + SERVER_MAX + 10 + 12
    );
    // What is not a placeholder is what it looks like; a character is a character, not a byte.
    assert_eq!(filled_length("{nope} { {user"), 14);
    assert_eq!(filled_length("😀李{user}"), 2 + NAME_MAX);
    assert_eq!(cost_share(&[]), 0);
}

#[test]
fn deliveries_follow_the_kind() {
    assert_eq!(check_delivery(CardKind::Welcome, "dm").unwrap(), Delivery::Dm);
    assert_eq!(check_delivery(CardKind::Goodbye, "dm").unwrap_err().field, "delivery");
    assert!(check_delivery(CardKind::Welcome, "loud").is_err());
    for d in Delivery::ALL {
        assert_eq!(Delivery::parse(d.key()), Some(d));
    }
    assert!(!Delivery::TextOnly.has_image() && !Delivery::ImageOnly.has_text());
    assert_eq!(CardKind::Goodbye.default_delivery(), Delivery::TextOnly);
    assert_eq!(Delivery::offered(CardKind::Goodbye).len(), 4);
    for kind in CardKind::ALL {
        assert_eq!(CardKind::parse(kind.key()), Some(kind));
    }
}
