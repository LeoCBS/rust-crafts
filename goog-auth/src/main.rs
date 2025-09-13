use google_cloud_storage::{
    client::{Client, ClientConfig},
    http::objects::{download::Range, get::GetObjectRequest},
};

#[tokio::main]
async fn main() {
    let config = ClientConfig::default().with_auth().await.unwrap();
    let client = Client::new(config);

    let data = client
        .download_object(
            &GetObjectRequest {
                bucket: "bucket".to_string(),
                object: "file.png".to_string(),
                ..Default::default()
            },
            &Range::default(),
        )
        .await;
}
