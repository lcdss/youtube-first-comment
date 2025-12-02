use dirs::cache_dir;
use google_youtube3::{
  api::{Comment, CommentSnippet, CommentThread, CommentThreadSnippet},
  hyper_rustls::{HttpsConnector, HttpsConnectorBuilder},
  hyper_util::{
    client::legacy::{connect::HttpConnector, Client},
    rt::TokioExecutor,
  },
  yup_oauth2::{ApplicationSecret, InstalledFlowAuthenticator, InstalledFlowReturnMethod},
  YouTube,
};
use std::{fmt, fs, io};

#[derive(Debug)]
pub enum YfcError {
  YouTubeApi(Box<google_youtube3::Error>),
  Authentication(io::Error),
  // NetworkError(String),
  UploadsPlaylistNotFound(String),
  TokenStorageError(io::Error),
  CommentPostFailed(String),
  // RateLimitExceeded,
  // ConfigError(String),
}

impl std::error::Error for YfcError {}

impl fmt::Display for YfcError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      YfcError::YouTubeApi(e) => write!(f, "YouTube API error: {}", e),
      YfcError::Authentication(e) => write!(f, "Authentication failed: {}", e),
      // YfcError::NetworkError(msg) => write!(f, "Network error: {}", msg),
      YfcError::UploadsPlaylistNotFound(channel_id) => {
        write!(f, "Uploads playlist not found for channel: {}", channel_id)
      }
      YfcError::TokenStorageError(e) => write!(f, "Failed to store token: {}", e),
      YfcError::CommentPostFailed(msg) => write!(f, "Failed to post comment: {}", msg),
      // YfcError::RateLimitExceeded => write!(f, "YouTube API rate limit exceeded"),
      // YfcError::ConfigError(msg) => write!(f, "Configuration error: {}", msg),
    }
  }
}

impl From<google_youtube3::Error> for YfcError {
  fn from(err: google_youtube3::Error) -> Self {
    YfcError::YouTubeApi(Box::new(err))
  }
}

impl From<io::Error> for YfcError {
  fn from(err: io::Error) -> Self {
    YfcError::Authentication(err)
  }
}

pub type Result<T> = std::result::Result<T, YfcError>;
type YoutubeClient = YouTube<HttpsConnector<HttpConnector>>;

pub struct Yfc {
  client: YoutubeClient,
}

impl Yfc {
  pub async fn new(client_id: &str, client_secret: &str) -> Result<Self> {
    let secret = ApplicationSecret {
      client_id: client_id.into(),
      client_secret: client_secret.into(),
      auth_uri: "https://accounts.google.com/o/oauth2/auth".into(),
      token_uri: "https://oauth2.googleapis.com/token".into(),
      ..Default::default()
    };

    let token_path = cache_dir()
      .expect("Could not find the cache directory")
      .join("yfc")
      .join("token.json");
    let app_cache_path = token_path.parent().unwrap();

    if !app_cache_path.exists() {
      fs::create_dir(app_cache_path).map_err(YfcError::TokenStorageError)?;
    }

    let auth = InstalledFlowAuthenticator::builder(secret, InstalledFlowReturnMethod::HTTPRedirect)
      .persist_tokens_to_disk(token_path)
      .build()
      .await
      .map_err(YfcError::TokenStorageError)?;

    let scopes = [
      "https://www.googleapis.com/auth/youtube.readonly",  // To read data
      "https://www.googleapis.com/auth/youtube.force-ssl", // To create comments
    ];

    // This will request both scopes at once instead of having to wait for a comment creation to log in again and give
    // the other scope
    auth.token(&scopes).await.map_err(io::Error::other)?;

    let https_connector = HttpsConnectorBuilder::new()
      .with_native_roots()?
      .https_only()
      .enable_http2()
      .build();

    let https_client = Client::builder(TokioExecutor::new()).build(https_connector);

    Ok(Self {
      client: YouTube::new(https_client, auth),
    })
  }

  pub async fn get_uploads_playlist_id(&self, channel_id: &str) -> Result<String> {
    let response = self
      .client
      .channels()
      .list(&vec!["contentDetails".into()])
      .add_id(channel_id)
      .doit()
      .await?;

    let (_, result) = response;

    result
      .items
      .and_then(|items| {
        items
          .first()
          .and_then(|item| item.content_details.as_ref())
          .and_then(|details| details.related_playlists.as_ref())
          .and_then(|playlists| playlists.uploads.clone())
      })
      .ok_or_else(|| YfcError::UploadsPlaylistNotFound(channel_id.to_string()))
  }

  pub async fn get_latest_video_id(&self, playlist_id: &str) -> Result<Option<String>> {
    let response = self
      .client
      .playlist_items()
      .list(&vec!["snippet".into()])
      .playlist_id(playlist_id)
      .max_results(1)
      .doit()
      .await?;

    let (_, result) = response;

    Ok(
      result
        .items
        .and_then(|items| items.first().cloned())
        .and_then(|item| item.snippet)
        .and_then(|snippet| {
          // Try to ignore #shorts by checking the description
          if snippet.description.unwrap_or_default().contains("#shorts") {
            println!("Latest video is a short");
            return None;
          }

          snippet
            .resource_id
            .as_ref()
            .map(|resource_id| resource_id.video_id.clone())
            .unwrap_or_default()
        }),
    )
  }

  pub async fn create_comment(&self, video_id: &str, comment: &str) -> Result<()> {
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

    self
      .client
      .comment_threads()
      .insert(comment_thread)
      .doit()
      .await
      .map_err(|e| YfcError::CommentPostFailed(e.to_string()))?;

    Ok(())
  }
}
