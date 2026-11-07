//! Pixel cake card used for birthday announcements and previews.

use super::{PIXEL, SANS, text};
use crate::{
    Theme,
    svg::{background, escape, fit_laid_out},
};
use pinchora_core::{i18n::Lang, t};
use std::fmt::Write;

pub struct BirthdayCard {
    pub lang: Lang,
    pub name: String,
    pub age: Option<i32>,
    pub badge: bool,
}

impl BirthdayCard {
    pub fn to_svg(&self) -> String {
        let t = Theme::reef();
        let mut out = String::with_capacity(7000);
        let _ = write!(
            out,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="900" height="360" viewBox="0 0 900 360">"#
        );
        background(&mut out, 900.0, 360.0, &t.bg_top, &t.bg_bottom);
        t.panel(&mut out, 24.0, 24.0, 852.0, 312.0, 8.0);
        let _ = write!(
            out,
            r#"<path d="M70 247H304V271H70Z" fill="{}"/><path d="M86 178H288V246H86Z" fill="{}"/><path d="M96 178H278V194H96Z" fill="{}"/><path d="M112 151H126V178H112ZM180 151H194V178H180ZM248 151H262V178H248Z" fill="{}"/><path d="M115 132L122 144L115 153L108 144ZM183 132L190 144L183 153L176 144ZM251 132L258 144L251 153L244 144Z" fill="{}"/><path d="M86 213H288V224H86Z" fill="{}"/>"#,
            t.ink, t.accent, t.gold, t.ink, t.gold, t.gold
        );
        text(
            &mut out,
            350.0,
            92.0,
            19.0,
            PIXEL,
            false,
            &t.accent,
            "start",
            &escape(&self.lang.upper(&t!(self.lang, "birthdays.card-label"))),
        );
        text(
            &mut out,
            350.0,
            158.0,
            38.0,
            SANS,
            true,
            &t.ink,
            "start",
            &escape(&fit_laid_out(&self.name, SANS, true, 38.0, 490.0)),
        );
        let detail = self.age.map_or_else(
            || t!(self.lang, "birthdays.card-wish"),
            |n| t!(self.lang, "birthdays.card-age", age = n),
        );
        text(
            &mut out,
            350.0,
            208.0,
            20.0,
            SANS,
            false,
            &t.muted,
            "start",
            &escape(&fit_laid_out(&detail, SANS, false, 20.0, 490.0)),
        );
        if self.badge {
            text(
                &mut out,
                350.0,
                269.0,
                15.0,
                PIXEL,
                false,
                &t.gold,
                "start",
                &escape(&self.lang.upper(&t!(self.lang, "birthdays.cake-day"))),
            );
        }
        out.push_str("</svg>");
        out
    }
}
