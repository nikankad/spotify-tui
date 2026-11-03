use super::{
  super::app::{ActiveBlock, App},
  common_key_events,
};
use crate::event::Key;
use crate::network::IoEvent;
use crate::spotify::model::*;

pub fn handler(key: Key, app: &mut App) {
  match key {
    k if common_key_events::up_event(k) => {
      app.set_current_route_state(Some(ActiveBlock::Empty), Some(ActiveBlock::MyPlaylists));
    }
    Key::Char('s') => {
      if let Some(CurrentlyPlaybackContext {
        item: Some(item), ..
      }) = app.current_playback_context.to_owned()
      {
        match item {
          PlayingItem::Track(track) => {
            if let Some(track_id) = track.id {
              app.dispatch(IoEvent::ToggleSaveTrack(track_id));
            }
          }
          PlayingItem::Episode(episode) => {
            app.dispatch(IoEvent::ToggleSavedItem(episode.uri));
          }
        };
      };
    }
    _ => {}
  };
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn saving_a_podcast_uses_its_episode_uri() {
    let (tx, mut rx) = tokio::sync::mpsc::channel(64);
    let mut app = App::new(tx, crate::user_config::UserConfig::new());
    app.current_playback_context = Some(
      serde_json::from_value(serde_json::json!({
        "item":{"type":"episode","id":"episode","uri":"spotify:episode:episode"}
      }))
      .unwrap(),
    );
    handler(Key::Char('s'), &mut app);
    assert!(
      matches!(rx.try_recv().unwrap(), IoEvent::ToggleSavedItem(uri) if uri == "spotify:episode:episode")
    );
  }

  #[test]
  fn on_left_press() {
    let mut app = App::default();
    app.set_current_route_state(Some(ActiveBlock::PlayBar), Some(ActiveBlock::PlayBar));

    handler(Key::Up, &mut app);
    let current_route = app.get_current_route();
    assert_eq!(current_route.active_block, ActiveBlock::Empty);
    assert_eq!(current_route.hovered_block, ActiveBlock::MyPlaylists);
  }
}
