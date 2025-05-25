use std::{thread::sleep, time};

use tracing::{Level, Span, event, instrument, span};

fn main() {
    // create a subsbcriber to get span and event information
    tracing_subscriber::fmt::init();
    event!(Level::INFO, "event without span context");
    let my_span = span!(Level::INFO, "my_span", answer = 42, name = "leo");
    let _enter = my_span.enter();
    tracing::info!("Doing something important");
    event!(Level::INFO, "something has happened!");
    sleep(time::Duration::from_secs(1));

    // more docs about nice instrument examples here https://docs.rs/tracing/latest/tracing/attr.instrument.html
    my_function(20);
    let mut my_type = MyType { name: "leo" };
    my_type.my_method(10);

    parent_span(&my_span);
}

#[instrument]
pub fn my_function(my_arg: usize) {
    // This event will be recorded inside a span named `my_function` with the
    // field `my_arg`.
    event!(Level::INFO, "inside my_function!");
}

pub fn parent_span(parent: &Span) {
    parent_span2(parent);
}
pub fn parent_span2(parent: &Span) {
    let sp = span!(parent: parent, Level::WARN, "parent func");
    let _enter_sp = sp.enter();
    tracing::warn!("error to parse fast_message");
}

#[derive(Debug)]
struct MyType {
    name: &'static str,
}

impl MyType {
    // This will skip the `data` field, but will include `self.name`,
    // formatted using `fmt::Display`.
    #[instrument(skip(self), fields(self.name = %self.name))]
    pub fn my_method(&mut self, an_interesting_argument: usize) {
        event!(Level::INFO, "inside my_method!");
    }
}
