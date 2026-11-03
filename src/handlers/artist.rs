use super::common_key_events;
use crate::{
  app::{ActiveBlock, App, TrackTableContext},
  event::Key,
};

pub fn handler(key: Key, app: &mut App) {
  if common_key_events::left_event(key) {
    common_key_events::handle_left_event(app);
    return;
  }
  if key == Key::Char('w') {
    app.current_user_saved_album_add(ActiveBlock::ArtistBlock);
    return;
  }
  if key == Key::Char('D') {
    app.current_user_saved_album_delete(ActiveBlock::ArtistBlock);
    return;
  }
  let Some(artist) = &mut app.artist else {
    return;
  };
  if common_key_events::down_event(key) {
    artist.selected_album_index = common_key_events::on_down_press_handler(
      &artist.albums.items,
      Some(artist.selected_album_index),
    );
  } else if common_key_events::up_event(key) {
    artist.selected_album_index = common_key_events::on_up_press_handler(
      &artist.albums.items,
      Some(artist.selected_album_index),
    );
  } else if common_key_events::high_event(key) {
    artist.selected_album_index = 0;
  } else if common_key_events::middle_event(key) {
    artist.selected_album_index = common_key_events::on_middle_press_handler(&artist.albums.items);
  } else if common_key_events::low_event(key) {
    artist.selected_album_index = common_key_events::on_low_press_handler(&artist.albums.items);
  } else if key == Key::Enter
    && let Some(album) = artist
      .albums
      .items
      .get(artist.selected_album_index)
      .cloned()
  {
    app.track_table.context = Some(TrackTableContext::AlbumSearch);
    app.dispatch(crate::network::IoEvent::GetAlbumTracks(Box::new(album)));
  }
}
