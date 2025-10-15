use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread::{self, sleep},
    time::Duration,
};

use anyhow::{Ok, Result};
use iceoryx2::{
    node::{self, Node, NodeBuilder},
    port::{listener::Listener, subscriber::Subscriber, update_connections::UpdateConnections},
    prelude::{CallbackProgression, EventId, ServiceName},
    service::{Service, ipc},
    waitset::{WaitSetAttachmentId, WaitSetBuilder},
};

const CYCLE_TIME: Duration = Duration::from_millis(100);

pub enum PubSubEvent {
    PublisherConnected = 0,
    PublisherDisconnected = 1,
    SubscriberConnected = 2,
    SubscriberDisconnected = 3,
    SentSample = 4,
    ReceivedSample = 5,
    SentHistory = 6,
    ProcessDied = 7,
    SubscribeReady = 8,
    Unknown,
}

impl From<PubSubEvent> for EventId {
    fn from(value: PubSubEvent) -> Self {
        EventId::new(value as usize)
    }
}

impl From<EventId> for PubSubEvent {
    fn from(value: EventId) -> Self {
        match value.as_value() {
            0 => PubSubEvent::PublisherConnected,
            1 => PubSubEvent::PublisherDisconnected,
            2 => PubSubEvent::SubscriberConnected,
            3 => PubSubEvent::SubscriberDisconnected,
            4 => PubSubEvent::SentSample,
            5 => PubSubEvent::ReceivedSample,
            6 => PubSubEvent::SentHistory,
            7 => PubSubEvent::ProcessDied,
            8 => PubSubEvent::SubscribeReady,
            _ => PubSubEvent::Unknown,
        }
    }
}

fn main() -> Result<()> {
    let service_name = "pubsub/test";

    let (started_tx, started_rx): (Sender<()>, Receiver<()>) = mpsc::channel();
    let daemon_thread = thread::spawn(move || {
        start_central_daemon(service_name, started_tx).expect("error to start daemon")
    });
    started_rx.recv().unwrap();
    thread::spawn(move || produce(service_name).expect("error to produce"));
    sleep(Duration::from_secs(2));
    thread::spawn(move || consume(service_name).expect("error to consume"));
    daemon_thread.join().unwrap();
    Ok(())
}

fn start_central_daemon(service_name: &str, started_tx: Sender<()>) -> Result<()> {
    let node = NodeBuilder::new()
        .name(&"central daemon".try_into()?)
        .create::<ipc::Service>()?;

    // The central daemon is responsible to create all services before hand and the other processes
    // just open the communication resources and start communicating.
    let _service_pubsub_1 = node
        .service_builder(&ServiceName::new(service_name)?)
        .publish_subscribe::<u64>()
        // We use here open_or_create so that, in case of a crash of the central daemon, it can
        // be restarted.
        .open_or_create()?;

    let _service_event_1 = node
        .service_builder(&ServiceName::new(service_name)?)
        .event()
        // Defines the maximum timespan between two notifications for this service. The user of a
        // notifier that send a notification after the deadline was already reached, receives an
        // MISSED_DEADLINE error after the notification was delivered.
        .deadline(Duration::from_millis(5000))
        // Whenever a new notifier is created the PublisherConnected event is emitted. this makes
        // sense since in this example a notifier is always created after a new publisher was
        // created.
        // The task of the notifier/event is it to inform and wake up other processes when certain
        // system event have happened.
        .notifier_created_event(PubSubEvent::PublisherConnected.into())
        .notifier_dropped_event(PubSubEvent::PublisherDisconnected.into())
        // This event is emitted when either the central daemon or a decentralized process detects
        // a dead node and cleaned up all of its stale resources succesfully.
        .notifier_dead_event(PubSubEvent::ProcessDied.into())
        .open_or_create()?;
    let waitset = WaitSetBuilder::new().create::<ipc::Service>()?;
    let _cycle_guard = waitset.attach_interval(CYCLE_TIME);

    println!("Central daemon up and running.");
    started_tx.send(()).unwrap();
    waitset.wait_and_process(|_| CallbackProgression::Continue)?;
    println!("exiting");
    Ok(())
}

fn consume(service_name: &str) -> Result<()> {
    let node = NodeBuilder::new().create::<ipc::Service>()?;
    let service_name = &ServiceName::new(service_name)?;

    let service = node
        .service_builder(service_name)
        .publish_subscribe::<u64>()
        .open_or_create()?;

    let service_event = node.service_builder(service_name).event().open()?;
    let listener = service_event.listener_builder().create()?;

    let subscriber = service.subscriber_builder().create()?;

    let waitset = WaitSetBuilder::new().create::<ipc::Service>()?;
    let deadline = listener.deadline().unwrap_or(CYCLE_TIME);
    let listener_1_guard = waitset.attach_deadline(&listener, deadline)?;

    let on_event = |attachment_id: WaitSetAttachmentId<ipc::Service>| {
        if attachment_id.has_event_from(&listener_1_guard) {
            handle_incoming_event(&listener, &subscriber, &service_name);
        }
        CallbackProgression::Continue
    };

    //notifier publishers to start
    notify_sub_ready(service_name, &node)?;
    println!("subscriber ready");

    waitset.wait_and_process(on_event)?;
    println!("exit");
    Ok(())
}

fn notify_sub_ready<S: Service>(service_name: &str, node: &Node<S>) -> Result<()> {
    let service_name = &ServiceName::new(&format!("{}-healthz", service_name))?;
    let service_event = node
        .service_builder(service_name)
        .event()
        .open_or_create()?;
    let notifier = service_event
        .notifier_builder()
        .default_event_id(PubSubEvent::SubscribeReady.into())
        .create()?;

    notifier.notify()?;
    Ok(())
}

fn handle_incoming_event(
    listener: &Listener<ipc::Service>,
    subscriber: &Subscriber<ipc::Service, u64, ()>,
    service_name: &ServiceName,
) {
    listener
        .try_wait_all(|event_id| {
            if event_id == PubSubEvent::ProcessDied.into() {
                println!("{service_name}: process died!");
            } else if event_id == PubSubEvent::PublisherConnected.into() {
                println!("{service_name}: publisher connected!");
            } else if event_id == PubSubEvent::PublisherDisconnected.into() {
                println!("{service_name}: publisher disconnected!");
            } else if event_id == PubSubEvent::SentSample.into() {
                if let Some(sample) = subscriber.receive().expect("") {
                    println!("{}: Received sample {} ...", service_name, *sample)
                }
            }
        })
        .expect("");
}

fn produce(service_name: &str) -> Result<()> {
    let node = NodeBuilder::new().create::<ipc::Service>()?;

    let service_name = &ServiceName::new(service_name)?;
    let service = node
        .service_builder(service_name)
        .publish_subscribe::<u64>()
        .open_or_create()?;
    let service_event = node
        .service_builder(&ServiceName::new(service_name)?)
        .event()
        .open()?;

    let publisher = service.publisher_builder().create()?;
    let notifier = service_event
        .notifier_builder()
        .default_event_id(PubSubEvent::SentSample.into())
        .create()?;

    wait_until_subscriber_is_ready(service_name, &node)?;

    let waitset = WaitSetBuilder::new().create::<ipc::Service>()?;

    let _cycle_guard = waitset.attach_interval(CYCLE_TIME);
    let mut counter: u64 = 0;

    waitset.wait_and_process(|_| {
        println!("{service_name}: Send sample {counter} ...");
        publisher
            .send_copy(counter)
            .expect("sample delivery successful.");
        notifier.notify().expect("notification successful.");
        counter += 1;
        if counter > 25 {
            CallbackProgression::Stop
        } else {
            CallbackProgression::Continue
        }
    })?;

    println!("exit");
    Ok(())
}

fn wait_until_subscriber_is_ready(service_name: &str, node: &Node<ipc::Service>) -> Result<()> {
    let service_name = &ServiceName::new(&format!("{}-healthz", service_name))?;
    let service_event = node
        .service_builder(service_name)
        .event()
        .open_or_create()?;
    let listener = service_event.listener_builder().create()?;

    let waitset = WaitSetBuilder::new().create::<ipc::Service>()?;
    let deadline = listener.deadline().unwrap_or(CYCLE_TIME);
    let listener_1_guard = waitset.attach_deadline(&listener, deadline)?;

    let on_event = |attachment_id: WaitSetAttachmentId<ipc::Service>| {
        if attachment_id.has_event_from(&listener_1_guard) {
            println!("subscriber ready");
            CallbackProgression::Stop
        } else {
            println!("wait until subscriber is done");
            CallbackProgression::Continue
        }
    };
    waitset.wait_and_process(on_event)?;
    Ok(())
}
