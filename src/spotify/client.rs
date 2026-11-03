use super::{RepeatState, model::*};
use crate::auth::Auth;
use anyhow::{Context, Result, bail};
use reqwest::{Method, StatusCode};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::{
  sync::Arc,
  time::{Duration, Instant},
};
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct Spotify {
  http: Option<reqwest::Client>,
  auth: Option<Arc<Mutex<Auth>>>,
  demo: Option<Arc<std::sync::Mutex<super::demo::Demo>>>,
  cooldown: Arc<std::sync::Mutex<Option<Instant>>>,
  base_url: String,
}

macro_rules! library_methods {
  ($contains:ident, $add:ident, $delete:ident, $kind:literal) => {
    pub async fn $contains(&self, ids: &[String]) -> Result<Vec<bool>> {
      self.library_contains($kind, ids).await
    }
    pub async fn $add(&self, ids: &[String]) -> Result<()> {
      self.library_write(Method::PUT, $kind, ids).await
    }
    pub async fn $delete(&self, ids: &[String]) -> Result<()> {
      self.library_write(Method::DELETE, $kind, ids).await
    }
  };
}

impl Spotify {
  pub fn new(auth: Auth) -> Result<Self> {
    Ok(Self {
      http: Some(auth.client()),
      auth: Some(Arc::new(Mutex::new(auth))),
      demo: None,
      cooldown: Arc::new(std::sync::Mutex::new(None)),
      base_url: "https://api.spotify.com/v1".into(),
    })
  }

  /// An entirely in-memory session: no HTTP client, OAuth or cache access.
  pub fn development() -> Self {
    Self {
      http: None,
      auth: None,
      demo: Some(Arc::new(std::sync::Mutex::new(super::demo::Demo::new()))),
      cooldown: Arc::new(std::sync::Mutex::new(None)),
      base_url: String::new(),
    }
  }

  pub fn is_development(&self) -> bool {
    self.demo.is_some()
  }

  #[cfg(test)]
  pub(crate) fn for_test(base_url: String, cache_path: std::path::PathBuf) -> Self {
    let mut client = Self::new(Auth::for_test(String::new(), cache_path)).unwrap();
    client.base_url = base_url;
    client.http = Some(
      crate::http::builder()
        .https_only(false)
        .no_proxy()
        .build()
        .unwrap(),
    );
    client
  }

  async fn request(
    &self,
    method: Method,
    path: &str,
    query: &[(&str, String)],
    body: Option<Value>,
  ) -> Result<Vec<u8>> {
    if let Some(demo) = &self.demo {
      let response = demo
        .lock()
        .map_err(|_| anyhow::anyhow!("Development session failed"))?
        .request(&method, path, query, body.as_ref())?;
      return Ok(serde_json::to_vec(&response)?);
    }
    let auth = self
      .auth
      .as_ref()
      .context("Missing Spotify authentication")?;
    let http = self.http.as_ref().context("Missing Spotify transport")?;
    // All requests share the same backoff, including concurrent searches and polling.
    if let Some(until) = *self.cooldown.lock().unwrap()
      && let Some(remaining) = until.checked_duration_since(Instant::now())
    {
      bail!(
        "Spotify requests are paused; retry in {} seconds",
        remaining.as_secs() + 1
      );
    }
    let mut token = auth.lock().await.access_token(false).await?;
    for attempt in 0..2 {
      let mut request = http
        .request(method.clone(), format!("{}{path}", self.base_url))
        .bearer_auth(&token)
        .query(query);
      if let Some(body) = &body {
        request = request.json(body);
      }
      let response = request.send().await?;
      if response.status() == StatusCode::UNAUTHORIZED && attempt == 0 {
        let mut auth = auth.lock().await;
        let current = auth.access_token(false).await?;
        token = if current == token {
          auth.access_token(true).await?
        } else {
          current
        };
        continue;
      }
      return self.decode(response).await;
    }
    unreachable!()
  }

  async fn decode(&self, response: reqwest::Response) -> Result<Vec<u8>> {
    let status = response.status();
    let retry_after = response
      .headers()
      .get(reqwest::header::RETRY_AFTER)
      .and_then(|v| v.to_str().ok())
      .and_then(|v| v.parse::<u64>().ok())
      .unwrap_or(60);
    let body_limit = if status.is_success() {
      crate::http::API_BODY_LIMIT
    } else {
      crate::http::TOKEN_BODY_LIMIT
    };
    let bytes = crate::http::body(response, body_limit).await;
    if status == StatusCode::TOO_MANY_REQUESTS {
      // A malformed error body must not disable the shared backoff.
      let bytes = bytes.unwrap_or_default();
      let value: Value = serde_json::from_slice(&bytes).unwrap_or_default();
      let quota = value.pointer("/error/reason").and_then(Value::as_str) == Some("QUOTA_EXCEEDED");
      // Do not replay writes or block the event loop with long sleeps.
      let wait = if quota {
        retry_after.max(3600)
      } else {
        retry_after.max(1)
      };
      let until = Instant::now()
        .checked_add(Duration::from_secs(wait))
        .unwrap_or_else(|| Instant::now() + Duration::from_secs(86400));
      let mut cooldown = self.cooldown.lock().unwrap();
      *cooldown = Some(cooldown.map_or(until, |old| old.max(until)));
      if quota {
        bail!(
          "Spotify's Development Mode quota is exhausted; requests paused for {wait} seconds. Apps on the same developer account share this quota"
        );
      }
      bail!("Spotify rate limit reached; requests paused for {wait} seconds");
    }
    let bytes = bytes?;
    if !status.is_success() {
      let value: Value = serde_json::from_slice(&bytes).unwrap_or_default();
      let message = value
        .pointer("/error/message")
        .and_then(Value::as_str)
        .unwrap_or("request failed");
      let hint = match status {
        StatusCode::FORBIDDEN => {
          " Check Premium, app access and allowed users; development apps cannot read every playlist."
        }
        StatusCode::NOT_FOUND => " Check the item or activate a Spotify playback device.",
        StatusCode::UNAUTHORIZED => " Sign in again by removing the token cache.",
        _ => "",
      };
      let message = crate::spotify::model::terminal_text(message);
      bail!("Spotify {status}: {message}.{hint}");
    }
    if bytes.is_empty() {
      Ok(b"null".to_vec())
    } else {
      Ok(bytes)
    }
  }

  async fn get<T: DeserializeOwned>(&self, path: &str, query: &[(&str, String)]) -> Result<T> {
    serde_json::from_slice(&self.request(Method::GET, path, query, None).await?)
      .context("Spotify returned an unexpected response")
  }
  async fn write(
    &self,
    method: Method,
    path: &str,
    query: &[(&str, String)],
    body: Option<Value>,
  ) -> Result<()> {
    self.request(method, path, query, body).await?;
    Ok(())
  }
  pub async fn current_user(&self) -> Result<PrivateUser> {
    self.get("/me", &[]).await
  }
  pub async fn device(&self) -> Result<DevicePayload> {
    self.get("/me/player/devices", &[]).await
  }
  pub async fn current_playback(&self) -> Result<Option<CurrentlyPlaybackContext>> {
    self
      .get(
        "/me/player",
        &[("additional_types", "track,episode".into())],
      )
      .await
  }
  pub async fn current_user_playlists(
    &self,
    limit: u32,
    offset: Option<u32>,
  ) -> Result<Page<SimplifiedPlaylist>> {
    self.get("/me/playlists", &pagination(limit, offset)).await
  }
  pub async fn playlist_items(
    &self,
    id: &str,
    limit: u32,
    offset: u32,
  ) -> Result<Page<PlaylistTrack>> {
    self
      .get::<PositionedPage<PlaylistTrack>>(
        &format!("/playlists/{}/items", catalog_id(id)?),
        &pagination(limit, Some(offset)),
      )
      .await
      .map(Into::into)
  }
  pub async fn playlist(&self, id: &str) -> Result<FullPlaylist> {
    self
      .get(&format!("/playlists/{}", catalog_id(id)?), &[])
      .await
  }
  pub async fn search(&self, term: &str, limit: u32) -> Result<SearchResults> {
    self
      .get(
        "/search",
        &[
          ("q", term.into()),
          ("type", "track,artist,album,playlist,show".into()),
          ("limit", limit.clamp(1, 10).to_string()),
        ],
      )
      .await
  }
  pub async fn current_user_saved_tracks(
    &self,
    limit: u32,
    offset: Option<u32>,
  ) -> Result<Page<SavedTrack>> {
    self.get("/me/tracks", &pagination(limit, offset)).await
  }
  pub async fn current_user_saved_albums(
    &self,
    limit: u32,
    offset: Option<u32>,
  ) -> Result<Page<SavedAlbum>> {
    self.get("/me/albums", &pagination(limit, offset)).await
  }
  pub async fn get_saved_show(&self, limit: u32, offset: Option<u32>) -> Result<Page<Show>> {
    self.get("/me/shows", &pagination(limit, offset)).await
  }
  pub async fn current_user_followed_artists(
    &self,
    limit: u32,
    after: Option<String>,
  ) -> Result<CursorPageFullArtists> {
    let mut query = vec![
      ("type", "artist".into()),
      ("limit", limit.clamp(1, 50).to_string()),
    ];
    if let Some(after) = &after {
      query.push(("after", after.clone()));
    }
    let mut result: CursorPageFullArtists = self.get("/me/following", &query).await?;
    result.artists.request_after = after;
    Ok(result)
  }
  pub async fn current_user_recently_played(
    &self,
    limit: u32,
  ) -> Result<CursorBasedPage<PlayHistory>> {
    self
      .get(
        "/me/player/recently-played",
        &[("limit", limit.clamp(1, 50).to_string())],
      )
      .await
  }
  pub async fn artist(&self, id: &str) -> Result<FullArtist> {
    self
      .get(&format!("/artists/{}", catalog_id(id)?), &[])
      .await
  }
  pub async fn artist_albums(
    &self,
    id: &str,
    limit: u32,
    offset: u32,
  ) -> Result<Page<SimplifiedAlbum>> {
    self
      .get(
        &format!("/artists/{}/albums", catalog_id(id)?),
        &pagination(limit, Some(offset)),
      )
      .await
  }
  pub async fn album(&self, id: &str) -> Result<FullAlbum> {
    self.get(&format!("/albums/{}", catalog_id(id)?), &[]).await
  }
  pub async fn track(&self, id: &str) -> Result<FullTrack> {
    self.get(&format!("/tracks/{}", catalog_id(id)?), &[]).await
  }
  pub async fn album_tracks(
    &self,
    id: &str,
    limit: u32,
    offset: u32,
  ) -> Result<Page<SimplifiedTrack>> {
    self
      .get::<PositionedPage<SimplifiedTrack>>(
        &format!("/albums/{}/tracks", catalog_id(id)?),
        &pagination(limit, Some(offset)),
      )
      .await
      .map(Into::into)
  }
  pub async fn show(&self, id: &str) -> Result<FullShow> {
    self.get(&format!("/shows/{}", catalog_id(id)?), &[]).await
  }
  pub async fn show_episodes(
    &self,
    id: &str,
    limit: u32,
    offset: impl Into<Option<u32>>,
  ) -> Result<Page<SimplifiedEpisode>> {
    self
      .get::<PositionedPage<SimplifiedEpisode>>(
        &format!("/shows/{}/episodes", catalog_id(id)?),
        &pagination(limit, offset.into()),
      )
      .await
      .map(Into::into)
  }

  async fn library_contains(&self, kind: &str, ids: &[String]) -> Result<Vec<bool>> {
    let mut result = Vec::new();
    for ids in ids.chunks(40) {
      let uris = library_uris(kind, ids)?;
      let saved: Vec<bool> = self.get("/me/library/contains", &[("uris", uris)]).await?;
      if saved.len() != ids.len() {
        bail!("Spotify returned an incomplete saved-items response");
      }
      result.extend(saved);
    }
    Ok(result)
  }
  async fn library_write(&self, method: Method, kind: &str, ids: &[String]) -> Result<()> {
    for ids in ids.chunks(40) {
      self
        .write(
          method.clone(),
          "/me/library",
          &[("uris", library_uris(kind, ids)?)],
          None,
        )
        .await?;
    }
    Ok(())
  }
  library_methods!(
    current_user_saved_tracks_contains,
    current_user_saved_tracks_add,
    current_user_saved_tracks_delete,
    "track"
  );
  library_methods!(
    current_user_saved_albums_contains,
    current_user_saved_albums_add,
    current_user_saved_albums_delete,
    "album"
  );
  library_methods!(
    user_artist_check_follow,
    user_follow_artists,
    user_unfollow_artists,
    "artist"
  );
  pub async fn check_users_saved_shows(&self, ids: Vec<String>) -> Result<Vec<bool>> {
    self.library_contains("show", &ids).await
  }
  pub async fn save_shows(&self, ids: Vec<String>) -> Result<()> {
    self.library_write(Method::PUT, "show", &ids).await
  }
  pub async fn remove_users_saved_shows(&self, ids: Vec<String>) -> Result<()> {
    self.library_write(Method::DELETE, "show", &ids).await
  }
  pub async fn follow_playlist(&self, id: &str) -> Result<()> {
    self
      .library_write(Method::PUT, "playlist", &[id.into()])
      .await
  }
  pub async fn unfollow_playlist(&self, id: &str) -> Result<()> {
    self
      .library_write(Method::DELETE, "playlist", &[id.into()])
      .await
  }

  pub async fn queue(&self) -> Result<Queue> {
    Ok(
      self
        .get::<Option<Queue>>("/me/player/queue", &[])
        .await?
        .unwrap_or_default(),
    )
  }
  pub async fn saved_episodes(&self, limit: u32, offset: u32) -> Result<Page<SavedEpisode>> {
    self
      .get("/me/episodes", &pagination(limit, Some(offset)))
      .await
  }
  pub async fn episode(&self, id: &str) -> Result<FullEpisode> {
    self
      .get(&format!("/episodes/{}", catalog_id(id)?), &[])
      .await
  }
  pub async fn saved_audiobooks(&self, limit: u32, offset: u32) -> Result<Page<Audiobook>> {
    self
      .get("/me/audiobooks", &pagination(limit, Some(offset)))
      .await
  }
  pub async fn audiobook(&self, id: &str) -> Result<Audiobook> {
    self
      .get(&format!("/audiobooks/{}", catalog_id(id)?), &[])
      .await
  }
  pub async fn audiobook_chapters(
    &self,
    id: &str,
    limit: u32,
    offset: u32,
  ) -> Result<Page<Chapter>> {
    self
      .get(
        &format!("/audiobooks/{}/chapters", catalog_id(id)?),
        &pagination(limit, Some(offset)),
      )
      .await
  }
  pub async fn search_audiobooks(
    &self,
    query: &str,
    limit: u32,
    offset: u32,
  ) -> Result<Page<Audiobook>> {
    let result: AudiobookSearch = self
      .get(
        "/search",
        &[
          ("q", query.into()),
          ("type", "audiobook".into()),
          ("limit", limit.clamp(1, 10).to_string()),
          ("offset", offset.to_string()),
        ],
      )
      .await?;
    Ok(result.audiobooks)
  }

  pub async fn saved_items(&self, uris: &[String]) -> Result<Vec<bool>> {
    let mut result = Vec::new();
    for batch in uris.chunks(40) {
      for uri in batch {
        validate_library_uri(uri)?;
      }
      let saved: Vec<bool> = self
        .get("/me/library/contains", &[("uris", batch.join(","))])
        .await?;
      if saved.len() != batch.len() {
        bail!("Spotify returned an incomplete saved-items response");
      }
      result.extend(saved);
    }
    Ok(result)
  }
  pub async fn set_saved_item(&self, uri: &str, saved: bool) -> Result<()> {
    validate_library_uri(uri)?;
    self
      .write(
        if saved { Method::PUT } else { Method::DELETE },
        "/me/library",
        &[("uris", uri.into())],
        None,
      )
      .await
  }

  pub async fn play_item(&self, uri: &str, position_ms: u32, device: Option<String>) -> Result<()> {
    let mut parts = uri.split(':');
    if parts.next() != Some("spotify")
      || !matches!(parts.next(), Some("track" | "episode"))
      || parts.next().is_none()
      || parts.next().is_some()
    {
      bail!("Playback through the Web API requires a track or episode URI");
    }
    catalog_id(uri)?;
    self
      .write(
        Method::PUT,
        "/me/player/play",
        &device_query(device),
        Some(json!({"uris": [uri], "position_ms": position_ms})),
      )
      .await
  }

  pub async fn start_playback(
    &self,
    device: Option<String>,
    context: Option<String>,
    uris: Option<Vec<String>>,
    offset: Option<Value>,
  ) -> Result<()> {
    let mut body = json!({});
    if let Some(context) = context {
      body["context_uri"] = json!(context);
    }
    if let Some(uris) = uris {
      body["uris"] = json!(uris);
    }
    if let Some(offset) = offset {
      body["offset"] = offset;
    }
    self
      .write(
        Method::PUT,
        "/me/player/play",
        &device_query(device),
        Some(body),
      )
      .await
  }
  pub async fn pause_playback(&self, device: Option<String>) -> Result<()> {
    self
      .write(Method::PUT, "/me/player/pause", &device_query(device), None)
      .await
  }
  pub async fn next_track(&self, device: Option<String>) -> Result<()> {
    self
      .write(Method::POST, "/me/player/next", &device_query(device), None)
      .await
  }
  pub async fn previous_track(&self, device: Option<String>) -> Result<()> {
    self
      .write(
        Method::POST,
        "/me/player/previous",
        &device_query(device),
        None,
      )
      .await
  }
  pub async fn seek_track(&self, position: u32, device: Option<String>) -> Result<()> {
    let mut query = device_query(device);
    query.push(("position_ms", position.to_string()));
    self
      .write(Method::PUT, "/me/player/seek", &query, None)
      .await
  }
  pub async fn shuffle(&self, state: bool, device: Option<String>) -> Result<()> {
    let mut query = device_query(device);
    query.push(("state", state.to_string()));
    self
      .write(Method::PUT, "/me/player/shuffle", &query, None)
      .await
  }
  pub async fn repeat(&self, state: RepeatState, device: Option<String>) -> Result<()> {
    let mut query = device_query(device);
    query.push((
      "state",
      serde_json::to_value(state)?.as_str().unwrap().into(),
    ));
    self
      .write(Method::PUT, "/me/player/repeat", &query, None)
      .await
  }
  pub async fn volume(&self, volume: u8, device: Option<String>) -> Result<()> {
    let mut query = device_query(device);
    query.push(("volume_percent", volume.min(100).to_string()));
    self
      .write(Method::PUT, "/me/player/volume", &query, None)
      .await
  }
  pub async fn transfer_playback(&self, device: &str, play: bool) -> Result<()> {
    self
      .write(
        Method::PUT,
        "/me/player",
        &[],
        Some(json!({"device_ids": [device], "play": play})),
      )
      .await
  }
  pub async fn add_item_to_queue(&self, uri: &str, device: Option<String>) -> Result<()> {
    let mut query = device_query(device);
    query.push(("uri", uri.into()));
    self
      .write(Method::POST, "/me/player/queue", &query, None)
      .await
  }
}

fn validate_library_uri(uri: &str) -> Result<()> {
  let mut parts = uri.split(':');
  if parts.next() != Some("spotify")
    || !matches!(
      parts.next(),
      Some("track" | "episode" | "album" | "artist" | "playlist" | "show" | "audiobook")
    )
    || parts.next().is_none()
    || parts.next().is_some()
  {
    bail!("Unsupported Spotify library URI");
  }
  catalog_id(uri)?;
  Ok(())
}

fn pagination(limit: u32, offset: Option<u32>) -> Vec<(&'static str, String)> {
  vec![
    ("limit", limit.clamp(1, 50).to_string()),
    ("offset", offset.unwrap_or(0).to_string()),
  ]
}
fn device_query(device: Option<String>) -> Vec<(&'static str, String)> {
  device
    .filter(|s| !s.is_empty())
    .map(|s| vec![("device_id", s)])
    .unwrap_or_default()
}
fn catalog_id(input: &str) -> Result<&str> {
  let id = input.rsplit(':').next().unwrap_or_default();
  if id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric()) {
    bail!("Invalid Spotify item ID");
  }
  Ok(id)
}
fn library_uris(kind: &str, ids: &[String]) -> Result<String> {
  ids
    .iter()
    .map(|id| Ok(format!("spotify:{kind}:{}", catalog_id(id)?)))
    .collect::<Result<Vec<_>>>()
    .map(|ids| ids.join(","))
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::spotify::test_support::server;

  fn client(base_url: String, token_url: String, dir: &tempfile::TempDir) -> Spotify {
    let mut client =
      Spotify::new(Auth::for_test(token_url, dir.path().join("token.json"))).unwrap();
    client.base_url = base_url;
    client.http = Some(
      crate::http::builder()
        .https_only(false)
        .no_proxy()
        .build()
        .unwrap(),
    );
    client
  }

  #[tokio::test]
  async fn media_saves_preserve_types_and_resume_uses_position_ms() {
    let (base, task) = server(vec![
      (200, "", "[true,false]".into()),
      (204, "", "".into()),
      (204, "", "".into()),
      (204, "", "".into()),
    ])
    .await;
    let dir = tempfile::tempdir().unwrap();
    let spotify = client(base, String::new(), &dir);
    assert_eq!(
      spotify
        .saved_items(&[
          "spotify:episode:episode".into(),
          "spotify:audiobook:book".into()
        ])
        .await
        .unwrap(),
      vec![true, false]
    );
    spotify
      .set_saved_item("spotify:episode:episode", true)
      .await
      .unwrap();
    spotify
      .set_saved_item("spotify:audiobook:book", false)
      .await
      .unwrap();
    spotify
      .play_item("spotify:episode:episode", 45000, Some("device".into()))
      .await
      .unwrap();
    let requests = task.await.unwrap();
    assert!(requests[1].starts_with("PUT /me/library?uris=spotify%3Aepisode%3Aepisode "));
    assert!(requests[2].starts_with("DELETE /me/library?uris=spotify%3Aaudiobook%3Abook "));
    assert!(requests[3].starts_with("PUT /me/player/play?device_id=device "));
    let body: Value = serde_json::from_str(requests[3].split("\r\n\r\n").nth(1).unwrap()).unwrap();
    assert_eq!(body["uris"], json!(["spotify:episode:episode"]));
    assert_eq!(body["position_ms"], 45000);
    assert!(body.get("offset").is_none());
  }

  #[tokio::test]
  async fn unsupported_chapter_playback_and_library_writes_never_send_requests() {
    let dir = tempfile::tempdir().unwrap();
    let spotify = client("http://127.0.0.1:1".into(), String::new(), &dir);
    assert!(
      spotify
        .play_item("spotify:chapter:chapter", 0, None)
        .await
        .unwrap_err()
        .to_string()
        .contains("track or episode")
    );
    assert!(
      spotify
        .set_saved_item("spotify:chapter:chapter", true)
        .await
        .unwrap_err()
        .to_string()
        .contains("Unsupported")
    );
    assert!(
      spotify
        .set_saved_item("spotify:episode:../me", true)
        .await
        .unwrap_err()
        .to_string()
        .contains("Invalid")
    );
  }

  #[tokio::test]
  async fn uses_current_playlist_and_library_endpoints() {
    let (base, task) = server(vec![
      (
        200,
        "",
        r#"{"items":[{"item":{"type":"track","id":"song","name":"Song"}}],"total":1}"#.into(),
      ),
      (200, "", "[true]".into()),
      (204, "", "".into()),
      (204, "", "".into()),
      (204, "", "".into()),
      (204, "", "".into()),
    ])
    .await;
    let dir = tempfile::tempdir().unwrap();
    let spotify = client(base, String::new(), &dir);
    let page = spotify.playlist_items("playlist", 20, 5).await.unwrap();
    assert_eq!(page.items[0].track.as_ref().unwrap().name, "Song");
    assert_eq!(
      spotify
        .current_user_saved_tracks_contains(&["song".into()])
        .await
        .unwrap(),
      vec![true]
    );
    spotify
      .current_user_saved_tracks_add(&["song".into()])
      .await
      .unwrap();
    spotify
      .current_user_saved_albums_delete(&["album".into()])
      .await
      .unwrap();
    spotify
      .user_follow_artists(&["artist".into()])
      .await
      .unwrap();
    spotify.follow_playlist("playlist").await.unwrap();
    let requests = task.await.unwrap();
    assert!(requests[0].starts_with("GET /playlists/playlist/items?limit=20&offset=5 "));
    assert!(requests[1].starts_with("GET /me/library/contains?uris=spotify%3Atrack%3Asong "));
    assert!(requests[2].starts_with("PUT /me/library?uris=spotify%3Atrack%3Asong "));
    assert!(requests[3].starts_with("DELETE /me/library?uris=spotify%3Aalbum%3Aalbum "));
    assert!(requests[4].starts_with("PUT /me/library?uris=spotify%3Aartist%3Aartist "));
    assert!(requests[5].starts_with("PUT /me/library?uris=spotify%3Aplaylist%3Aplaylist "));
    assert!(requests.iter().all(|r| {
      r.to_lowercase()
        .contains("authorization: bearer test-token")
    }));
  }

  #[tokio::test]
  async fn combines_search_types_and_caps_the_limit() {
    let (base, task) = server(vec![(200, "", r#"{"tracks":{"items":[{"id":"song","name":"Song"}]},"playlists":{"items":[null,{"id":"p","name":null,"items":{"total":2}}]}}"#.into())]).await;
    let dir = tempfile::tempdir().unwrap();
    let spotify = client(base, String::new(), &dir);
    let result = spotify.search("a & b", 50).await.unwrap();
    assert_eq!(result.tracks.items.len(), 1);
    assert_eq!(result.playlists.items.len(), 1);
    assert_eq!(result.playlists.items[0].tracks["total"], 2);
    let requests = task.await.unwrap();
    let target = requests[0].split_whitespace().nth(1).unwrap();
    let url = url::Url::parse(&format!("http://localhost{target}")).unwrap();
    let query: std::collections::HashMap<_, _> = url.query_pairs().collect();
    assert_eq!(query["q"], "a & b");
    assert_eq!(query["limit"], "10");
    assert_eq!(query["type"], "track,artist,album,playlist,show");
  }

  #[tokio::test]
  async fn handles_no_playback_and_success_without_a_body() {
    let (base, task) = server(vec![(204, "", "".into()), (204, "", "".into())]).await;
    let dir = tempfile::tempdir().unwrap();
    let spotify = client(base, String::new(), &dir);
    assert!(spotify.current_playback().await.unwrap().is_none());
    spotify.pause_playback(None).await.unwrap();
    assert_eq!(task.await.unwrap().len(), 2);
  }

  #[tokio::test]
  async fn rate_limit_pauses_all_requests_without_replaying_a_write() {
    let (base, task) = server(vec![(
      429,
      "Retry-After: 120\r\n",
      r#"{"error":{"message":"slow down"}}"#.into(),
    )])
    .await;
    let dir = tempfile::tempdir().unwrap();
    let spotify = client(base, String::new(), &dir);
    let error = spotify.pause_playback(None).await.unwrap_err().to_string();
    assert!(error.contains("120 seconds"));
    assert!(
      spotify
        .clone()
        .current_user()
        .await
        .unwrap_err()
        .to_string()
        .contains("paused")
    );
    assert_eq!(task.await.unwrap().len(), 1);
  }

  #[tokio::test]
  async fn distinguishes_exhausted_development_quota() {
    let (base, task) = server(vec![(
      429,
      "",
      r#"{"error":{"reason":"QUOTA_EXCEEDED"}}"#.into(),
    )])
    .await;
    let dir = tempfile::tempdir().unwrap();
    let spotify = client(base, String::new(), &dir);
    assert!(
      spotify
        .current_user()
        .await
        .unwrap_err()
        .to_string()
        .contains("quota is exhausted")
    );
    assert!(spotify.cooldown.lock().unwrap().unwrap() > Instant::now() + Duration::from_secs(3500));
    task.await.unwrap();
  }

  #[tokio::test]
  async fn refreshes_a_rejected_token_once_then_retries() {
    let (base, task) = server(vec![
      (401, "", r#"{"error":{"message":"expired"}}"#.into()),
      (200, "", r#"{"id":"listener"}"#.into()),
    ])
    .await;
    let (token_url, tokens) = server(vec![(
      200,
      "",
      r#"{"access_token":"renewed","expires_in":3600}"#.into(),
    )])
    .await;
    let dir = tempfile::tempdir().unwrap();
    let spotify = client(base, token_url, &dir);
    assert_eq!(spotify.current_user().await.unwrap().id, "listener");
    let requests = task.await.unwrap();
    assert!(
      requests[1]
        .to_lowercase()
        .contains("authorization: bearer renewed")
    );
    let token_requests = tokens.await.unwrap();
    assert!(token_requests[0].contains("grant_type=refresh_token"));
    assert!(token_requests[0].contains("refresh_token=refresh-original"));
    assert!(!token_requests[0].contains("client_secret"));
    let stored: Value =
      serde_json::from_slice(&std::fs::read(dir.path().join("token.json")).unwrap()).unwrap();
    assert_eq!(stored["refresh_token"], "refresh-original");
  }

  #[tokio::test]
  async fn rejects_incomplete_saved_status_and_skips_empty_batches() {
    let (base, task) = server(vec![(200, "", "[]".into())]).await;
    let dir = tempfile::tempdir().unwrap();
    let spotify = client(base, String::new(), &dir);
    assert!(
      spotify
        .current_user_saved_tracks_contains(&[])
        .await
        .unwrap()
        .is_empty()
    );
    assert!(
      spotify
        .current_user_saved_tracks_contains(&["song".into()])
        .await
        .unwrap_err()
        .to_string()
        .contains("incomplete")
    );
    assert_eq!(task.await.unwrap().len(), 1);
  }
}
