use clap::Parser;
use dirs::cache_dir;
use google_youtube3::{
  api::{Comment, CommentSnippet, CommentThread, CommentThreadSnippet},
  hyper::{client::HttpConnector, Client},
  hyper_rustls::{HttpsConnector, HttpsConnectorBuilder},
  oauth2::{ApplicationSecret, InstalledFlowAuthenticator, InstalledFlowReturnMethod},
  YouTube,
};
use std::{
  error::Error,
  fs, io,
  path::PathBuf,
  time::{Duration, Instant},
};
use tokio::time::sleep;

#[derive(Parser)]
struct Args {
  /// Google client ID
  #[arg(long, required = true)]
  google_client_id: String,

  /// Google client secret
  #[arg(long, required = true)]
  google_client_secret: String,

  /// The comment body
  #[arg(long, required = true)]
  comment: String,

  /// YouTube channel ID
  #[arg(long, required = true)]
  channel_id: String,

  /// Pool interval (in seconds)
  #[arg(long, default_value = "60")]
  pool_interval: u64,

  /// Max wait time (in minutes)
  #[arg(long)]
  wait_limit: u64,
}

type YoutubeClient = YouTube<HttpsConnector<HttpConnector>>;

const MAX_RETRIES: u8 = 3;

async fn get_uploads_playlist_id(client: &YoutubeClient, channel_id: &str) -> Option<String> {
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

async fn get_latest_video_id(client: &YoutubeClient, playlist_id: &str) -> Option<String> {
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
        // Check for #shorts in the description
        if snippet.description.unwrap_or_default().contains("#shorts") {
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

async fn post_comment(client: &YoutubeClient, video_id: &str, comment: &str) -> google_youtube3::Result<()> {
  let comment_thread = CommentThread {
    snippet: Some(CommentThreadSnippet {
      video_id: Some(video_id.into()),
      top_level_comment: Some(Comment {
        snippet: Some(CommentSnippet {
          text_original: Some(comment.into()),
          ..Default::default()
        }),
        ..Default::default()
      }),
      ..Default::default()
    }),
    ..Default::default()
  };

  client.comment_threads().insert(comment_thread).doit().await.map(|_| ())
}

fn get_token_storage_path() -> PathBuf {
  cache_dir()
    .expect("Could not find the cache directory")
    .join("yfc")
    .join("token.json")
}

async fn get_youtube_client(client_id: &str, client_secret: &str) -> io::Result<YoutubeClient> {
  let secret = ApplicationSecret {
    client_id: client_id.into(),
    client_secret: client_secret.into(),
    auth_uri: "https://accounts.google.com/o/oauth2/auth".into(),
    token_uri: "https://oauth2.googleapis.com/token".into(),
    ..Default::default()
  };

  let token_path = get_token_storage_path();
  let app_cache_path = token_path.parent().unwrap();

  if !fs::exists(app_cache_path)? {
    fs::create_dir(app_cache_path)?;
  }

  let auth = InstalledFlowAuthenticator::builder(secret, InstalledFlowReturnMethod::HTTPRedirect)
    .persist_tokens_to_disk(token_path)
    .build()
    .await?;

  let scopes = [
    "https://www.googleapis.com/auth/youtube.readonly",  // To read data
    "https://www.googleapis.com/auth/youtube.force-ssl", // To create comments
  ];

  // This will request both scopes at once instead of having to wait for a comment creation to log in again and give the
  // other scope
  auth.token(&scopes).await.map_err(io::Error::other)?;

  let https_connector = HttpsConnectorBuilder::new()
    .with_native_roots()?
    .https_only()
    .enable_http2()
    .build();

  let https_client = Client::builder().build(https_connector);

  Ok(YouTube::new(https_client, auth))
}

fn format_duration(seconds: u64) -> String {
  let hours = seconds / 3600;
  let minutes = (seconds % 3600) / 60;
  let seconds = seconds % 60;

  let mut parts = Vec::new();

  if hours > 0 {
    parts.push(format!("{}h", hours));
  }
  if minutes > 0 {
    parts.push(format!("{}m", minutes));
  }
  if seconds > 0 {
    parts.push(format!("{}s", seconds));
  }

  parts.join(" ")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
  let args = Args::parse();
  let client = get_youtube_client(&args.google_client_id, &args.google_client_secret).await?;
  let uploads_playlist_id = get_uploads_playlist_id(&client, &args.channel_id)
    .await
    .ok_or("Failed to get uploads playlist ID")?;

  println!("Uploads Playlist ID: {uploads_playlist_id}");

  let mut retries = 0;
  let latest_video_id = get_latest_video_id(&client, &uploads_playlist_id).await;
  let started_at = Instant::now();

  loop {
    sleep(Duration::from_secs(args.pool_interval)).await;

    let elapsed_minutes = started_at.elapsed().as_secs() as f64 / 60.0;

    if elapsed_minutes >= args.wait_limit as f64 {
      println!("The wait limit of {} minutes was reached", args.wait_limit);
      break;
    }

    if let Some(new_video_id) = get_latest_video_id(&client, &uploads_playlist_id).await {
      println!("Latest Video ID: {new_video_id}");

      if Some(new_video_id.clone()) != latest_video_id {
        println!("New Video Published: {new_video_id}");

        match post_comment(&client, &new_video_id, &args.comment).await {
          Ok(_) => {
            println!("Comment created successfuly!");
            break;
          }
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

  println!(
    "The elapsed time was {}",
    format_duration(started_at.elapsed().as_secs())
  );

  Ok(())
}
