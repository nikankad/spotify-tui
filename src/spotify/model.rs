//! Response models for fields used by the terminal client.
//! Removed metadata is deliberately ignored. Both playlist response formats
//! are accepted so older Spotify apps continue to work.
use super::{RepeatState, Type};
use serde::{Deserialize, Deserializer};

/// Remote metadata must not emit terminal control sequences in CLI output.
pub fn terminal_text(text: &str) -> String {
  text
    .chars()
    .filter(|character| !character.is_control())
    .collect()
}
fn display_text<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
  let mut text = Option::<String>::deserialize(d)?.unwrap_or_default();
  text.retain(|character| !character.is_control());
  Ok(text)
}
fn optional_text<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
  Ok(Option::<String>::deserialize(d)?.map(|mut text| {
    text.retain(|character| !character.is_control());
    text
  }))
}

fn null_default<'de, D, T>(d: D) -> Result<T, D::Error>
where
  D: Deserializer<'de>,
  T: Deserialize<'de> + Default,
{
  Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

fn skip_nulls<'de, D, T>(d: D) -> Result<Vec<T>, D::Error>
where
  D: Deserializer<'de>,
  T: Deserialize<'de>,
{
  Ok(
    Option::<Vec<Option<T>>>::deserialize(d)?
      .unwrap_or_default()
      .into_iter()
      .flatten()
      .collect(),
  )
}

// Preserve unavailable entries in positional collections. Filtering them during
// decoding would shift playback offsets for all subsequent songs or episodes.
#[derive(Default, Deserialize)]
#[serde(default, bound(deserialize = "T: Deserialize<'de> + Default"))]
pub(super) struct PositionedPage<T> {
  items: Vec<Option<T>>,
  limit: u32,
  next: Option<String>,
  offset: u32,
  previous: Option<String>,
  total: u32,
}
impl<T> From<PositionedPage<T>> for Page<T>
where
  T: Default,
{
  fn from(page: PositionedPage<T>) -> Self {
    Self {
      items: page
        .items
        .into_iter()
        .map(Option::unwrap_or_default)
        .collect(),
      limit: page.limit,
      next: page.next,
      offset: page.offset,
      previous: page.previous,
      total: page.total,
    }
  }
}
fn positioned_page<'de, D, T>(d: D) -> Result<Page<T>, D::Error>
where
  D: Deserializer<'de>,
  T: Deserialize<'de> + Default,
{
  Ok(
    Option::<PositionedPage<T>>::deserialize(d)?
      .unwrap_or_default()
      .into(),
  )
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, bound(deserialize = "T: Deserialize<'de> + Default"))]
pub struct Page<T> {
  #[serde(deserialize_with = "skip_nulls")]
  pub items: Vec<T>,
  pub limit: u32,
  #[serde(deserialize_with = "optional_text")]
  pub next: Option<String>,
  pub offset: u32,
  #[serde(deserialize_with = "optional_text")]
  pub previous: Option<String>,
  pub total: u32,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, bound(deserialize = "T: Deserialize<'de> + Default"))]
pub struct CursorBasedPage<T> {
  #[serde(skip)]
  pub request_after: Option<String>,
  #[serde(deserialize_with = "skip_nulls")]
  pub items: Vec<T>,
  #[serde(deserialize_with = "optional_text")]
  pub next: Option<String>,
  pub cursors: Cursor,
  pub total: Option<u32>,
}
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Cursor {
  #[serde(deserialize_with = "optional_text")]
  pub after: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct SimplifiedArtist {
  #[serde(deserialize_with = "optional_text")]
  pub id: Option<String>,
  #[serde(deserialize_with = "display_text")]
  pub name: String,
}
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct FullArtist {
  #[serde(deserialize_with = "display_text")]
  pub id: String,
  #[serde(deserialize_with = "display_text")]
  pub name: String,
  #[serde(deserialize_with = "display_text")]
  pub uri: String,
}
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct CursorPageFullArtists {
  pub artists: CursorBasedPage<FullArtist>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct SimplifiedAlbum {
  #[serde(deserialize_with = "optional_text")]
  pub album_type: Option<String>,
  #[serde(deserialize_with = "optional_text")]
  pub id: Option<String>,
  #[serde(deserialize_with = "display_text")]
  pub name: String,
  pub artists: Vec<SimplifiedArtist>,
  #[serde(deserialize_with = "optional_text")]
  pub release_date: Option<String>,
  #[serde(deserialize_with = "optional_text")]
  pub uri: Option<String>,
}
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct FullAlbum {
  #[serde(deserialize_with = "display_text")]
  pub id: String,
  #[serde(deserialize_with = "display_text")]
  pub name: String,
  pub artists: Vec<SimplifiedArtist>,
  #[serde(deserialize_with = "display_text")]
  pub release_date: String,
  #[serde(deserialize_with = "positioned_page")]
  pub tracks: Page<SimplifiedTrack>,
  #[serde(deserialize_with = "display_text")]
  pub uri: String,
}
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct SavedAlbum {
  pub album: FullAlbum,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct FullTrack {
  #[serde(deserialize_with = "optional_text")]
  pub id: Option<String>,
  #[serde(deserialize_with = "display_text")]
  pub name: String,
  pub artists: Vec<SimplifiedArtist>,
  pub album: SimplifiedAlbum,
  pub duration_ms: u32,
  pub track_number: u32,
  #[serde(deserialize_with = "display_text")]
  pub uri: String,
  pub is_local: bool,
  pub is_playable: Option<bool>,
}
pub type SimplifiedTrack = FullTrack;
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct SavedTrack {
  pub track: FullTrack,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct PublicUser {
  #[serde(deserialize_with = "display_text")]
  pub id: String,
  #[serde(deserialize_with = "optional_text")]
  pub display_name: Option<String>,
}
pub type PrivateUser = PublicUser;
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct SimplifiedPlaylist {
  #[serde(deserialize_with = "display_text")]
  pub id: String,
  #[serde(deserialize_with = "display_text")]
  pub name: String,
  pub owner: PublicUser,
  pub collaborative: bool,
  pub public: Option<bool>,
  #[serde(deserialize_with = "display_text")]
  pub uri: String,
  #[serde(alias = "items", deserialize_with = "null_default")]
  pub tracks: std::collections::HashMap<String, serde_json::Value>,
}
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct FullPlaylist {
  #[serde(deserialize_with = "display_text")]
  pub id: String,
  #[serde(deserialize_with = "display_text")]
  pub name: String,
  #[serde(deserialize_with = "display_text")]
  pub uri: String,
  pub owner: PublicUser,
  #[serde(alias = "items", deserialize_with = "positioned_page")]
  pub tracks: Page<PlaylistTrack>,
}
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct PlaylistTrack {
  #[serde(alias = "item", deserialize_with = "playlist_track")]
  pub track: Option<FullTrack>,
}
fn playlist_track<'de, D: Deserializer<'de>>(d: D) -> Result<Option<FullTrack>, D::Error> {
  let value = Option::<serde_json::Value>::deserialize(d)?;
  match value {
    Some(value) if value.get("type").and_then(|t| t.as_str()) == Some("track") => {
      serde_json::from_value(value)
        .map(Some)
        .map_err(serde::de::Error::custom)
    }
    _ => Ok(None), // Episodes and unavailable items don't belong in a track table.
  }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct SimplifiedShow {
  #[serde(deserialize_with = "display_text")]
  pub id: String,
  #[serde(deserialize_with = "display_text")]
  pub name: String,
  #[serde(deserialize_with = "display_text")]
  pub uri: String,
  #[serde(deserialize_with = "display_text")]
  pub publisher: String,
}
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct FullShow {
  #[serde(deserialize_with = "display_text")]
  pub id: String,
  #[serde(deserialize_with = "display_text")]
  pub name: String,
  #[serde(deserialize_with = "display_text")]
  pub uri: String,
  #[serde(deserialize_with = "display_text")]
  pub publisher: String,
  #[serde(deserialize_with = "positioned_page")]
  pub episodes: Page<SimplifiedEpisode>,
}
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Show {
  pub show: SimplifiedShow,
}
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct FullEpisode {
  pub resume_point: Option<ResumePoint>,
  #[serde(deserialize_with = "display_text")]
  pub id: String,
  #[serde(deserialize_with = "display_text")]
  pub name: String,
  #[serde(deserialize_with = "display_text")]
  pub uri: String,
  pub duration_ms: u32,
  #[serde(deserialize_with = "display_text")]
  pub release_date: String,
  pub show: SimplifiedShow,
}
pub type SimplifiedEpisode = FullEpisode;

// Tagging prevents an episode with omitted metadata being decoded as a track.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PlayingItem {
  Track(FullTrack),
  Episode(FullEpisode),
}
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Device {
  #[serde(deserialize_with = "display_text")]
  pub id: String,
  #[serde(deserialize_with = "display_text")]
  pub name: String,
  pub is_active: bool,
  pub is_restricted: bool,
  #[serde(deserialize_with = "null_default")]
  pub volume_percent: u32,
  pub supports_volume: Option<bool>,
}
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct DevicePayload {
  pub devices: Vec<Device>,
}
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Context {
  #[serde(deserialize_with = "display_text")]
  pub uri: String,
  #[serde(rename = "type")]
  pub _type: Type,
}
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct CurrentlyPlaybackContext {
  pub device: Device,
  pub repeat_state: RepeatState,
  pub shuffle_state: bool,
  pub context: Option<Context>,
  pub progress_ms: Option<u32>,
  pub is_playing: bool,
  pub item: Option<PlayingItem>,
}
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct PlayHistory {
  pub track: SimplifiedTrack,
  pub played_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct SearchResults {
  pub tracks: Page<FullTrack>,
  pub artists: Page<FullArtist>,
  pub albums: Page<SimplifiedAlbum>,
  pub playlists: Page<SimplifiedPlaylist>,
  pub shows: Page<SimplifiedShow>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct ResumePoint {
  pub fully_played: bool,
  pub resume_position_ms: u32,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Queue {
  pub currently_playing: Option<PlayingItem>,
  #[serde(deserialize_with = "skip_nulls")]
  pub queue: Vec<PlayingItem>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct SavedEpisode {
  pub episode: FullEpisode,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Contributor {
  #[serde(deserialize_with = "display_text")]
  pub name: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Audiobook {
  #[serde(deserialize_with = "display_text")]
  pub id: String,
  #[serde(deserialize_with = "display_text")]
  pub name: String,
  #[serde(deserialize_with = "display_text")]
  pub uri: String,
  pub authors: Vec<Contributor>,
  pub narrators: Vec<Contributor>,
  pub total_chapters: u32,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Chapter {
  #[serde(deserialize_with = "display_text")]
  pub id: String,
  #[serde(deserialize_with = "display_text")]
  pub name: String,
  #[serde(deserialize_with = "display_text")]
  pub uri: String,
  pub chapter_number: u32,
  pub duration_ms: u32,
  pub resume_point: Option<ResumePoint>,
}

#[derive(Deserialize)]
pub struct AudiobookSearch {
  pub audiobooks: Page<Audiobook>,
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;

  #[test]
  fn remote_metadata_cannot_emit_terminal_control_sequences() {
    let track: FullTrack = serde_json::from_value(json!({"name":"Song\u{1b}[31m\r\n","uri":"spotify:track:id\u{1b}","artists":[{"name":"Author\u{7}"}]})).unwrap();
    assert!(!track.name.chars().any(char::is_control));
    assert_eq!(track.uri, "spotify:track:id");
    assert_eq!(track.artists[0].name, "Author");
  }

  #[test]
  fn omitted_2026_metadata_does_not_break_catalogue_responses() {
    let album: FullAlbum = serde_json::from_value(
      json!({"id":"album", "name":"Album", "tracks":{"items":[{"type":"track","id":"song"}]}}),
    )
    .unwrap();
    assert_eq!(album.tracks.items[0].id.as_deref(), Some("song"));
    let artist: FullArtist =
      serde_json::from_value(json!({"id":"artist","name":"Artist"})).unwrap();
    assert_eq!(artist.name, "Artist");
    let show: SimplifiedShow =
      serde_json::from_value(json!({"name":"Show","publisher":null})).unwrap();
    assert!(show.publisher.is_empty());
    let device: Device =
      serde_json::from_value(json!({"id":null,"volume_percent":null,"supports_volume":false}))
        .unwrap();
    assert!(device.id.is_empty());
    assert_eq!(device.volume_percent, 0);
  }

  #[test]
  fn playlist_nulls_and_episodes_keep_their_original_positions() {
    let playlist: FullPlaylist = serde_json::from_value(json!({"items":{"offset":20,"items":[null,{"item":null},{"item":{"type":"episode","id":"episode"}},{"item":{"type":"track","id":"song"}}]}})).unwrap();
    assert_eq!(playlist.tracks.items.len(), 4);
    assert!(
      playlist.tracks.items[..3]
        .iter()
        .all(|item| item.track.is_none())
    );
    assert_eq!(
      playlist.tracks.items[3]
        .track
        .as_ref()
        .unwrap()
        .id
        .as_deref(),
      Some("song")
    );
  }

  #[test]
  fn accepts_both_playlist_formats_and_episode_playback() {
    for key in ["tracks", "items"] {
      let item = if key == "tracks" { "track" } else { "item" };
      let playlist: FullPlaylist =
        serde_json::from_value(json!({key:{"items":[{item:{"type":"track","id":"song"}}]}}))
          .unwrap();
      assert_eq!(
        playlist.tracks.items[0]
          .track
          .as_ref()
          .unwrap()
          .id
          .as_deref(),
        Some("song")
      );
    }
    let playback: CurrentlyPlaybackContext =
      serde_json::from_value(json!({"item":{"type":"episode","id":"episode","name":"Episode"}}))
        .unwrap();
    assert!(matches!(playback.item, Some(PlayingItem::Episode(_))));
  }
}
