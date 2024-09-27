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
    client: &reqwest::Client,
    api_key: &str,
    channel_id: &str,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let response = client
        .get(format!(
            "https://www.googleapis.com/youtube/v3/channels?part=contentDetails&id={}&key={}",
            channel_id, api_key
        ))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    let playlist_id = response["items"][0]["contentDetails"]["relatedPlaylists"]["uploads"]
        .as_str()
        .map(String::from);

    Ok(playlist_id)
}

async fn get_latest_video_id(
    client: &reqwest::Client,
    api_key: &str,
    playlist_id: &str,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let response = client
        .get(format!(
            "https://www.googleapis.com/youtube/v3/playlistItems?part=snippet&playlistId={}&maxResults=1&key={}",
            playlist_id, api_key
        ))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    let description = response["items"][0]["snippet"]["description"].to_string();

    if description.contains("#shorts") {
        println!("Latest video is a short");
        return Ok(None);
    }

    let video_id = response["items"][0]["snippet"]["resourceId"]["videoId"]
        .as_str()
        .map(String::from);

    Ok(video_id)
}

async fn post_comment(
    client: &YouTube<HttpsConnector<HttpConnector>>,
    video_id: &str,
) -> Result<(), Box<dyn Error>> {
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

    println!("Posting the comment...");

    let response = client.comment_threads().insert(comment_thread).doit().await;

    match response {
        Ok(_) => {
            println!("Comment posted successfully!");
            Ok(())
        }
        Err(err) => Err(Box::new(err)),
    }
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

    let api_key = env::var("API_KEY")?;
    let channel_id = env::var("CHANNEL_ID")?;

    println!("API_KEY: {api_key}");
    println!("CHANNEL_ID: {channel_id}");

    let client = reqwest::Client::new();
    let youtube_client = get_youtube_client().await?;

    let uploads_playlist_id = get_uploads_playlist_id(&client, &api_key, &channel_id)
        .await?
        .ok_or("Failed to get uploads playlist ID")?;

    println!("Uploads Playlist ID: {uploads_playlist_id}");

    let mut retries = 0;
    let latest_video_id = get_latest_video_id(&client, &api_key, &uploads_playlist_id)
        .await
        .unwrap_or(None);

    loop {
        sleep(Duration::from_secs(POOL_INTERVAL)).await;

        if let Some(new_video_id) =
            get_latest_video_id(&client, &api_key, &uploads_playlist_id).await?
        {
            println!("Latest Video ID: {new_video_id}");

            if Some(new_video_id.clone()) != latest_video_id {
                println!("New Video Published: {new_video_id}");

                match post_comment(&youtube_client, &new_video_id).await {
                    Ok(_) => process::exit(0),
                    Err(e) => {
                        eprintln!("Failed to post comment: {e}");
                        retries += 1;
                    }
                }
            }
        }

        if retries == MAX_RETRIES {
            process::exit(-1);
        }
    }
}
