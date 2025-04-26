# Tracing

## Table of Contents
- [Tracing](#tracing)
  - [About tracing](#about-tracing)
  - [Simple main with span, event and subscriber](#simple-main-with-span-event-and-subscriber)
    - [Spans](#spans)
    - [Events](#events)
    - [Subscriber](#subscriber)
  - [Example](#example)

## About tracing

Simple project to understand and integrate concepts about tracing and subscriber crates.
To see a full documentation about tracing and subscriber access these links:
 * https://docs.rs/tracing/latest/tracing/
 * https://docs.rs/tracing-subscriber/latest/tracing_subscriber/

## Simple main with span, event and subscriber

Tracing is a concept to instrument programs to collect structured event-based diagnostic information. In resume, to produce and collect theses structured information we will use these three main concepts: span, event and subscriber.

### Spans

Span represent represent a period of time with a beginning and an end.

### Events

Event represent a moment in time comparable with the log records, the different is that an event could be inside and span context, we will understand more below with an example.

### Subscriber

A subscriber is responsible to record or aggregate span and event information.

## Example

```
use std::{thread::sleep, time};

use tracing::{Level, event, instrument, span};

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
}

#[instrument]
pub fn my_function(my_arg: usize) {
    // This event will be recorded inside a span named `my_function` with the
    // field `my_arg`.
    event!(Level::INFO, "inside my_function!");
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
```

Output program

```
2025-04-27T12:20:15.136726Z  INFO tracing: event without span context
2025-04-27T12:20:15.136779Z  INFO my_span{answer=42 name="leo"}: tracing: Doing something important
2025-04-27T12:20:15.136794Z  INFO my_span{answer=42 name="leo"}: tracing: something has happened!
2025-04-27T12:20:16.137039Z  INFO my_span{answer=42 name="leo"}:my_function{my_arg=20}: tracing: inside my_function!
2025-04-27T12:20:16.137241Z  INFO my_span{answer=42 name="leo"}:my_method{an_interesting_argument=10 self.name=leo}: tracing: inside my_method!
```

