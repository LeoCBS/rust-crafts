use std::{sync::Arc, time::Duration};

use anyhow::bail;
use gcloud_pubsub::{client::Client as ClientPubSub, publisher::PublisherConfig};
use google_cloud_googleapis::pubsub::v1::PubsubMessage;
use iceoryx2::{
    node::{Node, NodeBuilder},
    pending_response::PendingResponse,
    port::client::Client,
    prelude::{ServiceName, ZeroCopySend},
    service::{Service, ipc, ipc_threadsafe, local},
};
use tokio::time::sleep;
use zerocopy::{
    FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned,
    native_endian::{U16, U32},
};

#[tokio::main]
async fn main() {
    let endpoint = format!("localhost:{}", "8085");
    unsafe {
        std::env::set_var("PUBSUB_EMULATOR_HOST", endpoint.to_string());
        std::env::set_var("GOOGLE_CLOUD_PROJECT", "local-project");
    }
    let pubsub_client = ClientPubSub::new(Default::default())
        .await
        .expect("error to create pubsub client");
    let node = Arc::new(NodeBuilder::new().create::<ipc::Service>().unwrap());
    for _ in 0..10 {
        let client_to_for = pubsub_client.clone();
        let node_to_for = node.clone();
        tokio::spawn(async move {
            let node = Arc::new(NodeBuilder::new().create::<ipc::Service>().unwrap());
            let c = Connector::new(client_to_for, 10);
            c.run_iceoryx(node).await;
        });
        sleep(Duration::from_secs(2)).await;
    }
}

pub struct Connector {
    publisher_client: ClientPubSub,
    limit: u16,
}

impl Connector {
    pub fn new(publisher_client: ClientPubSub, limit: u16) -> Connector {
        Connector {
            publisher_client,
            limit,
        }
    }

    async fn run_iceoryx<S: Service>(&self, node: Arc<Node<S>>) {
        //let node = Arc::new(NodeBuilder::new().create::<local::Service>().unwrap());
        let service_name_iox = ServiceName::new("test/async").unwrap();
        let service = node
            .service_builder(&service_name_iox)
            .request_response::<ConsumerRequest, [u8]>()
            .open_or_create()
            .unwrap();

        let iox_client = service.client_builder().create().unwrap();
        let msgs = consume_messages(&iox_client, node.clone(), None, 10).unwrap();
        let topic = self.publisher_client.topic("lala");
        let publisher = topic.new_publisher(Some(PublisherConfig::default()));
        publisher
            .publish(PubsubMessage {
                data: vec![10],
                ..Default::default()
            })
            .await;
        println!("{:?}", node);
    }
}

fn consume_messages<S: Service>(
    iox_client: &Client<S, ConsumerRequest, (), [u8], ()>,
    client_node: Arc<Node<S>>,
    data_resp_opt: Option<DataResponse>,
    limit: u16,
) -> anyhow::Result<Vec<DataResponse>> {
    let mut consume_payload = None;
    if let Some(data_resp) = data_resp_opt {
        consume_payload = Some(Payload {
            date: Date {
                year: data_resp.year,
                month: data_resp.month,
                day: data_resp.day,
            },
            version: data_resp.sequence_version.into(),
            start_sequence_num: data_resp.sequence_number + 1,
            limit: limit,
        });
    }
    let req_payload = ConsumerRequest::Consume {
        payload: consume_payload,
    };
    let pending_response = send_iox_request(iox_client, client_node, req_payload)?;
    let header_size = std::mem::size_of::<ResponseConsume>();
    let mut responses: Vec<DataResponse> = vec![];
    while let Some(response) = pending_response.receive()? {
        let payload = response.payload();
        let mut offset = 0;
        while offset < payload.len() {
            let header_bytes = &payload[offset..offset + header_size];
            let header = match ResponseConsume::from_bytes(header_bytes) {
                Some(h) => h,
                None => bail!("error to decode response comsume from queue"),
            };
            offset += header_size;
            let frame_size = header.frame_length.get() as usize;
            let frame = &payload[offset..offset + frame_size];
            offset += frame_size;
            let data_frame: Vec<u8> = Vec::from(frame);
            responses.push(DataResponse {
                year: header.year.into(),
                month: header.month.into(),
                day: header.day.into(),
                sequence_version: header.sequence_version.into(),
                sequence_number: header.sequence_number.into(),
                data_frame,
            });
        }
    }
    Ok(responses)
}

fn send_iox_request<S: Service>(
    iox_client: &Client<S, ConsumerRequest, (), [u8], ()>,
    client_node: Arc<Node<S>>,
    req_payload: ConsumerRequest,
) -> anyhow::Result<PendingResponse<S, ConsumerRequest, (), [u8], ()>> {
    let request = iox_client.loan_uninit()?;
    let request = request.write_payload(req_payload);
    let pending_response = request.send()?;
    println!("esperando response");
    while !pending_response.has_response() {
        if let Err(e) = client_node.wait(Duration::from_millis(100)) {
            bail!("wainting client_node error: {e}");
        }
    }
    Ok(pending_response)
}

#[repr(C)]
#[derive(Debug, ZeroCopySend)]
#[type_name("ConsumerRequest")]
pub enum ConsumerRequest {
    Consume { payload: Option<Payload> },
}

#[repr(C)]
#[derive(Debug, ZeroCopySend)]
pub struct Payload {
    pub date: Date,
    pub version: u16,
    pub start_sequence_num: u32,
    pub limit: u16,
}

#[repr(C)]
#[derive(Debug, ZeroCopySend)]
pub struct Date {
    pub year: u16,
    pub month: u8,
    pub day: u8,
}

#[derive(Clone, Debug)]
struct DataResponse {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub sequence_version: u16,
    pub sequence_number: u32,
    data_frame: Vec<u8>,
}

#[derive(Debug, FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct ResponseConsume {
    pub year: U16,
    pub month: u8,
    pub day: u8,
    pub sequence_version: U16,
    pub sequence_number: U32,
    pub frame_length: U16,
}

impl ResponseConsume {
    pub fn from_bytes(bytes: &[u8]) -> Option<&ResponseConsume> {
        ResponseConsume::ref_from_bytes(bytes).ok()
    }
}

impl AsRef<[u8]> for ResponseConsume {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}
