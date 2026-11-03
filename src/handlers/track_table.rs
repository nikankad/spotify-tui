use super::common_key_events;
use crate::{
  app::{App, TrackTableContext},
  event::Key,
  network::IoEvent,
};

pub fn handler(key: Key, app: &mut App) {
  match key {
    k if common_key_events::left_event(k) => common_key_events::handle_left_event(app),
    k if common_key_events::down_event(k) => {
      app.track_table.selected_index = common_key_events::on_down_press_handler(
        &app.track_table.tracks,
        Some(app.track_table.selected_index),
      );
    }
    k if common_key_events::up_event(k) => {
      app.track_table.selected_index = common_key_events::on_up_press_handler(
        &app.track_table.tracks,
        Some(app.track_table.selected_index),
      );
    }
    k if common_key_events::high_event(k) => app.track_table.selected_index = 0,
    k if common_key_events::middle_event(k) => {
      app.track_table.selected_index =
        common_key_events::on_middle_press_handler(&app.track_table.tracks);
    }
    k if common_key_events::low_event(k) => {
      app.track_table.selected_index =
        common_key_events::on_low_press_handler(&app.track_table.tracks);
    }
    Key::Enter => play(app, false),
    Key::Char('S') => play(app, true),
    Key::Char('s') => {
      if let Some(id) = app
        .track_table
        .tracks
        .get(app.track_table.selected_index)
        .and_then(|track| track.id.clone())
      {
        app.dispatch(IoEvent::ToggleSaveTrack(id));
      }
    }
    k if k == app.user_config.keys.next_page => change_page(app, true),
    k if k == app.user_config.keys.previous_page => change_page(app, false),
    k if k == app.user_config.keys.jump_to_end => jump(app, true),
    k if k == app.user_config.keys.jump_to_start => jump(app, false),
    k if k == app.user_config.keys.add_item_to_queue => {
      if let Some(track) = app.track_table.tracks.get(app.track_table.selected_index) {
        app.dispatch(IoEvent::AddItemToQueue(track.uri.clone()));
      }
    }
    _ => {}
  }
}

fn is_playlist(app: &App) -> bool {
  matches!(
    app.track_table.context,
    Some(TrackTableContext::MyPlaylists | TrackTableContext::PlaylistSearch)
  )
}

fn play(app: &mut App, random: bool) {
  if is_playlist(app) {
    let Some(uri) = app.playlist_uri.clone() else {
      return;
    };
    let position = if random {
      let Some(total) = app
        .playlist_tracks
        .as_ref()
        .map(|page| page.total)
        .filter(|&total| total > 0)
      else {
        return;
      };
      Some(fastrand::u32(0..total) as usize)
    } else {
      app.playlist_track_position(app.track_table.selected_index)
    };
    if let Some(position) = position {
      app.dispatch(IoEvent::StartPlayback(Some(uri), None, Some(position)));
    }
  } else if app.track_table.context == Some(TrackTableContext::SavedTracks) {
    if app.track_table.tracks.is_empty() {
      return;
    }
    let selected = if random {
      fastrand::usize(0..app.track_table.tracks.len())
    } else {
      app.track_table.selected_index
    };
    if selected < app.track_table.tracks.len() {
      let uris = app
        .track_table
        .tracks
        .iter()
        .map(|track| track.uri.clone())
        .collect();
      app.dispatch(IoEvent::StartPlayback(None, Some(uris), Some(selected)));
    }
  }
}

fn change_page(app: &mut App, next: bool) {
  if is_playlist(app) {
    let (Some(uri), Some(page)) = (app.playlist_uri.clone(), app.playlist_tracks.as_ref()) else {
      return;
    };
    let offset = if next {
      let offset = page.offset.saturating_add(page.limit.max(1));
      if offset >= page.total {
        return;
      }
      offset
    } else {
      page.offset.saturating_sub(page.limit.max(1))
    };
    app.playlist_offset = offset;
    app.dispatch(IoEvent::GetPlaylistTracks(uri, offset));
  } else if app.track_table.context == Some(TrackTableContext::SavedTracks) {
    if next {
      app.get_current_user_saved_tracks_next();
    } else {
      app.get_current_user_saved_tracks_previous();
    }
  }
}

fn jump(app: &mut App, end: bool) {
  if !is_playlist(app) {
    return;
  }
  let (Some(uri), Some(page)) = (app.playlist_uri.clone(), app.playlist_tracks.as_ref()) else {
    return;
  };
  let limit = page.limit.max(1);
  let offset = if end {
    page.total.saturating_sub(1) / limit * limit
  } else {
    0
  };
  app.playlist_offset = offset;
  app.dispatch(IoEvent::GetPlaylistTracks(uri, offset));
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{
    app::TrackTable,
    spotify::model::{Page, PlaylistTrack},
  };
  use serde_json::json;

  #[test]
  fn playback_keeps_original_position_after_unavailable_items() {
    let (tx, mut rx) = tokio::sync::mpsc::channel(64);
    let mut app = App::new(tx, crate::user_config::UserConfig::new());
    let track: crate::spotify::model::FullTrack =
      serde_json::from_value(json!({"type":"track","id":"song","uri":"spotify:track:song"}))
        .unwrap();
    app.playlist_tracks = Some(Page {
      offset: 20,
      items: vec![
        PlaylistTrack::default(),
        PlaylistTrack {
          track: Some(track.clone()),
        },
      ],
      ..Default::default()
    });
    app.playlist_uri = Some("spotify:playlist:playlist".into());
    app.track_table = TrackTable {
      context: Some(TrackTableContext::PlaylistSearch),
      tracks: vec![track],
      selected_index: 0,
    };
    handler(Key::Enter, &mut app);
    assert!(
      matches!(rx.try_recv().unwrap(), IoEvent::StartPlayback(Some(uri), None, Some(21)) if uri == "spotify:playlist:playlist")
    );
  }

  #[test]
  fn empty_lists_do_not_start_random_playback() {
    let (tx, mut rx) = tokio::sync::mpsc::channel(64);
    let mut app = App::new(tx, crate::user_config::UserConfig::new());
    app.track_table.context = Some(TrackTableContext::SavedTracks);
    handler(Key::Char('S'), &mut app);
    assert!(rx.try_recv().is_err());
  }
}
