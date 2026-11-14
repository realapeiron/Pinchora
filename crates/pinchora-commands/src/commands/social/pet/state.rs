use super::*;

// /Pet state

/// A pet's needs as of `now`. They drift from `needs_at`; a pet on a trip has its `needs_at` in
/// the future, so they stay as they were when it left.
pub fn current_needs(pet: &PetRow, now: DateTime<Utc>) -> Needs {
    let hours = (now - pet.needs_at).num_seconds().max(0) as f32 / 3600.0;
    Needs {
        hunger: pet.hunger,
        fun: pet.fun,
        energy: pet.energy,
        hygiene: pet.hygiene,
    }
    .after(hours)
}

/// Whether the pet fell asleep: nobody looked after it from the day its hunger, fun and hygiene
/// all ran out until a day later. Worked out from the stored needs, so no job has to watch for it.
pub fn is_asleep(pet: &PetRow, now: DateTime<Utc>) -> bool {
    if pet.is_away(now) {
        return false;
    }
    let stored = Needs {
        hunger: pet.hunger,
        fun: pet.fun,
        energy: pet.energy,
        hygiene: pet.hygiene,
    };
    (now - pet.needs_at).num_seconds() as f32 / 3600.0 >= stored.sleeps_after_hours()
}

pub(super) fn status(pet: &PetRow, now: DateTime<Utc>) -> PetStatus {
    if pet.is_away(now) {
        PetStatus::Away
    } else if is_asleep(pet, now) {
        PetStatus::Asleep
    } else if pet.has_souvenir(now) {
        PetStatus::Back
    } else {
        PetStatus::Awake
    }
}

/// "2 h 5 min", "25 min", "under a minute".
pub(super) fn span(secs: i64, lang: Lang) -> String {
    let minutes = (secs.max(0) + 59) / 60;
    match (minutes / 60, minutes % 60) {
        (0, 0) => t!(lang, "pet.span.under-minute"),
        (0, m) => t!(lang, "pet.span.minutes", m = m),
        (h, 0) => t!(lang, "pet.span.hours", h = h),
        (h, m) => t!(lang, "pet.span.hours-minutes", h = h, m = m),
    }
}

/// The name of the place a pet is away at or came back from.
pub(super) fn place_of(pet: &PetRow, lang: Lang) -> String {
    match pet.trip_dest.as_deref().and_then(trips::get) {
        Some(trip) => Content::get().name(&trip.id, lang).into_owned(),
        None => t!(lang, "pet.somewhere"),
    }
}

/// How long until `ends`, as `span` writes it; empty without an end.
pub(super) fn left_in(ends: Option<DateTime<Utc>>, now: DateTime<Utc>, lang: Lang) -> String {
    ends.map_or_else(String::new, |ends| span((ends - now).num_seconds(), lang))
}

/// The day number of `now`, for the daily free wake-up.
pub(super) fn day_of(now: DateTime<Utc>) -> i32 {
    now.timestamp().div_euclid(86_400) as i32
}

/// What a level-up says: the new level, and any trip it opens.
pub(super) fn level_up_note(lang: Lang, name: &str, from: i32, to: i32) -> String {
    let mut note = t!(lang, "pet.level-up", name = name, level = to);
    for trip in trips::all()
        .iter()
        .filter(|t| t.unlock_level > from && t.unlock_level <= to)
    {
        note.push('\n');
        note.push_str(&t!(
            lang,
            "pet.trip-unlocked",
            trip = Content::get().name(&trip.id, lang)
        ));
    }
    note
}
