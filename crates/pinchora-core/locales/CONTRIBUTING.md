# Translating Pinchora

Every user-facing text lives in a catalog here: `locales/<code>/<area>.toml`, one folder per
language, one file per part of the bot. The build embeds every file it finds (`build.rs`), so a new
file needs no list.

## Writing text in code

```rust
use pinchora_core::t;

cx.reply_ephemeral(t!(cx.lang(), "fish.no-rod")).await?;
let line = t!(cx.lang(), "fish.caught", fish = name, count = n);
```

- `cx.lang()` is the language this interaction is answered in: the player's `/language` pick, their
  Discord language, the server's default (`/config language`), English.
- Messages that answer nobody in particular (welcomes, level-ups, logs, giveaways) use
  `app.guild_lang(guild).await`.
- Cards take a `lang: Lang` field and look labels up with `t!(self.lang, "card.…")`.
- Keys are dotted: `[fish]` + `caught = "…"` is `fish.caught`. Name keys after the command or card.
  A key may live in any file of the language, but only once.
- `{name}` is an argument. A table of `one`/`other` is a plural, picked by the `count` argument:
  `days = { one = "{count} day", other = "{count} days" }`. Turkish needs only `other`.
- Pixel-font labels are drawn in capitals and must stay short. The pixel font has no İ ı Ş ş Ğ ğ:
  they are drawn as I i S s G g. Look at the PNGs the simulator writes.

## Slash commands

English descriptions stay in the `#[command(desc = …)]` and `///` docs. Other languages translate
them in `locales/<code>/commands.toml` under `[cmd]`, keyed by the path as typed:
`"house edit" = "…"`, an option `"house edit room" = "…"`, a choice
`"language set language=Automatic" = "…"`. Registration fills Discord's `description_localizations`
from these (names stay English). Discord allows 100 characters.

## Checks

- `cargo test -p pinchora-core i18n` fails when a key is in one language and not the other, is
  defined twice, or takes different `{arguments}`.
- `cargo test -p pinchora-commands lang` fails when a `commands.toml` key names no real command or
  is over 100 characters, and checks the registration JSON.
- `cargo test -p pinchora-commands untranslated -- --ignored --nocapture` lists everything a
  language has not translated yet, commands included, ready to paste.

## Adding a language

1. Add the variant to `Lang` in `crates/pinchora-core/src/i18n.rs`, with its `code`, `native_name`,
   the Discord locales it covers (`discord_locales`, e.g. `["de"]`) and `from_discord`.
2. Copy `locales/en/` to `locales/<code>/` and translate every file; add `commands.toml`.
3. Add a `LangChoice` variant in `crates/pinchora-commands/src/commands/language.rs`.
4. Run the checks above until they pass, then look at the cards in `target/sim/`.
