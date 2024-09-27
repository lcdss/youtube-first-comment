use google_youtube3::{
    api::{Comment, CommentSnippet, CommentThread, CommentThreadSnippet},
    hyper::{client::HttpConnector, Client},
    hyper_rustls::{HttpsConnector, HttpsConnectorBuilder},
    oauth2::{ApplicationSecret, InstalledFlowAuthenticator, InstalledFlowReturnMethod},
    YouTube,
};
use serde::{Deserialize, Serialize};
use std::{env, error::Error, process, time::Duration};
use tokio::time::sleep;

#[derive(Debug, Deserialize, Serialize)]
struct YoutubeNotification {
    kind: String,
}

const MAX_RETRIES: u8 = 3;
const POOL_INTERVAL: u64 = 15;

async fn get_uploads_playlist_id(
    client: &YouTube<HttpsConnector<HttpConnector>>,
    channel_id: &str,
) -> Option<String> {
    let response = client
        .channels()
        .list(&vec!["contentDetails".into()])
        .add_id(channel_id)
        .doit()
        .await;

    if let Ok((_, result)) = response {
        result.items.and_then(|items| {
            items
                .first()
                .and_then(|item| item.content_details.as_ref())
                .and_then(|details| details.related_playlists.as_ref())
                .and_then(|playlists| playlists.uploads.clone())
        })
    } else {
        None
    }
}

async fn get_latest_video_id(
    client: &YouTube<HttpsConnector<HttpConnector>>,
    playlist_id: &str,
) -> Option<String> {
    let response = client
        .playlist_items()
        .list(&vec!["snippet".into()])
        .playlist_id(playlist_id)
        .max_results(1)
        .doit()
        .await;

    if let Ok((_, result)) = response {
        result
            .items
            .and_then(|items| items.first().cloned())
            .and_then(|item| item.snippet)
            .and_then(|snippet| {
                let description = snippet.description.unwrap_or_default();

                // Check for #shorts in the description
                if description.contains("#shorts") {
                    println!("Latest video is a short");
                    return None;
                }

                snippet
                    .resource_id
                    .as_ref()
                    .map(|resource_id| resource_id.video_id.clone())
                    .unwrap_or_default()
            })
    } else {
        None
    }
}

async fn post_comment(
    client: &YouTube<HttpsConnector<HttpConnector>>,
    video_id: &str,
) -> Result<(), google_youtube3::Error> {
    let comment = "Always first for you".to_string();

    let comment_thread = CommentThread {
        snippet: Some(CommentThreadSnippet {
            video_id: Some(video_id.to_string()),
            top_level_comment: Some(Comment {
                snippet: Some(CommentSnippet {
                    text_original: Some(comment),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        }),
        ..Default::default()
    };

    client
        .comment_threads()
        .insert(comment_thread)
        .doit()
        .await
        .map(|_| ())
}

async fn get_youtube_client() -> Result<YouTube<HttpsConnector<HttpConnector>>, Box<dyn Error>> {
    let secret = ApplicationSecret {
        client_id: env::var("GOOGLE_CLIENT_ID")?,
        client_secret: env::var("GOOGLE_CLIENT_SECRET")?,
        auth_uri: "https://accounts.google.com/o/oauth2/auth".into(),
        token_uri: "https://oauth2.googleapis.com/token".into(),
        ..Default::default()
    };

    let auth = InstalledFlowAuthenticator::builder(secret, InstalledFlowReturnMethod::HTTPRedirect)
        .persist_tokens_to_disk("token.json")
        .build()
        .await?;

    let https_connector = HttpsConnectorBuilder::new()
        .with_native_roots()?
        .https_only()
        .enable_http2()
        .build();

    let https_client = Client::builder().build(https_connector);

    Ok(YouTube::new(https_client, auth))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenvy::dotenv()?;

    let client = get_youtube_client().await?;

    let uploads_playlist_id = get_uploads_playlist_id(&client, &env::var("CHANNEL_ID")?)
        .await
        .ok_or("Failed to get uploads playlist ID")?;

    println!("Uploads Playlist ID: {uploads_playlist_id}");

    let mut retries = 0;
    let latest_video_id = get_latest_video_id(&client, &uploads_playlist_id).await;

    loop {
        sleep(Duration::from_secs(POOL_INTERVAL)).await;

        if let Some(new_video_id) = get_latest_video_id(&client, &uploads_playlist_id).await {
            println!("Latest Video ID: {new_video_id}");

            if Some(new_video_id.clone()) != latest_video_id {
                println!("New Video Published: {new_video_id}");

                match post_comment(&client, &new_video_id).await {
                    Ok(_) => process::exit(0),
                    Err(e) => {
                        eprintln!("Failed to post comment: {e}");
                        retries += 1;
                    }
                }
            }
        }

        if retries == MAX_RETRIES {
            panic!("Max tries to create a comment was reached")
        }
    }
}
