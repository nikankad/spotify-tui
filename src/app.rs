use super::user_config::UserConfig;
use crate::network::IoEvent;
use crate::spotify::model::*;
use anyhow::anyhow;
use std::{cmp::min, collections::HashSet, time::Instant};
use tokio::sync::mpsc::{Sender, error::TrySendError};
use tui::layout::Rect;

use arboard::Clipboard;

pub const LIBRARY_OPTIONS: [&str; 8] = [
  "Recently Played",
  "Liked Songs",
  "Albums",
  "Artists",
  "Podcasts",
  "Queue",
  "Saved Episodes",
  "Audiobooks",
];

const DEFAULT_ROUTE: Route = Route {
  id: RouteId::Home,
  active_block: ActiveBlock::Empty,
  hovered_block: ActiveBlock::Library,
};

use crate::paging::Bookmark;
pub use crate::paging::ScrollableResultPages;

#[derive(Default)]
pub struct SpotifyResultAndSelectedIndex<T> {
  pub index: usize,
  pub result: T,
}

#[derive(Clone)]
pub struct Library {
  pub selected_index: usize,
  pub saved_tracks: ScrollableResultPages<Page<SavedTrack>>,
  pub saved_albums: ScrollableResultPages<Page<SavedAlbum>>,
  pub saved_shows: ScrollableResultPages<Page<Show>>,
  pub saved_artists: ScrollableResultPages<CursorBasedPage<FullArtist>>,
  pub show_episodes: ScrollableResultPages<Page<SimplifiedEpisode>>,
}

#[derive(PartialEq, Debug)]
pub enum SearchResultBlock {
  AlbumSearch,
  SongSearch,
  ArtistSearch,
  PlaylistSearch,
  ShowSearch,
  Empty,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum DialogContext {
  PlaylistWindow,
  PlaylistSearch,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ActiveBlock {
  Media,
  PlayBar,
  AlbumTracks,
  AlbumList,
  ArtistBlock,
  Empty,
  Error,
  HelpMenu,
  Home,
  Input,
  Library,
  MyPlaylists,
  Podcasts,
  EpisodeTable,
  RecentlyPlayed,
  SearchResultBlock,
  SelectDevice,
  TrackTable,
  Artists,
  BasicView,
  Dialog(DialogContext),
}

#[derive(Clone, PartialEq, Debug)]
pub enum RouteId {
  Media(crate::media::View),
  AlbumTracks,
  AlbumList,
  Artist,
  BasicView,
  Error,
  Home,
  RecentlyPlayed,
  Search,
  SelectedDevice,
  TrackTable,
  Artists,
  Podcasts,
  PodcastEpisodes,
  Dialog,
}

#[derive(Debug)]
pub struct Route {
  pub id: RouteId,
  pub active_block: ActiveBlock,
  pub hovered_block: ActiveBlock,
}

// Is it possible to compose enums?
#[derive(PartialEq, Debug)]
pub enum TrackTableContext {
  MyPlaylists,
  AlbumSearch,
  PlaylistSearch,
  SavedTracks,
}

// Is it possible to compose enums?
#[derive(Clone, PartialEq, Debug, Copy)]
pub enum AlbumTableContext {
  Simplified,
  Full,
}

#[derive(Clone, PartialEq, Debug, Copy)]
pub enum EpisodeTableContext {
  Simplified,
  Full,
}

pub struct SearchResult {
  pub albums: Option<Page<SimplifiedAlbum>>,
  pub artists: Option<Page<FullArtist>>,
  pub playlists: Option<Page<SimplifiedPlaylist>>,
  pub tracks: Option<Page<FullTrack>>,
  pub shows: Option<Page<SimplifiedShow>>,
  pub selected_album_index: Option<usize>,
  pub selected_artists_index: Option<usize>,
  pub selected_playlists_index: Option<usize>,
  pub selected_tracks_index: Option<usize>,
  pub selected_shows_index: Option<usize>,
  pub hovered_block: SearchResultBlock,
  pub selected_block: SearchResultBlock,
}

#[derive(Default)]
pub struct TrackTable {
  pub tracks: Vec<FullTrack>,
  pub selected_index: usize,
  pub context: Option<TrackTableContext>,
}

#[derive(Clone)]
pub struct SelectedShow {
  pub show: SimplifiedShow,
}

#[derive(Clone)]
pub struct SelectedFullShow {
  pub show: FullShow,
}

#[derive(Clone)]
pub struct SelectedAlbum {
  pub album: SimplifiedAlbum,
  pub tracks: Page<SimplifiedTrack>,
  pub selected_index: usize,
}

#[derive(Clone)]
pub struct SelectedFullAlbum {
  pub album: FullAlbum,
  pub selected_index: usize,
}

#[derive(Clone)]
pub struct Artist {
  pub artist_name: String,
  pub albums: Page<SimplifiedAlbum>,
  pub selected_album_index: usize,
}

pub struct App {
  pub development: bool,
  pub media: crate::media::MediaLibrary,
  pub instant_since_last_current_playback_poll: Instant,
  navigation_stack: Vec<Route>,
  pub home_scroll: u16,
  pub user_config: UserConfig,
  pub artists: Vec<FullArtist>,
  pub artist: Option<Artist>,
  pub album_table_context: AlbumTableContext,
  pub saved_album_tracks_index: usize,
  pub api_error: String,
  pub current_playback_context: Option<CurrentlyPlaybackContext>,
  pub devices: Option<DevicePayload>,
  // Inputs:
  // input is the string for input;
  // input_idx is the index of the cursor in terms of character;
  // input_cursor_position is the sum of the width of characters preceding the cursor.
  // Reason for this complication is due to non-ASCII characters, they may
  // take more than 1 bytes to store and more than 1 character width to display.
  pub input: Vec<char>,
  pub input_idx: usize,
  pub input_cursor_position: u16,
  pub liked_song_ids_set: HashSet<String>,
  pub followed_artist_ids_set: HashSet<String>,
  pub saved_album_ids_set: HashSet<String>,
  pub saved_show_ids_set: HashSet<String>,
  pub library: Library,
  pub playlist_offset: u32,
  pub playlist_tracks: Option<Page<PlaylistTrack>>,
  pub playlist_uri: Option<String>,
  pub playlists: Option<Page<SimplifiedPlaylist>>,
  pub recently_played: SpotifyResultAndSelectedIndex<Option<CursorBasedPage<PlayHistory>>>,
  pub search_results: SearchResult,
  pub selected_album_simplified: Option<SelectedAlbum>,
  pub selected_album_full: Option<SelectedFullAlbum>,
  pub selected_device_index: Option<usize>,
  pub selected_playlist_index: Option<usize>,
  pub active_playlist_index: Option<usize>,
  pub size: Rect,
  pub song_progress_ms: u128,
  pub seek_ms: Option<u128>,
  pub track_table: TrackTable,
  pub episode_table_context: EpisodeTableContext,
  pub selected_show_simplified: Option<SelectedShow>,
  pub selected_show_full: Option<SelectedFullShow>,
  pub user: Option<PrivateUser>,
  pub album_list_index: usize,
  pub artists_list_index: usize,
  pub clipboard: Option<Clipboard>,
  pub shows_list_index: usize,
  pub episode_list_index: usize,
  pub help_docs_size: u32,
  pub help_menu_page: u32,
  pub help_menu_max_lines: u32,
  pub help_menu_offset: u32,
  pub is_loading: bool,
  io_tx: Option<Sender<IoEvent>>,
  pub is_fetching_current_playback: bool,
  pub dialog: Option<String>,
  pub confirm: bool,
}

impl Default for App {
  fn default() -> Self {
    App {
      development: false,
      media: Default::default(),
      album_table_context: AlbumTableContext::Full,
      album_list_index: 0,
      artists_list_index: 0,
      shows_list_index: 0,
      episode_list_index: 0,
      artists: vec![],
      artist: None,
      user_config: UserConfig::new(),
      saved_album_tracks_index: 0,
      recently_played: Default::default(),
      size: Rect::default(),
      selected_album_simplified: None,
      selected_album_full: None,
      home_scroll: 0,
      library: Library {
        saved_tracks: ScrollableResultPages::new(),
        saved_albums: ScrollableResultPages::new(),
        saved_shows: ScrollableResultPages::new(),
        saved_artists: ScrollableResultPages::new(),
        show_episodes: ScrollableResultPages::new(),
        selected_index: 0,
      },
      liked_song_ids_set: HashSet::new(),
      followed_artist_ids_set: HashSet::new(),
      saved_album_ids_set: HashSet::new(),
      saved_show_ids_set: HashSet::new(),
      navigation_stack: vec![DEFAULT_ROUTE],
      api_error: String::new(),
      current_playback_context: None,
      devices: None,
      input: vec![],
      input_idx: 0,
      input_cursor_position: 0,
      playlist_offset: 0,
      playlist_tracks: None,
      playlist_uri: None,
      playlists: None,
      search_results: SearchResult {
        hovered_block: SearchResultBlock::SongSearch,
        selected_block: SearchResultBlock::Empty,
        albums: None,
        artists: None,
        playlists: None,
        shows: None,
        selected_album_index: None,
        selected_artists_index: None,
        selected_playlists_index: None,
        selected_tracks_index: None,
        selected_shows_index: None,
        tracks: None,
      },
      song_progress_ms: 0,
      seek_ms: None,
      selected_device_index: None,
      selected_playlist_index: None,
      active_playlist_index: None,
      track_table: Default::default(),
      episode_table_context: EpisodeTableContext::Full,
      selected_show_simplified: None,
      selected_show_full: None,
      user: None,
      instant_since_last_current_playback_poll: Instant::now(),
      clipboard: Clipboard::new().ok(),
      help_docs_size: 0,
      help_menu_page: 0,
      help_menu_max_lines: 0,
      help_menu_offset: 0,
      is_loading: false,
      io_tx: None,
      is_fetching_current_playback: false,
      dialog: None,
      confirm: false,
    }
  }
}

impl App {
  pub fn new(io_tx: Sender<IoEvent>, user_config: UserConfig) -> App {
    App {
      io_tx: Some(io_tx),
      user_config,
      ..App::default()
    }
  }

  pub fn playlist_track_position(&self, selected: usize) -> Option<usize> {
    let page = self.playlist_tracks.as_ref()?;
    let position = page
      .items
      .iter()
      .enumerate()
      .filter(|(_, item)| item.track.is_some())
      .nth(selected)?
      .0;
    Some(page.offset as usize + position)
  }

  // Send a network event to the network thread
  pub fn dispatch(&mut self, action: IoEvent) {
    // `is_loading` will be set to false again after the async action has finished in network.rs
    self.is_loading = true;
    if let Some(io_tx) = &self.io_tx {
      match io_tx.try_send(action) {
        Ok(()) => {}
        Err(error) => {
          self.is_loading = false;
          self.is_fetching_current_playback = false;
          self.handle_error(anyhow!(match error {
            TrySendError::Full(_) => "Too many pending Spotify requests; wait and try again",
            TrySendError::Closed(_) => "Spotify network worker has stopped",
          }));
        }
      }
    }
  }

  fn apply_seek(&mut self, seek_ms: u32) {
    if let Some(CurrentlyPlaybackContext {
      item: Some(item), ..
    }) = &self.current_playback_context
    {
      let duration_ms = match item {
        PlayingItem::Track(track) => track.duration_ms,
        PlayingItem::Episode(episode) => episode.duration_ms,
      };

      let event = if seek_ms < duration_ms {
        IoEvent::Seek(seek_ms)
      } else {
        IoEvent::NextTrack
      };

      self.dispatch(event);
    }
  }

  fn poll_current_playback(&mut self) {
    // Poll every 5 seconds
    let poll_interval_ms = 5_000;

    let elapsed = self
      .instant_since_last_current_playback_poll
      .elapsed()
      .as_millis();

    if !self.is_fetching_current_playback && elapsed >= poll_interval_ms {
      self.is_fetching_current_playback = true;
      // Trigger the seek if the user has set a new position
      match self.seek_ms {
        Some(seek_ms) => self.apply_seek(seek_ms as u32),
        None => self.dispatch(IoEvent::GetCurrentPlayback),
      }
    }
  }

  pub fn update_on_tick(&mut self) {
    self.poll_current_playback();
    if let Some(CurrentlyPlaybackContext {
      item: Some(item),
      progress_ms: Some(progress_ms),
      is_playing,
      ..
    }) = &self.current_playback_context
    {
      // Update progress even when the song is not playing,
      // because seeking is possible while paused
      let elapsed = if *is_playing {
        self
          .instant_since_last_current_playback_poll
          .elapsed()
          .as_millis()
      } else {
        0u128
      } + u128::from(*progress_ms);

      let duration_ms = match item {
        PlayingItem::Track(track) => track.duration_ms,
        PlayingItem::Episode(episode) => episode.duration_ms,
      };

      if elapsed < u128::from(duration_ms) {
        self.song_progress_ms = elapsed;
      } else {
        self.song_progress_ms = duration_ms.into();
      }
    }
  }

  pub fn seek_forwards(&mut self) {
    if let Some(CurrentlyPlaybackContext {
      item: Some(item), ..
    }) = &self.current_playback_context
    {
      let duration_ms = match item {
        PlayingItem::Track(track) => track.duration_ms,
        PlayingItem::Episode(episode) => episode.duration_ms,
      };

      let old_progress = match self.seek_ms {
        Some(seek_ms) => seek_ms,
        None => self.song_progress_ms,
      };

      let new_progress = min(
        (old_progress as u32).saturating_add(self.user_config.behavior.seek_milliseconds),
        duration_ms,
      );

      self.seek_ms = Some(new_progress as u128);
    }
  }

  pub fn seek_backwards(&mut self) {
    let old_progress = match self.seek_ms {
      Some(seek_ms) => seek_ms,
      None => self.song_progress_ms,
    };
    let new_progress =
      (old_progress as u32).saturating_sub(self.user_config.behavior.seek_milliseconds);
    self.seek_ms = Some(new_progress as u128);
  }

  pub fn increase_volume(&mut self) {
    if let Some(context) = &self.current_playback_context {
      if context.device.supports_volume == Some(false) {
        return;
      }
      let current_volume = context.device.volume_percent.min(100) as u8;
      let next_volume = current_volume
        .saturating_add(self.user_config.behavior.volume_increment)
        .min(100);

      if next_volume != current_volume {
        self.dispatch(IoEvent::ChangeVolume(next_volume));
      }
    }
  }

  pub fn decrease_volume(&mut self) {
    if let Some(context) = &self.current_playback_context {
      if context.device.supports_volume == Some(false) {
        return;
      }
      let current_volume = context.device.volume_percent.min(100) as u8;
      let next_volume = current_volume.saturating_sub(self.user_config.behavior.volume_increment);

      if next_volume != current_volume {
        self.dispatch(IoEvent::ChangeVolume(next_volume));
      }
    }
  }

  pub fn handle_error(&mut self, e: anyhow::Error) {
    self.push_navigation_stack(RouteId::Error, ActiveBlock::Error);
    self.api_error = e.to_string();
  }

  pub fn toggle_playback(&mut self) {
    if let Some(CurrentlyPlaybackContext {
      is_playing: true, ..
    }) = &self.current_playback_context
    {
      self.dispatch(IoEvent::PausePlayback);
    } else {
      // When no offset or uris are passed, spotify will resume current playback
      self.dispatch(IoEvent::StartPlayback(None, None, None));
    }
  }

  pub fn previous_track(&mut self) {
    if self.song_progress_ms >= 3_000 {
      self.dispatch(IoEvent::Seek(0));
    } else {
      self.dispatch(IoEvent::PreviousTrack);
    }
  }

  // The navigation_stack actually only controls the large block to the right of `library` and
  // `playlists`
  pub fn push_navigation_stack(&mut self, next_route_id: RouteId, next_active_block: ActiveBlock) {
    if !self
      .navigation_stack
      .last()
      .map(|last_route| last_route.id == next_route_id)
      .unwrap_or(false)
    {
      if self.navigation_stack.len() >= 64 {
        self.navigation_stack.remove(1);
      }
      self.navigation_stack.push(Route {
        id: next_route_id,
        active_block: next_active_block,
        hovered_block: next_active_block,
      });
    }
  }

  pub fn pop_navigation_stack(&mut self) -> Option<Route> {
    if self.navigation_stack.len() == 1 {
      None
    } else {
      self.navigation_stack.pop()
    }
  }

  pub fn get_current_route(&self) -> &Route {
    // if for some reason there is no route return the default
    self.navigation_stack.last().unwrap_or(&DEFAULT_ROUTE)
  }

  fn get_current_route_mut(&mut self) -> &mut Route {
    self.navigation_stack.last_mut().unwrap()
  }

  pub fn set_current_route_state(
    &mut self,
    active_block: Option<ActiveBlock>,
    hovered_block: Option<ActiveBlock>,
  ) {
    let current_route = self.get_current_route_mut();
    if let Some(active_block) = active_block {
      current_route.active_block = active_block;
    }
    if let Some(hovered_block) = hovered_block {
      current_route.hovered_block = hovered_block;
    }
  }

  pub fn copy_song_url(&mut self) {
    let clipboard = match &mut self.clipboard {
      Some(ctx) => ctx,
      None => return,
    };

    if let Some(CurrentlyPlaybackContext {
      item: Some(item), ..
    }) = &self.current_playback_context
    {
      match item {
        PlayingItem::Track(track) => {
          if let Err(e) = clipboard.set_text(format!(
            "https://open.spotify.com/track/{}",
            track.id.to_owned().unwrap_or_default()
          )) {
            self.handle_error(anyhow!("failed to set clipboard content: {}", e));
          }
        }
        PlayingItem::Episode(episode) => {
          if let Err(e) = clipboard.set_text(format!(
            "https://open.spotify.com/episode/{}",
            episode.id.to_owned()
          )) {
            self.handle_error(anyhow!("failed to set clipboard content: {}", e));
          }
        }
      }
    }
  }

  pub fn copy_album_url(&mut self) {
    let clipboard = match &mut self.clipboard {
      Some(ctx) => ctx,
      None => return,
    };

    if let Some(CurrentlyPlaybackContext {
      item: Some(item), ..
    }) = &self.current_playback_context
    {
      match item {
        PlayingItem::Track(track) => {
          if let Err(e) = clipboard.set_text(format!(
            "https://open.spotify.com/album/{}",
            track.album.id.to_owned().unwrap_or_default()
          )) {
            self.handle_error(anyhow!("failed to set clipboard content: {}", e));
          }
        }
        PlayingItem::Episode(episode) => {
          if let Err(e) = clipboard.set_text(format!(
            "https://open.spotify.com/show/{}",
            episode.show.id.to_owned()
          )) {
            self.handle_error(anyhow!("failed to set clipboard content: {}", e));
          }
        }
      }
    }
  }

  pub fn get_current_user_saved_artists_next(&mut self) {
    let cache = &mut self.library.saved_artists;
    if let Some(page) = cache.get_results(Some(cache.index + 1)) {
      self.artists = page.items.clone();
      cache.index += 1;
    } else if let Some(last) = cache.get_results(None).and_then(|page| page.items.last()) {
      let id = last.id.clone();
      self.dispatch(IoEvent::GetFollowedArtists(Some(id)));
    }
  }

  pub fn get_current_user_saved_artists_previous(&mut self) {
    let cache = &mut self.library.saved_artists;
    if cache.index == 0 {
      return;
    }
    if let Some(page) = cache.get_results(Some(cache.index - 1)) {
      self.artists = page.items.clone();
      cache.index -= 1;
    } else if let Some(Bookmark::After(after)) = cache.previous_bookmark().cloned() {
      self.dispatch(IoEvent::GetFollowedArtists(after));
    }
  }

  pub fn get_current_user_saved_tracks_next(&mut self) {
    let cache = &mut self.library.saved_tracks;
    if let Some(page) = cache.get_results(Some(cache.index + 1)) {
      self.track_table.tracks = page.items.iter().map(|saved| saved.track.clone()).collect();
      cache.index += 1;
    } else if let Some(page) = cache.get_results(None).filter(|page| page.next.is_some()) {
      let offset = page.offset.saturating_add(page.limit);
      self.dispatch(IoEvent::GetCurrentSavedTracks(Some(offset)));
    }
  }

  pub fn get_current_user_saved_tracks_previous(&mut self) {
    let cache = &mut self.library.saved_tracks;
    if cache.index == 0 {
      return;
    }
    if cache.get_results(Some(cache.index - 1)).is_some() {
      cache.index -= 1;
      self.track_table.tracks = self
        .library
        .saved_tracks
        .get_results(None)
        .map(|page| page.items.iter().map(|saved| saved.track.clone()).collect())
        .unwrap_or_default();
    } else if let Some(Bookmark::Offset(offset)) = cache.previous_bookmark().cloned() {
      self.dispatch(IoEvent::GetCurrentSavedTracks(Some(offset)));
    }
  }

  pub fn shuffle(&mut self) {
    if let Some(context) = &self.current_playback_context {
      self.dispatch(IoEvent::Shuffle(context.shuffle_state));
    };
  }

  pub fn get_current_user_saved_albums_next(&mut self) {
    match self
      .library
      .saved_albums
      .get_results(Some(self.library.saved_albums.index + 1))
    {
      Some(_) => self.library.saved_albums.index += 1,
      None => {
        if let Some(saved_albums) = &self.library.saved_albums.get_results(None) {
          let offset = Some(saved_albums.offset + saved_albums.limit);
          self.dispatch(IoEvent::GetCurrentUserSavedAlbums(offset));
        }
      }
    }
  }

  pub fn get_current_user_saved_albums_previous(&mut self) {
    let cache = &mut self.library.saved_albums;
    if cache.index == 0 {
      return;
    }
    if cache.get_results(Some(cache.index - 1)).is_some() {
      cache.index -= 1;
    } else if let Some(Bookmark::Offset(offset)) = cache.previous_bookmark().cloned() {
      self.dispatch(IoEvent::GetCurrentUserSavedAlbums(Some(offset)));
    }
  }

  pub fn current_user_saved_album_delete(&mut self, block: ActiveBlock) {
    match block {
      ActiveBlock::SearchResultBlock => {
        if let Some(albums) = &self.search_results.albums
          && let Some(selected_index) = self.search_results.selected_album_index
        {
          let selected_album = &albums.items[selected_index];
          if let Some(album_id) = selected_album.id.clone() {
            self.dispatch(IoEvent::CurrentUserSavedAlbumDelete(album_id));
          }
        }
      }
      ActiveBlock::AlbumList => {
        if let Some(albums) = self.library.saved_albums.get_results(None)
          && let Some(selected_album) = albums.items.get(self.album_list_index)
        {
          let album_id = selected_album.album.id.clone();
          self.dispatch(IoEvent::CurrentUserSavedAlbumDelete(album_id));
        }
      }
      ActiveBlock::ArtistBlock => {
        if let Some(artist) = &self.artist
          && let Some(selected_album) = artist.albums.items.get(artist.selected_album_index)
          && let Some(album_id) = selected_album.id.clone()
        {
          self.dispatch(IoEvent::CurrentUserSavedAlbumDelete(album_id));
        }
      }
      _ => (),
    }
  }

  pub fn current_user_saved_album_add(&mut self, block: ActiveBlock) {
    match block {
      ActiveBlock::SearchResultBlock => {
        if let Some(albums) = &self.search_results.albums
          && let Some(selected_index) = self.search_results.selected_album_index
        {
          let selected_album = &albums.items[selected_index];
          if let Some(album_id) = selected_album.id.clone() {
            self.dispatch(IoEvent::CurrentUserSavedAlbumAdd(album_id));
          }
        }
      }
      ActiveBlock::ArtistBlock => {
        if let Some(artist) = &self.artist
          && let Some(selected_album) = artist.albums.items.get(artist.selected_album_index)
          && let Some(album_id) = selected_album.id.clone()
        {
          self.dispatch(IoEvent::CurrentUserSavedAlbumAdd(album_id));
        }
      }
      _ => (),
    }
  }

  pub fn get_current_user_saved_shows_next(&mut self) {
    match self
      .library
      .saved_shows
      .get_results(Some(self.library.saved_shows.index + 1))
    {
      Some(_) => self.library.saved_shows.index += 1,
      None => {
        if let Some(saved_shows) = &self.library.saved_shows.get_results(None) {
          let offset = Some(saved_shows.offset + saved_shows.limit);
          self.dispatch(IoEvent::GetCurrentUserSavedShows(offset));
        }
      }
    }
  }

  pub fn get_current_user_saved_shows_previous(&mut self) {
    let cache = &mut self.library.saved_shows;
    if cache.index == 0 {
      return;
    }
    if cache.get_results(Some(cache.index - 1)).is_some() {
      cache.index -= 1;
    } else if let Some(Bookmark::Offset(offset)) = cache.previous_bookmark().cloned() {
      self.dispatch(IoEvent::GetCurrentUserSavedShows(Some(offset)));
    }
  }

  pub fn get_episode_table_next(&mut self, show_id: String) {
    match self
      .library
      .show_episodes
      .get_results(Some(self.library.show_episodes.index + 1))
    {
      Some(_) => self.library.show_episodes.index += 1,
      None => {
        if let Some(show_episodes) = &self.library.show_episodes.get_results(None) {
          let offset = Some(show_episodes.offset + show_episodes.limit);
          self.dispatch(IoEvent::GetCurrentShowEpisodes(show_id, offset));
        }
      }
    }
  }

  pub fn get_episode_table_previous(&mut self) {
    let cache = &mut self.library.show_episodes;
    if cache.index == 0 {
      return;
    }
    if cache.get_results(Some(cache.index - 1)).is_some() {
      cache.index -= 1;
    } else if let Some(Bookmark::Offset(offset)) = cache.previous_bookmark().cloned() {
      let show = match self.episode_table_context {
        EpisodeTableContext::Full => self
          .selected_show_full
          .as_ref()
          .map(|selected| selected.show.id.clone()),
        EpisodeTableContext::Simplified => self
          .selected_show_simplified
          .as_ref()
          .map(|selected| selected.show.id.clone()),
      };
      if let Some(id) = show {
        self.dispatch(IoEvent::GetCurrentShowEpisodes(id, Some(offset)));
      }
    }
  }

  pub fn user_unfollow_artists(&mut self, block: ActiveBlock) {
    match block {
      ActiveBlock::SearchResultBlock => {
        if let Some(artists) = &self.search_results.artists
          && let Some(selected_index) = self.search_results.selected_artists_index
        {
          let selected_artist: &FullArtist = &artists.items[selected_index];
          let artist_id = selected_artist.id.clone();
          self.dispatch(IoEvent::UserUnfollowArtists(vec![artist_id]));
        }
      }
      ActiveBlock::AlbumList => {
        if let Some(artists) = self.library.saved_artists.get_results(None)
          && let Some(selected_artist) = artists.items.get(self.artists_list_index)
        {
          let artist_id = selected_artist.id.clone();
          self.dispatch(IoEvent::UserUnfollowArtists(vec![artist_id]));
        }
      }

      _ => (),
    };
  }

  pub fn user_follow_artists(&mut self, block: ActiveBlock) {
    if block == ActiveBlock::SearchResultBlock
      && let Some(artists) = &self.search_results.artists
      && let Some(selected_index) = self.search_results.selected_artists_index
    {
      let selected_artist: &FullArtist = &artists.items[selected_index];
      let artist_id = selected_artist.id.clone();
      self.dispatch(IoEvent::UserFollowArtists(vec![artist_id]));
    }
  }

  pub fn user_follow_playlist(&mut self) {
    if let SearchResult {
      playlists: Some(ref playlists),
      selected_playlists_index: Some(selected_index),
      ..
    } = self.search_results
    {
      let selected_playlist: &SimplifiedPlaylist = &playlists.items[selected_index];
      let selected_id = selected_playlist.id.clone();
      let selected_public = selected_playlist.public;
      let selected_owner_id = selected_playlist.owner.id.clone();
      self.dispatch(IoEvent::UserFollowPlaylist(
        selected_owner_id,
        selected_id,
        selected_public,
      ));
    }
  }

  pub fn user_unfollow_playlist(&mut self) {
    if let (Some(playlists), Some(selected_index), Some(user)) =
      (&self.playlists, self.selected_playlist_index, &self.user)
    {
      let selected_playlist = &playlists.items[selected_index];
      let selected_id = selected_playlist.id.clone();
      let user_id = user.id.clone();
      self.dispatch(IoEvent::UserUnfollowPlaylist(user_id, selected_id))
    }
  }

  pub fn user_unfollow_playlist_search_result(&mut self) {
    if let (Some(playlists), Some(selected_index), Some(user)) = (
      &self.search_results.playlists,
      self.search_results.selected_playlists_index,
      &self.user,
    ) {
      let selected_playlist = &playlists.items[selected_index];
      let selected_id = selected_playlist.id.clone();
      let user_id = user.id.clone();
      self.dispatch(IoEvent::UserUnfollowPlaylist(user_id, selected_id))
    }
  }

  pub fn user_follow_show(&mut self, block: ActiveBlock) {
    match block {
      ActiveBlock::SearchResultBlock => {
        if let Some(shows) = &self.search_results.shows
          && let Some(selected_index) = self.search_results.selected_shows_index
          && let Some(show_id) = shows.items.get(selected_index).map(|item| item.id.clone())
        {
          self.dispatch(IoEvent::CurrentUserSavedShowAdd(show_id));
        }
      }
      ActiveBlock::EpisodeTable => match self.episode_table_context {
        EpisodeTableContext::Full => {
          if let Some(selected_episode) = &self.selected_show_full {
            let show_id = selected_episode.show.id.clone();
            self.dispatch(IoEvent::CurrentUserSavedShowAdd(show_id));
          }
        }
        EpisodeTableContext::Simplified => {
          if let Some(selected_episode) = &self.selected_show_simplified {
            let show_id = selected_episode.show.id.clone();
            self.dispatch(IoEvent::CurrentUserSavedShowAdd(show_id));
          }
        }
      },
      _ => (),
    }
  }

  pub fn user_unfollow_show(&mut self, block: ActiveBlock) {
    match block {
      ActiveBlock::Podcasts => {
        if let Some(shows) = self.library.saved_shows.get_results(None)
          && let Some(selected_show) = shows.items.get(self.shows_list_index)
        {
          let show_id = selected_show.show.id.clone();
          self.dispatch(IoEvent::CurrentUserSavedShowDelete(show_id));
        }
      }
      ActiveBlock::SearchResultBlock => {
        if let Some(shows) = &self.search_results.shows
          && let Some(selected_index) = self.search_results.selected_shows_index
        {
          let show_id = shows.items[selected_index].id.to_owned();
          self.dispatch(IoEvent::CurrentUserSavedShowDelete(show_id));
        }
      }
      ActiveBlock::EpisodeTable => match self.episode_table_context {
        EpisodeTableContext::Full => {
          if let Some(selected_episode) = &self.selected_show_full {
            let show_id = selected_episode.show.id.clone();
            self.dispatch(IoEvent::CurrentUserSavedShowDelete(show_id));
          }
        }
        EpisodeTableContext::Simplified => {
          if let Some(selected_episode) = &self.selected_show_simplified {
            let show_id = selected_episode.show.id.clone();
            self.dispatch(IoEvent::CurrentUserSavedShowDelete(show_id));
          }
        }
      },
      _ => (),
    }
  }

  pub fn repeat(&mut self) {
    if let Some(context) = &self.current_playback_context {
      self.dispatch(IoEvent::Repeat(context.repeat_state));
    }
  }

  pub fn get_artist(&mut self, artist_id: String, input_artist_name: String) {
    self.dispatch(IoEvent::GetArtist(artist_id, input_artist_name));
  }

  pub fn calculate_help_menu_offset(&mut self) {
    let old_offset = self.help_menu_offset;

    if self.help_menu_max_lines < self.help_docs_size {
      self.help_menu_offset = self.help_menu_page * self.help_menu_max_lines;
    }
    if self.help_menu_offset > self.help_docs_size {
      self.help_menu_offset = old_offset;
      self.help_menu_page -= 1;
    }
  }
}

#[cfg(test)]
mod review_tests {
  use super::*;

  #[test]
  fn request_backpressure_is_bounded_and_does_not_block_the_ui() {
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    let mut app = App::new(tx, UserConfig::new());
    app.dispatch(IoEvent::NextTrack);
    app.is_fetching_current_playback = true;
    app.dispatch(IoEvent::PreviousTrack);
    assert!(app.api_error.contains("pending"));
    assert!(!app.is_fetching_current_playback);
    assert!(matches!(rx.try_recv().unwrap(), IoEvent::NextTrack));
    assert!(rx.try_recv().is_err());
  }

  #[test]
  fn evicted_library_pages_are_reloaded_when_navigating_back() {
    let (tx, mut rx) = tokio::sync::mpsc::channel(8);
    let mut app = App::new(tx, UserConfig::new());
    for page in 0..20u32 {
      app.library.saved_tracks.add_pages(Page {
        offset: page * 50,
        limit: 50,
        next: Some("next".into()),
        ..Default::default()
      });
    }
    app.library.saved_tracks.add_pages(Page {
      offset: 50,
      limit: 50,
      next: Some("next".into()),
      ..Default::default()
    });
    app.get_current_user_saved_tracks_previous();
    assert!(matches!(
      rx.try_recv().unwrap(),
      IoEvent::GetCurrentSavedTracks(Some(0))
    ));
    app.library.saved_tracks.add_pages(Page {
      offset: 0,
      limit: 50,
      next: Some("next".into()),
      ..Default::default()
    });
    app.get_current_user_saved_tracks_next();
    assert_eq!(app.library.saved_tracks.index, 1);
  }

  #[test]
  fn navigation_history_does_not_grow_without_bound() {
    let mut app = App::default();
    for index in 0..1000 {
      app.push_navigation_stack(
        if index % 2 == 0 {
          RouteId::Home
        } else {
          RouteId::Artist
        },
        ActiveBlock::Empty,
      );
    }
    assert_eq!(app.navigation_stack.len(), 64);
    assert_eq!(app.navigation_stack[0].id, RouteId::Home);
  }
}
