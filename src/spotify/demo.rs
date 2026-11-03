//! Deterministic, session-only Spotify responses. No sockets or credential files.
use anyhow::{Result, anyhow, bail};
use reqwest::Method;
use serde_json::{Value, json};
use std::{
  collections::{BTreeSet, VecDeque},
  time::Instant,
};

const TRACKS: usize = 96;
const ALBUMS: usize = 24;
const ARTISTS: usize = 12;
const SHOWS: usize = 8;
const EPISODES: usize = 32;
const BOOKS: usize = 24;
const CHAPTERS: usize = 24;
const PLAYLISTS: usize = 6;
const QUEUE_LIMIT: usize = 200;
const ARTIST_NAMES: [&str; ARTISTS] = [
  "Northbound",
  "Mira Vale",
  "Paper Satellites",
  "Juniper",
  "The Night Garden",
  "Solstice",
  "Atlas Radio",
  "Echo Harbour",
  "Velvet Circuit",
  "Sora",
  "Low Tide",
  "Luna Park",
];
const ALBUM_NAMES: [&str; 8] = [
  "Blue Hour",
  "City Lights",
  "Quiet Geometry",
  "Open Water",
  "After the Rain",
  "Parallel Lines",
  "Soft Focus",
  "Long Way Home",
];
const SONG_NAMES: [&str; 12] = [
  "First Light",
  "Night Drive",
  "Daybreak",
  "Still Water",
  "Signal",
  "Slow Motion",
  "Moonrise",
  "Northern Sky",
  "Wandering",
  "Second Wind",
  "Paper Planes",
  "Home Again",
];
const SHOW_NAMES: [&str; SHOWS] = [
  "The Curious Mind",
  "Design After Hours",
  "Field Notes",
  "Small Worlds",
  "Sound Stories",
  "Tomorrow, Today",
  "Coffee and Code",
  "The Long Conversation",
];
const BOOK_NAMES: [&str; 6] = [
  "The Glass Observatory",
  "A Map of Quiet Places",
  "Letters from Tomorrow",
  "The Last Lighthouse",
  "Beyond the Horizon",
  "The Art of Paying Attention",
];

fn id(kind: char, index: usize) -> String {
  format!("{kind}{index:021}")
}
fn uri(kind: &str, index: usize) -> String {
  format!("spotify:{kind}:{}", id(prefix(kind), index))
}
fn prefix(kind: &str) -> char {
  match kind {
    "track" => 't',
    "album" => 'a',
    "artist" => 'r',
    "playlist" => 'p',
    "show" => 's',
    "episode" => 'e',
    "audiobook" => 'b',
    "chapter" => 'c',
    _ => '?',
  }
}
fn index(input: &str, kind: &str, count: usize) -> Result<usize> {
  let raw = input.rsplit(':').next().unwrap_or_default();
  let number = raw
    .strip_prefix(prefix(kind))
    .and_then(|value| value.parse::<usize>().ok());
  number
    .filter(|&n| n < count)
    .ok_or_else(|| anyhow!("Development mode: unknown sample {kind} ID"))
}
fn artist(n: usize) -> Value {
  json!({"id":id('r', n),"name":ARTIST_NAMES[n],"uri":uri("artist",n)})
}
fn album(n: usize) -> Value {
  json!({"id":id('a',n),"name":format!("{} {}",ALBUM_NAMES[n % 8], n / 8 + 1),"uri":uri("album",n),"album_type":"album","release_date":format!("{}-06-12",2026 - n % 5),"artists":[artist(n % ARTISTS)]})
}
fn track(n: usize) -> Value {
  json!({"type":"track","id":id('t',n),"name":format!("{} {}",SONG_NAMES[n % 12], n / 12 + 1),"uri":uri("track",n),"artists":[artist(n / 4 % ARTISTS)],"album":album(n / 4),"duration_ms":180000 + n % 7 * 17000,"track_number":n % 4 + 1,"is_local":false,"is_playable":true})
}
fn show(n: usize) -> Value {
  json!({"id":id('s',n),"name":SHOW_NAMES[n],"uri":uri("show",n),"publisher":"Demo Audio Network"})
}
fn episode(n: usize) -> Value {
  json!({"type":"episode","id":id('e',n),"name":format!("Episode {}: {}",n / SHOWS + 1, ["Starting Something New","A Different Perspective","Behind the Scenes","Looking Ahead"][n / SHOWS]),"uri":uri("episode",n),"show":show(n % SHOWS),"duration_ms":2400000,"release_date":"2026-10-28","resume_point":{"fully_played":n % 4 == 3,"resume_position_ms":if n % 4 == 3 {2400000} else {n % 3 * 300000}}})
}
fn book(n: usize) -> Value {
  json!({"id":id('b',n),"name":format!("{} — Volume {}",BOOK_NAMES[n % 6],n / 6 + 1),"uri":uri("audiobook",n),"authors":[{"name":"Alex Morgan"}],"narrators":[{"name":"Jamie Rivers"}],"total_chapters":CHAPTERS})
}
fn chapter(n: usize, book_index: usize) -> Value {
  let number = book_index * CHAPTERS + n;
  json!({"id":id('c',number),"name":format!("Chapter {}: {}",n + 1, SONG_NAMES[n % 12]),"uri":uri("chapter",number),"chapter_number":n + 1,"duration_ms":1200000,"resume_point":{"fully_played":n < 2,"resume_position_ms":if n < 2 {1200000} else {0}}})
}
fn user() -> Value {
  json!({"id":"demo","display_name":"Development Listener"})
}
fn playlist(n: usize) -> Value {
  json!({"id":id('p',n),"name":(["Late Night Focus","Weekend Wandering","Morning Coffee","Deep Work","Evening Unwind","On the Road"][n]),"uri":uri("playlist",n),"owner":user(),"public":n.is_multiple_of(2),"collaborative":false,"items":{"total":TRACKS}})
}
fn devices() -> Vec<Value> {
  vec![
    json!({"id":"demoDesktop","name":"Demo Desktop","is_active":true,"volume_percent":45,"supports_volume":true}),
    json!({"id":"demoSpeaker","name":"Demo Living Room","is_active":false,"volume_percent":65,"supports_volume":true}),
    json!({"id":"demoPhone","name":"Demo Phone (fixed volume)","is_active":false,"volume_percent":100,"supports_volume":false}),
  ]
}
fn page(items: Vec<Value>, query: &[(&str, String)]) -> Value {
  let limit = parameter(query, "limit")
    .parse::<usize>()
    .unwrap_or(20)
    .clamp(1, 50);
  let offset = parameter(query, "offset").parse::<usize>().unwrap_or(0);
  let total = items.len();
  let next =
    (offset.saturating_add(limit) < total).then(|| format!("demo:page:{}", offset + limit));
  json!({"items":items.into_iter().skip(offset).take(limit).collect::<Vec<_>>(),"total":total,"limit":limit,"offset":offset,"next":next,"previous":(offset > 0).then_some("demo:previous")})
}
fn parameter<'a>(query: &'a [(&str, String)], name: &str) -> &'a str {
  query
    .iter()
    .find(|(key, _)| *key == name)
    .map(|(_, value)| value.as_str())
    .unwrap_or_default()
}
fn matches(item: &Value, term: &str) -> bool {
  item["name"]
    .as_str()
    .unwrap_or_default()
    .to_lowercase()
    .contains(term)
    || item["artists"]
      .as_array()
      .is_some_and(|artists| artists.iter().any(|artist| matches(artist, term)))
}

pub(super) struct Demo {
  saved: BTreeSet<String>,
  player: Value,
  queue: VecDeque<Value>,
  position: usize,
  updated: Instant,
}
impl Demo {
  pub fn new() -> Self {
    let mut saved = BTreeSet::new();
    for (kind, count) in [
      ("track", TRACKS),
      ("album", ALBUMS),
      ("artist", ARTISTS),
      ("playlist", PLAYLISTS),
      ("show", SHOWS),
      ("episode", EPISODES),
      ("audiobook", BOOKS),
    ] {
      for n in 0..count {
        saved.insert(uri(kind, n));
      }
    }
    Self {
      saved,
      player: json!({"device":devices()[0],"repeat_state":"off","shuffle_state":false,"context":{"uri":uri("playlist",0),"type":"playlist"},"progress_ms":42000,"is_playing":true,"item":track(0)}),
      queue: VecDeque::from([track(1), track(2), episode(0), track(3)]),
      position: 0,
      updated: Instant::now(),
    }
  }
  fn saved_page(
    &self,
    kind: &str,
    count: usize,
    build: fn(usize) -> Value,
    wrapper: Option<&str>,
    query: &[(&str, String)],
  ) -> Value {
    let items = (0..count)
      .filter(|&n| self.saved.contains(&uri(kind, n)))
      .map(build)
      .map(|item| {
        if let Some(key) = wrapper {
          json!({key:item})
        } else {
          item
        }
      })
      .collect();
    page(items, query)
  }
  fn tick(&mut self) {
    if self.player["is_playing"] == true {
      let progress = self.player["progress_ms"]
        .as_u64()
        .unwrap_or(0)
        .saturating_add(self.updated.elapsed().as_millis().min(u64::MAX as u128) as u64);
      let duration = self.player["item"]["duration_ms"].as_u64().unwrap_or(1);
      if progress >= duration {
        if self.player["repeat_state"] == "track" {
          self.player["progress_ms"] = json!(0);
        } else {
          self.advance(true);
        }
      } else {
        self.player["progress_ms"] = json!(progress);
      }
    }
    self.updated = Instant::now();
  }
  fn playable(input: &str) -> Result<Value> {
    if input.starts_with("spotify:episode:") {
      Ok(episode(index(input, "episode", EPISODES)?))
    } else {
      Ok(track(index(input, "track", TRACKS)?))
    }
  }
  fn advance(&mut self, forward: bool) {
    self.position = if forward {
      (self.position + 1) % TRACKS
    } else {
      (self.position + TRACKS - 1) % TRACKS
    };
    self.player["item"] = if forward {
      self
        .queue
        .pop_front()
        .unwrap_or_else(|| track(self.position))
    } else {
      track(self.position)
    };
    self.player["progress_ms"] = json!(0);
  }
  pub fn request(
    &mut self,
    method: &Method,
    path: &str,
    query: &[(&str, String)],
    body: Option<&Value>,
  ) -> Result<Value> {
    self.tick();
    if *method != Method::GET {
      return self.write(method, path, query, body);
    }
    let parts: Vec<_> = path.trim_matches('/').split('/').collect();
    Ok(match parts.as_slice() {
      ["me"] => user(),
      ["me", "player"] => self.player.clone(),
      ["me", "player", "devices"] => {
        let mut values = devices();
        for device in &mut values {
          device["is_active"] = json!(device["id"] == self.player["device"]["id"]);
          if device["is_active"] == true {
            *device = self.player["device"].clone();
          }
        }
        json!({"devices":values})
      }
      ["me", "player", "queue"] => {
        json!({"currently_playing":self.player["item"],"queue":self.queue})
      }
      ["me", "playlists"] => self.saved_page("playlist", PLAYLISTS, playlist, None, query),
      ["me", "tracks"] => self.saved_page("track", TRACKS, track, Some("track"), query),
      ["me", "albums"] => self.saved_page(
        "album",
        ALBUMS,
        |n| {
          let mut value = album(n);
          value["tracks"] = page((n * 4..n * 4 + 4).map(track).collect(), &[]);
          value
        },
        Some("album"),
        query,
      ),
      ["me", "shows"] => self.saved_page("show", SHOWS, show, Some("show"), query),
      ["me", "episodes"] => self.saved_page("episode", EPISODES, episode, Some("episode"), query),
      ["me", "audiobooks"] => self.saved_page("audiobook", BOOKS, book, None, query),
      ["me", "following"] => {
        let values: Vec<_> = (0..ARTISTS)
          .filter(|&n| self.saved.contains(&uri("artist", n)))
          .map(artist)
          .collect();
        let after = parameter(query, "after");
        let offset = if after.is_empty() {
          0
        } else {
          values
            .iter()
            .position(|value| value["id"] == after)
            .map_or(values.len(), |n| n + 1)
        };
        let mut query = query.to_vec();
        query.push(("offset", offset.to_string()));
        let mut result = page(values, &query);
        result["cursors"] = json!({"after":result["items"].as_array().and_then(|items|items.last()).map(|last|last["id"].clone())});
        json!({"artists":result})
      }
      ["me", "player", "recently-played"] => page(
        (0..TRACKS)
          .map(|n| json!({"track":track(n),"played_at":"2026-11-03T12:00:00Z"}))
          .collect(),
        query,
      ),
      ["me", "library", "contains"] => json!(
        parameter(query, "uris")
          .split(',')
          .map(|uri| self.saved.contains(uri))
          .collect::<Vec<_>>()
      ),
      ["search"] => {
        let term = parameter(query, "q").to_lowercase();
        if parameter(query, "type") == "audiobook" {
          json!({"audiobooks":page((0..BOOKS).map(book).filter(|item|matches(item,&term)).collect(),query)})
        } else {
          json!({"tracks":page((0..TRACKS).map(track).filter(|item|matches(item,&term)).collect(),query),"artists":page((0..ARTISTS).map(artist).filter(|item|matches(item,&term)).collect(),query),"albums":page((0..ALBUMS).map(album).filter(|item|matches(item,&term)).collect(),query),"playlists":page((0..PLAYLISTS).map(playlist).filter(|item|matches(item,&term)).collect(),query),"shows":page((0..SHOWS).map(show).filter(|item|matches(item,&term)).collect(),query)})
        }
      }
      ["tracks", id] => track(index(id, "track", TRACKS)?),
      ["artists", id] => artist(index(id, "artist", ARTISTS)?),
      ["artists", id, "albums"] => {
        let n = index(id, "artist", ARTISTS)?;
        page(
          (0..ALBUMS)
            .filter(|album| album % ARTISTS == n)
            .map(album)
            .collect(),
          query,
        )
      }
      ["albums", id] => {
        let n = index(id, "album", ALBUMS)?;
        let mut result = album(n);
        result["tracks"] = page((n * 4..n * 4 + 4).map(track).collect(), &[]);
        result
      }
      ["albums", id, "tracks"] => {
        let n = index(id, "album", ALBUMS)?;
        page((n * 4..n * 4 + 4).map(track).collect(), query)
      }
      ["playlists", id] => {
        let n = index(id, "playlist", PLAYLISTS)?;
        let mut result = playlist(n);
        result["items"] = self.playlist_items(n, &[]);
        result
      }
      ["playlists", id, "items"] => self.playlist_items(index(id, "playlist", PLAYLISTS)?, query),
      ["shows", id] => {
        let n = index(id, "show", SHOWS)?;
        let mut result = show(n);
        result["episodes"] = page(
          (0..EPISODES)
            .filter(|episode| episode % SHOWS == n)
            .map(episode)
            .collect(),
          &[],
        );
        result
      }
      ["shows", id, "episodes"] => {
        let n = index(id, "show", SHOWS)?;
        page(
          (0..EPISODES)
            .filter(|episode| episode % SHOWS == n)
            .map(episode)
            .collect(),
          query,
        )
      }
      ["episodes", id] => episode(index(id, "episode", EPISODES)?),
      ["audiobooks", id] => book(index(id, "audiobook", BOOKS)?),
      ["audiobooks", id, "chapters"] => {
        let book = index(id, "audiobook", BOOKS)?;
        page((0..CHAPTERS).map(|n| chapter(n, book)).collect(), query)
      }
      _ => bail!("Development mode does not implement {method} {path}"),
    })
  }
  fn playlist_items(&self, n: usize, query: &[(&str, String)]) -> Value {
    page(
      (0..TRACKS)
        .map(|offset| json!({"item":track((offset+n*8)%TRACKS)}))
        .collect(),
      query,
    )
  }
  fn write(
    &mut self,
    method: &Method,
    path: &str,
    query: &[(&str, String)],
    body: Option<&Value>,
  ) -> Result<Value> {
    let empty = json!({});
    let body = body.unwrap_or(&empty);
    match (method.as_str(), path) {
      ("PUT" | "DELETE", "/me/library") => {
        for input in parameter(query, "uris").split(',') {
          if *method == Method::PUT {
            self.saved.insert(input.into());
          } else {
            self.saved.remove(input);
          }
        }
      }
      ("PUT", "/me/player/play") => {
        let context = body["context_uri"].as_str();
        let offset = body["offset"]["position"].as_u64().unwrap_or(0) as usize;
        if let Some(input) = body["uris"]
          .as_array()
          .and_then(|uris| uris.get(offset))
          .and_then(Value::as_str)
        {
          self.player["item"] = Self::playable(input)?;
          self.player["context"] = Value::Null;
        } else if let Some(context) = context {
          let (kind, n) = if context.starts_with("spotify:album:") {
            ("album", index(context, "album", ALBUMS)?)
          } else {
            ("playlist", index(context, "playlist", PLAYLISTS)?)
          };
          let n = if kind == "album" {
            n * 4 + offset % 4
          } else {
            (n * 8 + offset) % TRACKS
          };
          self.player["item"] = track(n);
          self.position = n;
          self.player["context"] = json!({"uri":context,"type":kind});
        }
        self.player["progress_ms"] = body.get("position_ms").cloned().unwrap_or(json!(0));
        self.player["is_playing"] = json!(true);
      }
      ("PUT", "/me/player/pause") => self.player["is_playing"] = json!(false),
      ("POST", "/me/player/next") => self.advance(true),
      ("POST", "/me/player/previous") => self.advance(false),
      ("PUT", "/me/player/seek") => {
        let duration = self.player["item"]["duration_ms"].as_u64().unwrap_or(0);
        self.player["progress_ms"] = json!(
          parameter(query, "position_ms")
            .parse::<u64>()?
            .min(duration)
        );
      }
      ("PUT", "/me/player/shuffle") => {
        self.player["shuffle_state"] = json!(parameter(query, "state").parse::<bool>()?)
      }
      ("PUT", "/me/player/repeat") => {
        self.player["repeat_state"] = json!(parameter(query, "state"))
      }
      ("PUT", "/me/player/volume") => {
        if self.player["device"]["supports_volume"] == false {
          bail!("This demo device has fixed volume");
        }
        self.player["device"]["volume_percent"] =
          json!(parameter(query, "volume_percent").parse::<u8>()?.min(100));
      }
      ("PUT", "/me/player") => {
        let device = body["device_ids"][0].as_str().unwrap_or_default();
        let mut selected = devices()
          .into_iter()
          .find(|value| value["id"] == device)
          .ok_or_else(|| anyhow!("Unknown demo device"))?;
        selected["is_active"] = json!(true);
        self.player["device"] = selected;
        self.player["is_playing"] = body.get("play").cloned().unwrap_or(json!(true));
      }
      ("POST", "/me/player/queue") => {
        if self.queue.len() == QUEUE_LIMIT {
          bail!("Development queue is full ({QUEUE_LIMIT} items)");
        }
        self
          .queue
          .push_back(Self::playable(parameter(query, "uri"))?);
      }
      _ => bail!("Development mode does not implement {method} {path}"),
    }
    self.updated = Instant::now();
    Ok(Value::Null)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::spotify::{RepeatState, Spotify, model::PlayingItem};

  #[tokio::test]
  async fn catalogue_and_library_reads_cover_every_surface() {
    let spotify = Spotify::development();
    assert_eq!(spotify.current_user().await.unwrap().id, "demo");
    assert_eq!(spotify.device().await.unwrap().devices.len(), 3);
    assert_eq!(
      spotify
        .current_user_playlists(20, None)
        .await
        .unwrap()
        .items
        .len(),
      PLAYLISTS
    );
    assert_eq!(
      spotify.playlist(&id('p', 0)).await.unwrap().tracks.total,
      TRACKS as u32
    );
    let first = spotify.current_user_saved_tracks(20, None).await.unwrap();
    let second = spotify
      .current_user_saved_tracks(20, Some(20))
      .await
      .unwrap();
    assert_eq!(first.total, TRACKS as u32);
    assert!(first.next.is_some());
    assert_ne!(first.items[0].track.id, second.items[0].track.id);
    assert!(
      spotify
        .current_user_saved_tracks(20, Some(999))
        .await
        .unwrap()
        .items
        .is_empty()
    );
    assert_eq!(
      spotify
        .playlist_items(&id('p', 1), 20, 20)
        .await
        .unwrap()
        .items
        .len(),
      20
    );
    let albums = spotify.current_user_saved_albums(20, None).await.unwrap();
    assert_eq!(albums.total, ALBUMS as u32);
    assert_eq!(
      spotify.album(&id('a', 0)).await.unwrap().tracks.items.len(),
      4
    );
    assert_eq!(
      spotify
        .album_tracks(&id('a', 0), 2, 2)
        .await
        .unwrap()
        .items
        .len(),
      2
    );
    let artists = spotify
      .current_user_followed_artists(3, None)
      .await
      .unwrap();
    let next = spotify
      .current_user_followed_artists(3, artists.artists.cursors.after)
      .await
      .unwrap();
    assert_ne!(artists.artists.items[0].id, next.artists.items[0].id);
    assert_eq!(
      spotify.artist(&id('r', 0)).await.unwrap().name,
      ARTIST_NAMES[0]
    );
    assert_eq!(
      spotify
        .artist_albums(&id('r', 0), 20, 0)
        .await
        .unwrap()
        .items
        .len(),
      2
    );
    assert_eq!(
      spotify.get_saved_show(20, None).await.unwrap().items.len(),
      SHOWS
    );
    assert_eq!(
      spotify
        .show(&id('s', 0))
        .await
        .unwrap()
        .episodes
        .items
        .len(),
      4
    );
    assert_eq!(
      spotify
        .show_episodes(&id('s', 0), 20, 0)
        .await
        .unwrap()
        .items
        .len(),
      4
    );
    assert_eq!(
      spotify
        .current_user_recently_played(20)
        .await
        .unwrap()
        .items
        .len(),
      20
    );
    assert_eq!(
      spotify.track(&id('t', 0)).await.unwrap().uri,
      uri("track", 0)
    );
    assert_eq!(
      spotify.saved_episodes(20, 20).await.unwrap().items.len(),
      12
    );
    assert_eq!(
      spotify.saved_audiobooks(20, 20).await.unwrap().items.len(),
      4
    );
    assert_eq!(
      spotify.audiobook(&id('b', 0)).await.unwrap().total_chapters,
      CHAPTERS as u32
    );
    assert_eq!(
      spotify
        .audiobook_chapters(&id('b', 0), 20, 20)
        .await
        .unwrap()
        .items
        .len(),
      4
    );
    let search = spotify.search("northbound", 10).await.unwrap();
    assert!(!search.tracks.items.is_empty());
    assert_eq!(search.artists.items.len(), 1);
    assert!(
      spotify
        .search("no-such-sample-zz", 10)
        .await
        .unwrap()
        .tracks
        .items
        .is_empty()
    );
    assert_eq!(
      spotify
        .search_audiobooks("observatory", 10, 0)
        .await
        .unwrap()
        .total,
      4
    );
    assert!(spotify.track("unknown").await.is_err());
  }

  #[tokio::test]
  async fn simulated_writes_are_visible_to_clones_and_reset_on_restart() {
    let spotify = Spotify::development();
    let clone = spotify.clone();
    for (kind, id) in [
      ("track", 0),
      ("episode", 0),
      ("album", 0),
      ("artist", 0),
      ("playlist", 0),
      ("show", 0),
      ("audiobook", 0),
    ] {
      let uri = uri(kind, id);
      spotify.set_saved_item(&uri, false).await.unwrap();
      assert_eq!(
        clone.saved_items(std::slice::from_ref(&uri)).await.unwrap(),
        [false]
      );
      spotify.set_saved_item(&uri, true).await.unwrap();
      assert_eq!(clone.saved_items(&[uri]).await.unwrap(), [true]);
    }
    let before = spotify.saved_episodes(50, 0).await.unwrap();
    spotify
      .set_saved_item(&uri("episode", 0), false)
      .await
      .unwrap();
    assert_eq!(
      spotify.saved_episodes(50, 0).await.unwrap().total,
      before.total - 1
    );
    assert_eq!(
      Spotify::development()
        .saved_items(&[uri("episode", 0)])
        .await
        .unwrap(),
      [true]
    );
    spotify.pause_playback(None).await.unwrap();
    assert!(!clone.current_playback().await.unwrap().unwrap().is_playing);
    spotify.seek_track(12000, None).await.unwrap();
    assert_eq!(
      clone.current_playback().await.unwrap().unwrap().progress_ms,
      Some(12000)
    );
    spotify.volume(75, None).await.unwrap();
    spotify.shuffle(true, None).await.unwrap();
    spotify.repeat(RepeatState::Track, None).await.unwrap();
    let player = clone.current_playback().await.unwrap().unwrap();
    assert_eq!(player.device.volume_percent, 75);
    assert!(player.shuffle_state);
    assert!(matches!(player.repeat_state, RepeatState::Track));
    spotify
      .play_item(&uri("episode", 1), 300000, None)
      .await
      .unwrap();
    let player = clone.current_playback().await.unwrap().unwrap();
    assert!(matches!(player.item, Some(PlayingItem::Episode(_))));
    assert!(player.progress_ms.unwrap() >= 300000);
    spotify
      .start_playback(
        None,
        Some(uri("album", 2)),
        None,
        Some(json!({"position":2})),
      )
      .await
      .unwrap();
    let player = spotify.current_playback().await.unwrap().unwrap();
    assert!(matches!(player.item,Some(PlayingItem::Track(track)) if track.id==Some(id('t',10))));
    let before = spotify.queue().await.unwrap().queue.len();
    spotify
      .add_item_to_queue(&uri("episode", 2), None)
      .await
      .unwrap();
    assert_eq!(clone.queue().await.unwrap().queue.len(), before + 1);
    spotify.next_track(None).await.unwrap();
    assert_eq!(clone.queue().await.unwrap().queue.len(), before);
    spotify.previous_track(None).await.unwrap();
    spotify.transfer_playback("demoPhone", true).await.unwrap();
    assert_eq!(
      clone
        .current_playback()
        .await
        .unwrap()
        .unwrap()
        .device
        .supports_volume,
      Some(false)
    );
    assert!(spotify.volume(50, None).await.is_err());
    assert_eq!(
      spotify
        .device()
        .await
        .unwrap()
        .devices
        .iter()
        .filter(|device| device.is_active)
        .count(),
      1
    );
  }

  #[test]
  fn playback_clock_advances_only_while_playing_and_queue_is_bounded() {
    let mut demo = Demo::new();
    demo.updated -= std::time::Duration::from_secs(2);
    demo.tick();
    assert!(demo.player["progress_ms"].as_u64().unwrap() >= 44000);
    demo.player["is_playing"] = json!(false);
    let paused = demo.player["progress_ms"].clone();
    demo.updated -= std::time::Duration::from_secs(2);
    demo.tick();
    assert_eq!(demo.player["progress_ms"], paused);
    demo.player["is_playing"] = json!(true);
    demo.player["progress_ms"] = json!(180000);
    demo.tick();
    assert_eq!(demo.player["item"]["id"], id('t', 1));
    assert_eq!(demo.player["progress_ms"], 0);
    demo.player["repeat_state"] = json!("track");
    demo.player["progress_ms"] = demo.player["item"]["duration_ms"].clone();
    demo.tick();
    assert_eq!(demo.player["item"]["id"], id('t', 1));
    assert_eq!(demo.player["progress_ms"], 0);
    for _ in demo.queue.len()..QUEUE_LIMIT {
      demo
        .request(
          &Method::POST,
          "/me/player/queue",
          &[("uri", uri("track", 0))],
          None,
        )
        .unwrap();
    }
    assert!(
      demo
        .request(
          &Method::POST,
          "/me/player/queue",
          &[("uri", uri("track", 0))],
          None
        )
        .is_err()
    );
    assert_eq!(demo.queue.len(), QUEUE_LIMIT);
  }
}
