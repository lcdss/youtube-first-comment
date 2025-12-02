mod utils;

use clap::Parser;
use core::f64;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use yfc::{Result, Yfc};

#[derive(Parser)]
#[command(
  version,
  name = "yfc",
  about = "A tool to create a new comment on YouTube when a new video is published for the specified channel"
)]
struct Args {
  /// Google client ID
  #[arg(long)]
  google_client_id: String,

  /// Google client secret
  #[arg(long)]
  google_client_secret: String,

  /// The comment body
  #[arg(long)]
  comment: String,

  /// YouTube channel ID
  #[arg(long)]
  channel_id: String,

  /// Poll interval (in seconds)
  #[arg(long, default_value = "60")]
  poll_interval: f32,

  /// Max wait time (in minutes)
  #[arg(long, required = false)]
  wait_limit: Option<u32>,
}

#[tokio::main]
async fn main() -> Result<()> {
  let args = Args::parse();
  let yfc = Yfc::new(&args.google_client_id, &args.google_client_secret).await?;
  let uploads_playlist_id = yfc.get_uploads_playlist_id(&args.channel_id).await?;

  println!("Uploads Playlist ID: {uploads_playlist_id}");

  let latest_video_id = yfc.get_latest_video_id(&uploads_playlist_id).await?;
  let started_at = Instant::now();
  let wait_limit = args.wait_limit.map_or(f64::INFINITY, |value| value as f64);

  let result = loop {
    sleep(Duration::from_secs_f32(args.poll_interval)).await;

    let elapsed_minutes = started_at.elapsed().as_secs() as f64 / 60.0;

    if elapsed_minutes >= wait_limit {
      println!("The wait limit of {} minutes was reached", wait_limit);
      break Ok(());
    }

    if let Some(new_video_id) = yfc.get_latest_video_id(&uploads_playlist_id).await? {
      println!("Latest Video ID: {new_video_id}");

      if Some(&new_video_id) != latest_video_id.as_ref() {
        println!("New Video Published: {new_video_id}");

        break match yfc.create_comment(&new_video_id, &args.comment).await {
          Ok(_) => {
            println!("Comment created successfuly!");
            Ok(())
          }
          Err(e) => Err(e),
        };
      }
    }
  };

  println!(
    "The elapsed time was {}",
    utils::format_duration(started_at.elapsed().as_secs())
  );

  result
}
