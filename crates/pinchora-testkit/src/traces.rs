use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use tracing::{
    Dispatch, Subscriber,
    field::{Field, Visit},
    span::{Attributes, Id},
};
use tracing_subscriber::{Layer, layer::Context, prelude::*};

type RecordedSpans = Vec<(String, BTreeMap<String, String>)>;

#[derive(Clone, Default)]
pub struct Spans(Arc<Mutex<RecordedSpans>>);

impl Spans {
    pub fn dispatch(&self) -> Dispatch {
        Dispatch::new(tracing_subscriber::registry().with(self.clone()))
    }

    pub fn named(&self, name: &str) -> Vec<BTreeMap<String, String>> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|(n, _)| n == name)
            .map(|(_, fields)| fields.clone())
            .collect()
    }
}

struct Fields(BTreeMap<String, String>);
impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0.insert(field.name().into(), format!("{value:?}"));
    }
}

impl<S: Subscriber> Layer<S> for Spans {
    fn on_new_span(&self, attributes: &Attributes<'_>, _: &Id, _: Context<'_, S>) {
        let mut fields = Fields(BTreeMap::new());
        attributes.record(&mut fields);
        self.0
            .lock()
            .unwrap()
            .push((attributes.metadata().name().into(), fields.0));
    }
}
