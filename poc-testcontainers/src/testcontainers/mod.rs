pub mod pubsub {
    pub fn run_testcontainers() {}
}

#[cfg(test)]
pub mod test_pubsubcontainers {

    use std::{
        sync::mpsc::{Receiver, Sender, channel},
        time::Duration,
    };

    use gcloud_gax::{grpc::Code, retry::RetrySetting};
    use gcloud_pubsub::{
        client::Client, publisher::PublisherConfig, subscription::SubscriptionConfig,
        topic::TopicConfig,
    };
    use google_cloud_googleapis::pubsub::v1::PubsubMessage;
    use testcontainers::{GenericImage, ImageExt, core::IntoContainerPort, runners::AsyncRunner};
    use tokio::time::sleep;
    use tokio_stream::StreamExt;
    use tracing_subscriber::EnvFilter;

    #[tokio::test]
    async fn test_new_ioxpubsub() {
        let container = GenericImage::new("google/cloud-sdk", "438.0.0-emulators")
            .with_exposed_port(8085.tcp())
            .with_cmd(vec![
                "gcloud",
                "beta",
                "emulators",
                "pubsub",
                "start",
                "--host-port=0.0.0.0:8085",
                "--project=local-project",
            ])
            .start()
            .await
            .expect("Failed to start pubsub");
        let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(
            "debug,testcontainers=warn,h2=warn,hyper_util=warn,tower=warn,tonic=info,bollard=info",
            //"debug",
        ))
        .try_init();
        let pubsub_port = container.get_host_port_ipv4(8085).await.unwrap();
        let (frame_tx, frame_rx): (Sender<Vec<u8>>, Receiver<Vec<u8>>) = channel();
        setup_pubsub("lala", pubsub_port, frame_tx).await;
        tracing::debug!("msg received {:?}", frame_rx.recv().unwrap())
    }

    async fn setup_pubsub(service_name: &str, pubsub_port: u16, frame_tx: Sender<Vec<u8>>) {
        let pb_client = get_pubsub_client(pubsub_port).await;
        let topic_name = service_name.replace("/", "");
        let retry = RetrySetting {
            from_millis: 10,
            max_delay: Some(Duration::from_secs(5)),
            factor: 1u64,
            take: 5,
            codes: vec![
                Code::Unavailable,
                Code::Unknown,
                Code::Aborted,
                Code::Cancelled,
            ],
        };
        let mut topic_config = TopicConfig::default();
        topic_config.message_retention_duration = Some(Duration::from_secs(60 * 60));
        let topic_status = pb_client
            .create_topic(&topic_name, Some(topic_config), Some(retry))
            .await;
        //TODO find a way to remove this match, create topic doesn't works with unwrap func x_x
        let topic_created = match topic_status {
            Ok(t) => {
                tracing::debug!("topic created {:?}", t);
                Some(t)
            }
            Err(e) => {
                tracing::warn!("error to create pubsub topic {}", e.code());
                None
            }
        };

        let config = SubscriptionConfig {
            enable_message_ordering: true,
            ..Default::default()
        };

        let subscription = pb_client
            .create_subscription("mdstream", &topic_name, config, None)
            .await
            .unwrap();
        tokio::spawn(async move {
            tracing::debug!("criou 2");
            let subscription = subscription.clone();
            let mut receiver = subscription.subscribe(None).await.unwrap();
            while let Some(msg) = receiver.next().await {
                tracing::debug!("Message received: {:?}", msg.message.data.clone());
                frame_tx.send(msg.message.data.clone()).unwrap();
                msg.ack().await.unwrap();
            }
        });
        sleep(Duration::from_secs(2)).await;
        let config_publisher = PublisherConfig::default();
        let publisher = topic_created.unwrap().new_publisher(Some(config_publisher));

        let message = PubsubMessage {
            data: "10".into(),
            ordering_key: String::from("ordering"),
            ..Default::default()
        };
        publisher.publish(message).await;
        sleep(Duration::from_secs(2)).await;
        tracing::debug!("criou");
    }

    pub async fn get_pubsub_client(pubsub_port: u16) -> Client {
        let endpoint = format!("localhost:{}", pubsub_port);
        unsafe {
            std::env::set_var("PUBSUB_EMULATOR_HOST", endpoint.to_string());
            std::env::set_var("GOOGLE_CLOUD_PROJECT", "local-project");
        }
        Client::new(Default::default())
            .await
            .expect("error to create pubsub client")
    }
}
