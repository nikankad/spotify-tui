use crate::app::{
  ActiveBlock, AlbumTableContext, App, Artist, EpisodeTableContext, RouteId, ScrollableResultPages,
  SelectedAlbum, SelectedFullAlbum, SelectedFullShow, SelectedShow, TrackTableContext,
};
use crate::config::ClientConfig;
use crate::spotify::RepeatState;
use crate::spotify::Spotify;
use crate::spotify::model::*;
use anyhow::anyhow;
use serde_json::json;
use std::{
  sync::Arc,
  time::{Duration, Instant},
};
use tokio::sync::Mutex;

#[derive(Debug)]
pub enum IoEvent {
  LoadMedia(crate::media::Request),
  OpenAudiobook(String),
  ToggleSavedItem(String),
  PlayMediaItem(String, u32),
  GetCurrentPlayback,
  GetPlaylists,
  GetDevices,
  GetSearchResults(String),
  GetPlaylistTracks(String, u32),
  GetCurrentSavedTracks(Option<u32>),
  StartPlayback(Option<String>, Option<Vec<String>>, Option<usize>),
  UpdateSearchLimits(u32, u32),
  Seek(u32),
  NextTrack,
  PreviousTrack,
  Shuffle(bool),
  Repeat(RepeatState),
  PausePlayback,
  ChangeVolume(u8),
  GetArtist(String, String),
  GetAlbumTracks(Box<SimplifiedAlbum>),
  GetCurrentUserSavedAlbums(Option<u32>),
  CurrentUserSavedAlbumsContains(Vec<String>),
  CurrentUserSavedAlbumDelete(String),
  CurrentUserSavedAlbumAdd(String),
  UserUnfollowArtists(Vec<String>),
  UserFollowArtists(Vec<String>),
  UserFollowPlaylist(String, String, Option<bool>),
  UserUnfollowPlaylist(String, String),
  GetUser,
  ToggleSaveTrack(String),
  GetRecentlyPlayed,
  GetFollowedArtists(Option<String>),
  UserArtistFollowCheck(Vec<String>),
  GetAlbum(String),
  TransferPlaybackToDevice(String),
  GetAlbumForTrack(String),
  CurrentUserSavedTracksContains(Vec<String>),
  GetCurrentUserSavedShows(Option<u32>),
  CurrentUserSavedShowsContains(Vec<String>),
  CurrentUserSavedShowDelete(String),
  CurrentUserSavedShowAdd(String),
  GetShowEpisodes(Box<SimplifiedShow>),
  GetShow(String),
  GetCurrentShowEpisodes(String, Option<u32>),
  AddItemToQueue(String),
}

#[derive(Clone)]
pub struct Network<'a> {
  pub spotify: Spotify,
  large_search_limit: u32,
  small_search_limit: u32,
  pub client_config: ClientConfig,
  pub app: &'a Arc<Mutex<App>>,
}

impl<'a> Network<'a> {
  pub fn new(spotify: Spotify, client_config: ClientConfig, app: &'a Arc<Mutex<App>>) -> Self {
    Network {
      spotify,
      large_search_limit: 20,
      small_search_limit: 4,
      client_config,
      app,
    }
  }

  pub fn set_device_id(&mut self, device_id: String) -> anyhow::Result<()> {
    if self.spotify.is_development() {
      self.client_config.device_id = Some(device_id);
      Ok(())
    } else {
      self.client_config.set_device_id(device_id)
    }
  }

  pub async fn populate_development(&mut self) {
    debug_assert!(self.spotify.is_development());
    for event in [
      IoEvent::GetUser,
      IoEvent::GetPlaylists,
      IoEvent::GetDevices,
      IoEvent::GetCurrentPlayback,
      IoEvent::GetRecentlyPlayed,
      IoEvent::GetCurrentUserSavedAlbums(None),
      IoEvent::GetFollowedArtists(None),
      IoEvent::GetCurrentUserSavedShows(None),
      IoEvent::GetCurrentSavedTracks(None),
      IoEvent::LoadMedia(crate::media::Request::new(crate::media::View::Queue)),
      IoEvent::LoadMedia(crate::media::Request::new(crate::media::View::Episodes)),
      IoEvent::LoadMedia(crate::media::Request::new(crate::media::View::Audiobooks)),
    ] {
      self.handle_network_event(event).await;
    }
    let mut app = self.app.lock().await;
    if app.api_error.is_empty() {
      app.push_navigation_stack(RouteId::TrackTable, ActiveBlock::TrackTable);
      app.set_current_route_state(Some(ActiveBlock::TrackTable), Some(ActiveBlock::TrackTable));
    }
  }

  #[allow(clippy::cognitive_complexity)]
  pub async fn handle_network_event(&mut self, io_event: IoEvent) {
    match io_event {
      IoEvent::LoadMedia(request) => {
        let view = request.view;
        match crate::media::MediaPage::load(&self.spotify, request, self.large_search_limit).await {
          Ok(page) => {
            let mut app = self.app.lock().await;
            *app.media.page_mut(view) = page;
            app.push_navigation_stack(RouteId::Media(view), ActiveBlock::Media);
            app.set_current_route_state(Some(ActiveBlock::Media), Some(ActiveBlock::Media));
          }
          Err(error) => self.handle_error(error).await,
        }
      }
      IoEvent::OpenAudiobook(id) => match self.spotify.audiobook(&id).await {
        Ok(book) => {
          self
            .app
            .lock()
            .await
            .dispatch(IoEvent::LoadMedia(crate::media::Request::chapters(
              book.id, book.name,
            )))
        }
        Err(error) => self.handle_error(error).await,
      },
      IoEvent::ToggleSavedItem(uri) => {
        match self.spotify.saved_items(std::slice::from_ref(&uri)).await {
          Ok(saved) => {
            let save = !saved[0];
            match self.spotify.set_saved_item(&uri, save).await {
              Ok(()) => {
                let mut app = self.app.lock().await;
                app.media.mark_saved(&uri, save);
                // A removal changes subsequent page boundaries; reload the visible
                // library page so the next page cannot skip the item that shifted.
                if let RouteId::Media(
                  view @ (crate::media::View::Episodes | crate::media::View::Audiobooks),
                ) = app.get_current_route().id
                {
                  let page = app.media.page(view);
                  if let Some(mut request) = page.request.clone() {
                    if page.entries.is_empty() && request.offset > 0 {
                      request.offset = request.offset.saturating_sub(page.limit.max(1));
                    }
                    app.dispatch(IoEvent::LoadMedia(request));
                  }
                }

                if let Some(id) = uri.strip_prefix("spotify:track:") {
                  if save {
                    app.liked_song_ids_set.insert(id.into());
                  } else {
                    app.liked_song_ids_set.remove(id);
                  }
                }
              }
              Err(error) => self.handle_error(error).await,
            }
          }
          Err(error) => self.handle_error(error).await,
        }
      }
      IoEvent::PlayMediaItem(uri, position) => {
        match self
          .spotify
          .play_item(&uri, position, self.client_config.device_id.clone())
          .await
        {
          Ok(()) => self.get_current_playback().await,
          Err(error) => self.handle_error(error).await,
        }
      }

      IoEvent::GetPlaylists => {
        self.get_current_user_playlists().await;
      }
      IoEvent::GetUser => {
        self.get_user().await;
      }
      IoEvent::GetDevices => {
        self.get_devices().await;
      }
      IoEvent::GetCurrentPlayback => {
        self.get_current_playback().await;
      }
      IoEvent::GetSearchResults(search_term) => {
        self.get_search_results(search_term).await;
      }

      IoEvent::GetPlaylistTracks(playlist_id, playlist_offset) => {
        self.get_playlist_tracks(playlist_id, playlist_offset).await;
      }
      IoEvent::GetCurrentSavedTracks(offset) => {
        self.get_current_user_saved_tracks(offset).await;
      }
      IoEvent::StartPlayback(context_uri, uris, offset) => {
        self.start_playback(context_uri, uris, offset).await;
      }
      IoEvent::UpdateSearchLimits(large_search_limit, small_search_limit) => {
        self.large_search_limit = large_search_limit.clamp(1, 50);
        self.small_search_limit = small_search_limit.clamp(1, 10);
      }
      IoEvent::Seek(position_ms) => {
        self.seek(position_ms).await;
      }
      IoEvent::NextTrack => {
        self.next_track().await;
      }
      IoEvent::PreviousTrack => {
        self.previous_track().await;
      }
      IoEvent::Repeat(repeat_state) => {
        self.repeat(repeat_state).await;
      }
      IoEvent::PausePlayback => {
        self.pause_playback().await;
      }
      IoEvent::ChangeVolume(volume) => {
        self.change_volume(volume).await;
      }
      IoEvent::GetArtist(artist_id, input_artist_name) => {
        self.get_artist(artist_id, input_artist_name).await;
      }
      IoEvent::GetAlbumTracks(album) => {
        self.get_album_tracks(album).await;
      }

      IoEvent::GetCurrentUserSavedAlbums(offset) => {
        self.get_current_user_saved_albums(offset).await;
      }
      IoEvent::CurrentUserSavedAlbumsContains(album_ids) => {
        self.current_user_saved_albums_contains(album_ids).await;
      }
      IoEvent::CurrentUserSavedAlbumDelete(album_id) => {
        self.current_user_saved_album_delete(album_id).await;
      }
      IoEvent::CurrentUserSavedAlbumAdd(album_id) => {
        self.current_user_saved_album_add(album_id).await;
      }
      IoEvent::UserUnfollowArtists(artist_ids) => {
        self.user_unfollow_artists(artist_ids).await;
      }
      IoEvent::UserFollowArtists(artist_ids) => {
        self.user_follow_artists(artist_ids).await;
      }
      IoEvent::UserFollowPlaylist(playlist_owner_id, playlist_id, is_public) => {
        self
          .user_follow_playlist(playlist_owner_id, playlist_id, is_public)
          .await;
      }
      IoEvent::UserUnfollowPlaylist(user_id, playlist_id) => {
        self.user_unfollow_playlist(user_id, playlist_id).await;
      }

      IoEvent::ToggleSaveTrack(track_id) => {
        self.toggle_save_track(track_id).await;
      }

      IoEvent::GetRecentlyPlayed => {
        self.get_recently_played().await;
      }
      IoEvent::GetFollowedArtists(after) => {
        self.get_followed_artists(after).await;
      }
      IoEvent::UserArtistFollowCheck(artist_ids) => {
        self.user_artist_check_follow(artist_ids).await;
      }
      IoEvent::GetAlbum(album_id) => {
        self.get_album(album_id).await;
      }
      IoEvent::TransferPlaybackToDevice(device_id) => {
        self.transfert_playback_to_device(device_id).await;
      }
      IoEvent::GetAlbumForTrack(track_id) => {
        self.get_album_for_track(track_id).await;
      }
      IoEvent::Shuffle(shuffle_state) => {
        self.shuffle(shuffle_state).await;
      }
      IoEvent::CurrentUserSavedTracksContains(track_ids) => {
        self.current_user_saved_tracks_contains(track_ids).await;
      }
      IoEvent::GetCurrentUserSavedShows(offset) => {
        self.get_current_user_saved_shows(offset).await;
      }
      IoEvent::CurrentUserSavedShowsContains(show_ids) => {
        self.current_user_saved_shows_contains(show_ids).await;
      }
      IoEvent::CurrentUserSavedShowDelete(show_id) => {
        self.current_user_saved_shows_delete(show_id).await;
      }
      IoEvent::CurrentUserSavedShowAdd(show_id) => {
        self.current_user_saved_shows_add(show_id).await;
      }
      IoEvent::GetShowEpisodes(show) => {
        self.get_show_episodes(show).await;
      }
      IoEvent::GetShow(show_id) => {
        self.get_show(show_id).await;
      }
      IoEvent::GetCurrentShowEpisodes(show_id, offset) => {
        self.get_current_show_episodes(show_id, offset).await;
      }
      IoEvent::AddItemToQueue(item) => {
        self.add_item_to_queue(item).await;
      }
    };

    let mut app = self.app.lock().await;
    app.is_loading = false;
  }

  async fn handle_error(&mut self, e: anyhow::Error) {
    let mut app = self.app.lock().await;
    app.handle_error(e);
  }

  async fn get_user(&mut self) {
    match self.spotify.current_user().await {
      Ok(user) => {
        let mut app = self.app.lock().await;
        app.user = Some(user);
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn get_devices(&mut self) {
    match self.spotify.device().await {
      Ok(mut result) => {
        result
          .devices
          .retain(|device| !device.id.is_empty() && !device.is_restricted);
        let mut app = self.app.lock().await;
        app.push_navigation_stack(RouteId::SelectedDevice, ActiveBlock::SelectDevice);
        app.selected_device_index = (!result.devices.is_empty()).then_some(0);
        app.devices = Some(result);
      }
      Err(error) => self.handle_error(error).await,
    }
  }

  async fn get_current_playback(&mut self) {
    let context = self.spotify.current_playback().await;

    match context {
      Ok(Some(c)) => {
        let mut app = self.app.lock().await;
        let previous_track_id =
          app
            .current_playback_context
            .as_ref()
            .and_then(|context| match &context.item {
              Some(PlayingItem::Track(track)) => track.id.as_deref(),
              _ => None,
            });
        let changed_track = c.item.as_ref().and_then(|item| match item {
          PlayingItem::Track(track) => track
            .id
            .as_ref()
            .filter(|id| Some(id.as_str()) != previous_track_id)
            .cloned(),
          PlayingItem::Episode(_) => None,
        });
        app.current_playback_context = Some(c);
        app.instant_since_last_current_playback_poll = Instant::now();
        if let Some(id) = changed_track {
          app.dispatch(IoEvent::CurrentUserSavedTracksContains(vec![id]));
        }
      }
      Ok(None) => {
        let mut app = self.app.lock().await;
        app.current_playback_context = None;
        app.song_progress_ms = 0;
        app.instant_since_last_current_playback_poll = Instant::now();
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
        self
          .app
          .lock()
          .await
          .instant_since_last_current_playback_poll = Instant::now();
      }
    }

    let mut app = self.app.lock().await;
    app.seek_ms.take();
    app.is_fetching_current_playback = false;
  }

  async fn current_user_saved_tracks_contains(&mut self, ids: Vec<String>) {
    match self.spotify.current_user_saved_tracks_contains(&ids).await {
      Ok(is_saved_vec) => {
        let mut app = self.app.lock().await;
        for (i, id) in ids.iter().enumerate() {
          if let Some(is_liked) = is_saved_vec.get(i) {
            if *is_liked {
              app.liked_song_ids_set.insert(id.to_string());
            } else {
              // The song is not liked, so check if it should be removed
              if app.liked_song_ids_set.contains(id) {
                app.liked_song_ids_set.remove(id);
              }
            }
          };
        }
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn get_playlist_tracks(&mut self, playlist_id: String, playlist_offset: u32) {
    match self
      .spotify
      .playlist_items(&playlist_id, self.large_search_limit, playlist_offset)
      .await
    {
      Ok(playlist_tracks) => {
        self.set_playlist_tracks_to_table(&playlist_tracks).await;
        let mut app = self.app.lock().await;
        app.playlist_uri = Some(format!(
          "spotify:playlist:{}",
          playlist_id.rsplit(':').next().unwrap_or(&playlist_id)
        ));
        if !matches!(
          app.track_table.context,
          Some(TrackTableContext::MyPlaylists | TrackTableContext::PlaylistSearch)
        ) {
          app.track_table.context = Some(TrackTableContext::MyPlaylists);
        }
        app.playlist_tracks = Some(playlist_tracks);
        app.track_table.selected_index = 0;
        app.push_navigation_stack(RouteId::TrackTable, ActiveBlock::TrackTable);
      }
      Err(error) => self.handle_error(error).await,
    }
  }

  async fn set_playlist_tracks_to_table(&mut self, playlist_track_page: &Page<PlaylistTrack>) {
    self
      .set_tracks_to_table(
        playlist_track_page
          .items
          .iter()
          .filter_map(|item| item.track.clone())
          .collect::<Vec<FullTrack>>(),
      )
      .await;
  }

  async fn set_tracks_to_table(&mut self, tracks: Vec<FullTrack>) {
    let mut app = self.app.lock().await;
    let ids = tracks.iter().filter_map(|item| item.id.clone()).collect();
    app.track_table.tracks = tracks;

    // Send this event round (don't block here)
    app.dispatch(IoEvent::CurrentUserSavedTracksContains(ids));
  }

  async fn get_current_user_saved_shows(&mut self, offset: Option<u32>) {
    if offset.is_none() {
      self.app.lock().await.library.saved_shows = ScrollableResultPages::new();
    }

    match self
      .spotify
      .get_saved_show(self.large_search_limit, offset)
      .await
    {
      Ok(saved_shows) => {
        // not to show a blank page
        if !saved_shows.items.is_empty() {
          let mut app = self.app.lock().await;
          app.library.saved_shows.add_pages(saved_shows);
        }
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn current_user_saved_shows_contains(&mut self, show_ids: Vec<String>) {
    if let Ok(are_followed) = self
      .spotify
      .check_users_saved_shows(show_ids.to_owned())
      .await
    {
      let mut app = self.app.lock().await;
      show_ids.iter().enumerate().for_each(|(i, id)| {
        if are_followed[i] {
          app.saved_show_ids_set.insert(id.to_owned());
        } else {
          app.saved_show_ids_set.remove(id);
        }
      })
    }
  }

  async fn get_show_episodes(&mut self, show: Box<SimplifiedShow>) {
    match self
      .spotify
      .show_episodes(&show.id, self.large_search_limit, 0)
      .await
    {
      Ok(episodes) => {
        if !episodes.items.is_empty() {
          let mut app = self.app.lock().await;
          app.library.show_episodes = ScrollableResultPages::new();
          app.library.show_episodes.add_pages(episodes);

          app.selected_show_simplified = Some(SelectedShow { show: *show });

          app.episode_table_context = EpisodeTableContext::Simplified;

          app.push_navigation_stack(RouteId::PodcastEpisodes, ActiveBlock::EpisodeTable);
        }
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn get_show(&mut self, show_id: String) {
    match self.spotify.show(&show_id).await {
      Ok(show) => {
        let selected_show = SelectedFullShow { show };

        let mut app = self.app.lock().await;

        app.selected_show_full = Some(selected_show);

        app.episode_table_context = EpisodeTableContext::Full;
        app.push_navigation_stack(RouteId::PodcastEpisodes, ActiveBlock::EpisodeTable);
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn get_current_show_episodes(&mut self, show_id: String, offset: Option<u32>) {
    match self
      .spotify
      .show_episodes(&show_id, self.large_search_limit, offset)
      .await
    {
      Ok(episodes) => {
        if !episodes.items.is_empty() {
          let mut app = self.app.lock().await;
          app.library.show_episodes.add_pages(episodes);
        }
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn get_search_results(&mut self, search_term: String) {
    match self
      .spotify
      .search(&search_term, self.small_search_limit)
      .await
    {
      Ok(SearchResults {
        tracks: track_results,
        artists: artist_results,
        albums: album_results,
        playlists: playlist_results,
        shows: show_results,
      }) => {
        let mut app = self.app.lock().await;

        let artist_ids = artist_results
          .items
          .iter()
          .map(|item| item.id.to_owned())
          .collect();

        // Check if these artists are followed
        app.dispatch(IoEvent::UserArtistFollowCheck(artist_ids));

        let album_ids = album_results
          .items
          .iter()
          .filter_map(|album| album.id.to_owned())
          .collect();

        // Check if these albums are saved
        app.dispatch(IoEvent::CurrentUserSavedAlbumsContains(album_ids));

        let show_ids = show_results
          .items
          .iter()
          .map(|show| show.id.to_owned())
          .collect();

        // check if these shows are saved
        app.dispatch(IoEvent::CurrentUserSavedShowsContains(show_ids));

        app.search_results.tracks = Some(track_results);
        app.search_results.artists = Some(artist_results);
        app.search_results.albums = Some(album_results);
        app.search_results.playlists = Some(playlist_results);
        app.search_results.shows = Some(show_results);
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    };
  }

  async fn get_current_user_saved_tracks(&mut self, offset: Option<u32>) {
    if offset.is_none() {
      self.app.lock().await.library.saved_tracks = ScrollableResultPages::new();
    }

    match self
      .spotify
      .current_user_saved_tracks(self.large_search_limit, offset)
      .await
    {
      Ok(saved_tracks) => {
        let mut app = self.app.lock().await;
        app.track_table.tracks = saved_tracks
          .items
          .iter()
          .map(|item| item.track.clone())
          .collect::<Vec<FullTrack>>();

        saved_tracks.items.iter().for_each(|item| {
          if let Some(track_id) = &item.track.id {
            app.liked_song_ids_set.insert(track_id.to_string());
          }
        });

        app.library.saved_tracks.add_pages(saved_tracks);
        app.track_table.context = Some(TrackTableContext::SavedTracks);
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn start_playback(
    &mut self,
    context_uri: Option<String>,
    uris: Option<Vec<String>>,
    offset: Option<usize>,
  ) {
    let (uris, context_uri) = if context_uri.is_some() {
      (None, context_uri)
    } else if uris.is_some() {
      (uris, None)
    } else {
      (None, None)
    };

    let offset = offset.map(|position| json!({"position": position}));

    let result = self
      .spotify
      .start_playback(
        self.client_config.device_id.clone(),
        context_uri,
        uris,
        offset,
      )
      .await;

    match result {
      Ok(()) => {
        let mut app = self.app.lock().await;
        app.song_progress_ms = 0;
        app.dispatch(IoEvent::GetCurrentPlayback);
      }
      Err(e) => {
        self.handle_error(e).await;
      }
    }
  }

  async fn seek(&mut self, position_ms: u32) {
    match self
      .spotify
      .seek_track(position_ms, self.client_config.device_id.clone())
      .await
    {
      Ok(()) => {
        // Spotify may briefly report the old position after accepting a seek.
        tokio::time::sleep(Duration::from_millis(1000)).await;
        self.get_current_playback().await;
      }
      Err(error) => self.handle_error(error).await,
    }
  }

  async fn next_track(&mut self) {
    match self
      .spotify
      .next_track(self.client_config.device_id.clone())
      .await
    {
      Ok(()) => {
        self.get_current_playback().await;
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    };
  }

  async fn previous_track(&mut self) {
    match self
      .spotify
      .previous_track(self.client_config.device_id.clone())
      .await
    {
      Ok(()) => {
        self.get_current_playback().await;
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    };
  }

  async fn shuffle(&mut self, shuffle_state: bool) {
    match self
      .spotify
      .shuffle(!shuffle_state, self.client_config.device_id.clone())
      .await
    {
      Ok(()) => {
        // Update the UI eagerly (otherwise the UI will wait until the next 5 second interval
        // due to polling playback context)
        let mut app = self.app.lock().await;
        if let Some(current_playback_context) = &mut app.current_playback_context {
          current_playback_context.shuffle_state = !shuffle_state;
        };
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    };
  }

  async fn repeat(&mut self, repeat_state: RepeatState) {
    let next_repeat_state = match repeat_state {
      RepeatState::Off => RepeatState::Context,
      RepeatState::Context => RepeatState::Track,
      RepeatState::Track => RepeatState::Off,
    };
    match self
      .spotify
      .repeat(next_repeat_state, self.client_config.device_id.clone())
      .await
    {
      Ok(()) => {
        let mut app = self.app.lock().await;
        if let Some(current_playback_context) = &mut app.current_playback_context {
          current_playback_context.repeat_state = next_repeat_state;
        };
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    };
  }

  async fn pause_playback(&mut self) {
    match self
      .spotify
      .pause_playback(self.client_config.device_id.clone())
      .await
    {
      Ok(()) => {
        self.get_current_playback().await;
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    };
  }

  async fn change_volume(&mut self, volume_percent: u8) {
    match self
      .spotify
      .volume(volume_percent, self.client_config.device_id.clone())
      .await
    {
      Ok(()) => {
        let mut app = self.app.lock().await;
        if let Some(current_playback_context) = &mut app.current_playback_context {
          current_playback_context.device.volume_percent = volume_percent.into();
        };
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    };
  }

  async fn get_artist(&mut self, artist_id: String, input_artist_name: String) {
    let albums = self
      .spotify
      .artist_albums(&artist_id, self.large_search_limit, 0);
    let artist_name = if input_artist_name.is_empty() {
      self
        .spotify
        .artist(&artist_id)
        .await
        .map(|full_artist| full_artist.name)
        .unwrap_or_default()
    } else {
      input_artist_name
    };
    match albums.await {
      Ok(albums) => {
        let mut app = self.app.lock().await;
        app.dispatch(IoEvent::CurrentUserSavedAlbumsContains(
          albums
            .items
            .iter()
            .filter_map(|item| item.id.clone())
            .collect(),
        ));
        app.artist = Some(Artist {
          artist_name,
          albums,
          selected_album_index: 0,
        });
      }
      Err(error) => self.handle_error(error).await,
    }
  }

  async fn get_album_tracks(&mut self, album: Box<SimplifiedAlbum>) {
    if let Some(album_id) = &album.id {
      match self
        .spotify
        .album_tracks(&album_id.clone(), self.large_search_limit, 0)
        .await
      {
        Ok(tracks) => {
          let track_ids = tracks
            .items
            .iter()
            .filter_map(|item| item.id.clone())
            .collect::<Vec<String>>();

          let mut app = self.app.lock().await;
          app.selected_album_simplified = Some(SelectedAlbum {
            album: *album,
            tracks,
            selected_index: 0,
          });

          app.album_table_context = AlbumTableContext::Simplified;
          app.push_navigation_stack(RouteId::AlbumTracks, ActiveBlock::AlbumTracks);
          app.dispatch(IoEvent::CurrentUserSavedTracksContains(track_ids));
        }
        Err(e) => {
          self.handle_error(anyhow!(e)).await;
        }
      }
    }
  }

  async fn toggle_save_track(&mut self, track_id: String) {
    match self
      .spotify
      .current_user_saved_tracks_contains(std::slice::from_ref(&track_id))
      .await
    {
      Ok(saved) => {
        if saved.first() == Some(&true) {
          match self
            .spotify
            .current_user_saved_tracks_delete(std::slice::from_ref(&track_id))
            .await
          {
            Ok(()) => {
              let mut app = self.app.lock().await;
              app.liked_song_ids_set.remove(&track_id);
            }
            Err(e) => {
              self.handle_error(anyhow!(e)).await;
            }
          }
        } else {
          match self
            .spotify
            .current_user_saved_tracks_add(std::slice::from_ref(&track_id))
            .await
          {
            Ok(()) => {
              // TODO: This should ideally use the same logic as `self.current_user_saved_tracks_contains`
              let mut app = self.app.lock().await;
              app.liked_song_ids_set.insert(track_id);
            }
            Err(e) => {
              self.handle_error(anyhow!(e)).await;
            }
          }
        }
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    };
  }

  async fn get_followed_artists(&mut self, after: Option<String>) {
    match self
      .spotify
      .current_user_followed_artists(self.large_search_limit, after)
      .await
    {
      Ok(saved_artists) => {
        let mut app = self.app.lock().await;
        app.artists = saved_artists.artists.items.to_owned();
        app.library.saved_artists.add_pages(saved_artists.artists);
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    };
  }

  async fn user_artist_check_follow(&mut self, artist_ids: Vec<String>) {
    if let Ok(are_followed) = self.spotify.user_artist_check_follow(&artist_ids).await {
      let mut app = self.app.lock().await;
      artist_ids.iter().enumerate().for_each(|(i, id)| {
        if are_followed[i] {
          app.followed_artist_ids_set.insert(id.to_owned());
        } else {
          app.followed_artist_ids_set.remove(id);
        }
      });
    }
  }

  async fn get_current_user_saved_albums(&mut self, offset: Option<u32>) {
    if offset.is_none() {
      self.app.lock().await.library.saved_albums = ScrollableResultPages::new();
    }

    match self
      .spotify
      .current_user_saved_albums(self.large_search_limit, offset)
      .await
    {
      Ok(saved_albums) => {
        // not to show a blank page
        if !saved_albums.items.is_empty() {
          let mut app = self.app.lock().await;
          app.library.saved_albums.add_pages(saved_albums);
        }
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    };
  }

  async fn current_user_saved_albums_contains(&mut self, album_ids: Vec<String>) {
    if let Ok(are_followed) = self
      .spotify
      .current_user_saved_albums_contains(&album_ids)
      .await
    {
      let mut app = self.app.lock().await;
      album_ids.iter().enumerate().for_each(|(i, id)| {
        if are_followed[i] {
          app.saved_album_ids_set.insert(id.to_owned());
        } else {
          app.saved_album_ids_set.remove(id);
        }
      });
    }
  }

  pub async fn current_user_saved_album_delete(&mut self, album_id: String) {
    match self
      .spotify
      .current_user_saved_albums_delete(&[album_id.to_owned()])
      .await
    {
      Ok(_) => {
        self.get_current_user_saved_albums(None).await;
        let mut app = self.app.lock().await;
        app.saved_album_ids_set.remove(&album_id.to_owned());
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    };
  }

  async fn current_user_saved_album_add(&mut self, album_id: String) {
    match self
      .spotify
      .current_user_saved_albums_add(&[album_id.to_owned()])
      .await
    {
      Ok(_) => {
        let mut app = self.app.lock().await;
        app.saved_album_ids_set.insert(album_id.to_owned());
      }
      Err(e) => self.handle_error(anyhow!(e)).await,
    }
  }

  async fn current_user_saved_shows_delete(&mut self, show_id: String) {
    match self
      .spotify
      .remove_users_saved_shows(vec![show_id.to_owned()])
      .await
    {
      Ok(_) => {
        self.get_current_user_saved_shows(None).await;
        let mut app = self.app.lock().await;
        app.saved_show_ids_set.remove(&show_id.to_owned());
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn current_user_saved_shows_add(&mut self, show_id: String) {
    match self.spotify.save_shows(vec![show_id.to_owned()]).await {
      Ok(_) => {
        self.get_current_user_saved_shows(None).await;
        let mut app = self.app.lock().await;
        app.saved_show_ids_set.insert(show_id.to_owned());
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn user_unfollow_artists(&mut self, artist_ids: Vec<String>) {
    match self.spotify.user_unfollow_artists(&artist_ids).await {
      Ok(_) => {
        self.app.lock().await.library.saved_artists = ScrollableResultPages::new();
        self.get_followed_artists(None).await;
        let mut app = self.app.lock().await;
        artist_ids.iter().for_each(|id| {
          app.followed_artist_ids_set.remove(&id.to_owned());
        });
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn user_follow_artists(&mut self, artist_ids: Vec<String>) {
    match self.spotify.user_follow_artists(&artist_ids).await {
      Ok(_) => {
        self.app.lock().await.library.saved_artists = ScrollableResultPages::new();
        self.get_followed_artists(None).await;
        let mut app = self.app.lock().await;
        artist_ids.iter().for_each(|id| {
          app.followed_artist_ids_set.insert(id.to_owned());
        });
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn user_follow_playlist(
    &mut self,
    _playlist_owner_id: String,
    playlist_id: String,
    _is_public: Option<bool>,
  ) {
    match self.spotify.follow_playlist(&playlist_id).await {
      Ok(_) => {
        self.get_current_user_playlists().await;
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn user_unfollow_playlist(&mut self, _user_id: String, playlist_id: String) {
    match self.spotify.unfollow_playlist(&playlist_id).await {
      Ok(_) => {
        self.get_current_user_playlists().await;
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn get_current_user_playlists(&mut self) {
    let playlists = self
      .spotify
      .current_user_playlists(self.large_search_limit, None)
      .await;

    match playlists {
      Ok(p) => {
        let mut app = self.app.lock().await;
        app.playlists = Some(p);
        // Select the first playlist
        app.selected_playlist_index = app
          .playlists
          .as_ref()
          .filter(|page| !page.items.is_empty())
          .map(|_| 0);
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    };
  }

  async fn get_recently_played(&mut self) {
    match self
      .spotify
      .current_user_recently_played(self.large_search_limit)
      .await
    {
      Ok(result) => {
        let track_ids = result
          .items
          .iter()
          .filter_map(|item| item.track.id.clone())
          .collect::<Vec<String>>();

        self.current_user_saved_tracks_contains(track_ids).await;

        let mut app = self.app.lock().await;

        app.recently_played.result = Some(result);
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn get_album(&mut self, album_id: String) {
    match self.spotify.album(&album_id).await {
      Ok(album) => {
        let selected_album = SelectedFullAlbum {
          album,
          selected_index: 0,
        };

        let mut app = self.app.lock().await;

        app.selected_album_full = Some(selected_album);
        app.album_table_context = AlbumTableContext::Full;
        app.push_navigation_stack(RouteId::AlbumTracks, ActiveBlock::AlbumTracks);
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn get_album_for_track(&mut self, track_id: String) {
    match self.spotify.track(&track_id).await {
      Ok(track) => {
        // It is unclear when the id can ever be None, but perhaps a track can be album-less. If
        // so, there isn't much to do here anyways, since we're looking for the parent album.
        let album_id = match track.album.id {
          Some(id) => id,
          None => return,
        };

        if let Ok(album) = self.spotify.album(&album_id).await {
          // The way we map to the UI is zero-indexed, but Spotify is 1-indexed.
          let zero_indexed_track_number = track.track_number - 1;
          let selected_album = SelectedFullAlbum {
            album,
            // Overflow should be essentially impossible here, so we prefer the cleaner 'as'.
            selected_index: zero_indexed_track_number as usize,
          };

          let mut app = self.app.lock().await;

          app.saved_album_tracks_index = selected_album.selected_index;
          app.selected_album_full = Some(selected_album);
          app.album_table_context = AlbumTableContext::Full;
          app.push_navigation_stack(RouteId::AlbumTracks, ActiveBlock::AlbumTracks);
        }
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }

  async fn transfert_playback_to_device(&mut self, device_id: String) {
    match self.spotify.transfer_playback(&device_id, true).await {
      Ok(()) => {
        self.get_current_playback().await;
      }
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
        return;
      }
    };

    match self.set_device_id(device_id) {
      Ok(()) => {
        let mut app = self.app.lock().await;
        app.pop_navigation_stack();
      }
      Err(e) => {
        self.handle_error(e).await;
      }
    };
  }

  async fn add_item_to_queue(&mut self, item: String) {
    match self
      .spotify
      .add_item_to_queue(&item, self.client_config.device_id.clone())
      .await
    {
      Ok(()) => (),
      Err(e) => {
        self.handle_error(anyhow!(e)).await;
      }
    }
  }
}

#[cfg(test)]
mod development_tests {
  use super::*;
  use crate::media::{Request, View};
  use tui::{Terminal, backend::TestBackend, layout::Rect};

  #[tokio::test]
  async fn development_bootstrap_and_normal_routes_are_populated_and_render() {
    let (tx, mut rx) = tokio::sync::mpsc::channel(64);
    let mut initial = App::new(tx, crate::user_config::UserConfig::new());
    initial.development = true;
    let app = Arc::new(Mutex::new(initial));
    let mut network = Network::new(Spotify::development(), ClientConfig::new(), &app);
    network.populate_development().await;
    while let Ok(event) = rx.try_recv() {
      network.handle_network_event(event).await;
    }
    {
      let app = app.lock().await;
      assert!(app.api_error.is_empty(), "{}", app.api_error);
      assert_eq!(app.user.as_ref().unwrap().id, "demo");
      assert!(!app.playlists.as_ref().unwrap().items.is_empty());
      assert!(!app.track_table.tracks.is_empty());
      assert!(app.library.saved_albums.get_results(None).is_some());
      assert!(app.library.saved_artists.get_results(None).is_some());
      assert!(app.library.saved_shows.get_results(None).is_some());
      for view in [View::Queue, View::Episodes, View::Audiobooks] {
        assert!(!app.media.page(view).entries.is_empty());
      }
      assert_eq!(app.get_current_route().id, RouteId::TrackTable);
    }
    let artist = network
      .spotify
      .current_user_followed_artists(1, None)
      .await
      .unwrap()
      .artists
      .items
      .remove(0);
    let album = network
      .spotify
      .current_user_saved_albums(1, None)
      .await
      .unwrap()
      .items
      .remove(0)
      .album;
    let show = network
      .spotify
      .get_saved_show(1, None)
      .await
      .unwrap()
      .items
      .remove(0)
      .show;
    let book = network
      .spotify
      .saved_audiobooks(1, 0)
      .await
      .unwrap()
      .items
      .remove(0);
    for (event, route) in [
      (
        IoEvent::GetSearchResults("northbound".into()),
        RouteId::Search,
      ),
      (IoEvent::GetArtist(artist.id, artist.name), RouteId::Artist),
      (IoEvent::GetAlbum(album.id), RouteId::AlbumTracks),
      (IoEvent::GetShow(show.id), RouteId::PodcastEpisodes),
      (
        IoEvent::LoadMedia(Request::chapters(book.id, book.name)),
        RouteId::Media(View::Chapters),
      ),
      (
        IoEvent::LoadMedia(Request::search("observatory".into())),
        RouteId::Media(View::BookSearch),
      ),
    ] {
      network.handle_network_event(event).await;
      while let Ok(event) = rx.try_recv() {
        network.handle_network_event(event).await;
      }
      let mut app = app.lock().await;
      assert!(app.api_error.is_empty(), "{}", app.api_error);
      app.push_navigation_stack(route, ActiveBlock::Empty);
      for (width, height) in [(1, 1), (20, 6), (80, 24), (180, 50)] {
        app.size = Rect::new(0, 0, width, height);
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
          .draw(|frame| crate::ui::draw_main_layout(frame, &app))
          .unwrap();
      }
    }
    network
      .handle_network_event(IoEvent::TransferPlaybackToDevice("demoSpeaker".into()))
      .await;
    assert_eq!(
      network.client_config.device_id.as_deref(),
      Some("demoSpeaker")
    );
    assert!(app.lock().await.api_error.is_empty());
  }
}
