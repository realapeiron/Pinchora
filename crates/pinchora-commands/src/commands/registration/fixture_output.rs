// Generated with the same compatibility writer as production registrations.
fn component(custom_id: &str) -> &'static str {
    match custom_id.split(':').next() {
        // fixture-registrations:components:start
        Some("fixture") => button(), // fixture-registrations:components:end
        _ => "unknown",
    }
}
fn owner(prefix: &str) -> &'static str {
    match prefix {
        // fixture-registrations:owners:start
        "fixture" => "registration-fixture", // fixture-registrations:owners:end
        _ => "unknown",
    }
}
const PRIVATE: &[&str] = &[
    // fixture-registrations:private:start
    "registration-fixture", // fixture-registrations:private:end
];
const OPENS_MODAL: &[&str] = &[
    // fixture-registrations:opens_modal:start
    "fixture:open", // fixture-registrations:opens_modal:end
];
const HELP: &[&str] = &[
    // fixture-registrations:help_start:start
    "registration-fixture", // fixture-registrations:help_start:end
];
const SLASH_ONLY: &[&str] = &[
    // fixture-registrations:slash_only:start
    "registration-fixture", // fixture-registrations:slash_only:end
];
const ALIASES: &[(&str, &str)] = &[
    // fixture-registrations:aliases:start
    ("fixture", "registration-fixture"), // fixture-registrations:aliases:end
];

fn module(prefix: &str) -> &'static str {
    match prefix {
        // fixture-registrations:component_gates:start
        "fixture" => "house",
        // fixture-registrations:component_gates:end
        _ => "unknown",
    }
}
