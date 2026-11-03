//! Queue, saved episodes and audiobook browsing share paging and item actions.
use crate::{
  app::{ActiveBlock, App, RouteId},
  event::Key,
  network::IoEvent,
  spotify::{Spotify, model::*},
};
use anyhow::{Result, bail};
use tui::{
  Frame,
  layout::{Constraint, Layout, Rect},
  style::{Modifier, Style},
  text::Line,
  widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
  Queue,
  Episodes,
  Audiobooks,
  Chapters,
  BookSearch,
}

#[derive(Clone, Debug)]
pub struct Request {
  pub view: View,
  pub offset: u32,
  pub book_id: Option<String>,
  pub query: Option<String>,
  pub title: Option<String>,
}
impl Request {
  pub fn new(view: View) -> Self {
    Self {
      view,
      offset: 0,
      book_id: None,
      query: None,
      title: None,
    }
  }
  pub fn chapters(id: String, title: String) -> Self {
    Self {
      book_id: Some(id),
      title: Some(title),
      ..Self::new(View::Chapters)
    }
  }
  pub fn search(query: String) -> Self {
    Self {
      query: Some(query),
      ..Self::new(View::BookSearch)
    }
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
  Track,
  Episode,
  Audiobook,
  Chapter,
}
impl Kind {
  fn as_str(self) -> &'static str {
    match self {
      Self::Track => "track",
      Self::Episode => "episode",
      Self::Audiobook => "audiobook",
      Self::Chapter => "chapter",
    }
  }
}

#[derive(Clone, Debug)]
pub struct Entry {
  pub kind: Kind,
  pub id: String,
  pub uri: String,
  pub title: String,
  pub creator: String,
  pub duration_ms: Option<u32>,
  pub resume: Option<ResumePoint>,
  pub saved: Option<bool>,
}
impl Entry {
  fn new(kind: Kind, id: String, uri: String, title: String, creator: String) -> Self {
    let uri = if uri.is_empty() && !id.is_empty() {
      format!("spotify:{}:{id}", kind.as_str())
    } else {
      uri
    };
    Self {
      kind,
      id,
      uri,
      title,
      creator,
      duration_ms: None,
      resume: None,
      saved: None,
    }
  }
  pub fn playing(item: PlayingItem) -> Self {
    match item {
      PlayingItem::Track(track) => {
        let mut entry = Self::new(
          Kind::Track,
          track.id.unwrap_or_default(),
          track.uri,
          track.name,
          track
            .artists
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        );
        entry.duration_ms = Some(track.duration_ms);
        entry
      }
      PlayingItem::Episode(episode) => {
        let mut entry = Self::new(
          Kind::Episode,
          episode.id,
          episode.uri,
          episode.name,
          episode.show.name,
        );
        entry.duration_ms = Some(episode.duration_ms);
        entry.resume = episode.resume_point;
        entry
      }
    }
  }
  fn book(book: Audiobook) -> Self {
    Self::new(
      Kind::Audiobook,
      book.id,
      book.uri,
      book.name,
      book
        .authors
        .iter()
        .map(|author| author.name.as_str())
        .collect::<Vec<_>>()
        .join(", "),
    )
  }
  fn chapter(chapter: Chapter) -> Self {
    let mut entry = Self::new(
      Kind::Chapter,
      chapter.id,
      chapter.uri,
      format!("{}. {}", chapter.chapter_number, chapter.name),
      String::new(),
    );
    entry.duration_ms = Some(chapter.duration_ms);
    entry.resume = chapter.resume_point;
    entry
  }
  pub fn resume_ms(&self) -> u32 {
    self
      .resume
      .as_ref()
      .filter(|point| !point.fully_played)
      .map(|point| {
        point.resume_position_ms.min(
          self
            .duration_ms
            .filter(|&duration| duration > 0)
            .unwrap_or(u32::MAX),
        )
      })
      .unwrap_or(0)
  }
  pub fn web_url(&self) -> Option<String> {
    if self.id.is_empty() || !self.id.bytes().all(|b| b.is_ascii_alphanumeric()) {
      return None;
    }
    Some(format!(
      "https://open.spotify.com/{}/{}",
      self.kind.as_str(),
      self.id
    ))
  }
  pub fn display(&self) -> String {
    let mut text = self.title.clone();
    if !self.creator.is_empty() {
      text.push_str(&format!(" — {}", self.creator));
    }
    if let Some(point) = &self.resume {
      if point.fully_played {
        text.push_str(" [finished]");
      } else if point.resume_position_ms > 0 {
        text.push_str(&format!(
          " [resume {}]",
          crate::ui::util::millis_to_minutes(point.resume_position_ms.into())
        ));
      }
    }
    text
  }
}

#[derive(Clone, Debug, Default)]
pub struct MediaPage {
  pub entries: Vec<Entry>,
  pub selected: usize,
  pub total: u32,
  pub limit: u32,
  pub has_next: bool,
  pub title: String,
  pub now_playing: Option<String>,
  pub request: Option<Request>,
}
impl MediaPage {
  pub async fn load(spotify: &Spotify, request: Request, limit: u32) -> Result<Self> {
    let mut result = Self {
      request: Some(request.clone()),
      ..Self::default()
    };
    match request.view {
      View::Queue => {
        let queue = spotify.queue().await?;
        result.now_playing = queue
          .currently_playing
          .map(Entry::playing)
          .map(|entry| entry.display());
        result.entries = queue.queue.into_iter().map(Entry::playing).collect();
        result.title = "Queue".into();
        result.total = result.entries.len() as u32;
      }
      View::Episodes => {
        let page = spotify.saved_episodes(limit, request.offset).await?;
        result.total = page.total;
        result.limit = page.limit;
        result.has_next = page.next.is_some();
        result.entries = page
          .items
          .into_iter()
          .map(|item| {
            let mut entry = Entry::playing(PlayingItem::Episode(item.episode));
            entry.saved = Some(true);
            entry
          })
          .collect();
        result.title = "Saved episodes".into();
      }
      View::Audiobooks | View::BookSearch => {
        let page = if request.view == View::Audiobooks {
          spotify.saved_audiobooks(limit, request.offset).await?
        } else {
          let query = request.query.as_deref().unwrap_or_default().trim();
          if query.is_empty() {
            bail!("Enter an audiobook search phrase");
          }
          spotify
            .search_audiobooks(query, limit, request.offset)
            .await?
        };
        result.total = page.total;
        result.limit = page.limit;
        result.has_next = page.next.is_some();
        result.entries = page.items.into_iter().map(Entry::book).collect();
        if request.view == View::Audiobooks {
          for entry in &mut result.entries {
            entry.saved = Some(true);
          }
        }
        result.title = if request.view == View::Audiobooks {
          "Audiobooks".into()
        } else {
          format!(
            "Audiobooks: {}",
            request.query.as_deref().unwrap_or_default()
          )
        };
      }
      View::Chapters => {
        let Some(id) = request.book_id.as_deref() else {
          bail!("No audiobook selected");
        };
        let page = spotify
          .audiobook_chapters(id, limit, request.offset)
          .await?;
        result.total = page.total;
        result.limit = page.limit;
        result.has_next = page.next.is_some();
        result.entries = page.items.into_iter().map(Entry::chapter).collect();
        result.title = request
          .title
          .clone()
          .unwrap_or_else(|| "Audiobook chapters".into());
      }
    }
    Ok(result)
  }
  pub fn page_request(&self, next: bool) -> Option<Request> {
    let mut request = self.request.clone()?;
    if request.view == View::Queue || (next && !self.has_next) {
      return None;
    }
    if next {
      request.offset = request.offset.saturating_add(self.limit.max(1));
    } else {
      if request.offset == 0 {
        return None;
      }
      request.offset = request.offset.saturating_sub(self.limit.max(1));
    }
    Some(request)
  }
}

#[derive(Default)]
pub struct MediaLibrary {
  queue: MediaPage,
  episodes: MediaPage,
  audiobooks: MediaPage,
  chapters: MediaPage,
  search: MediaPage,
}
impl MediaLibrary {
  pub fn page(&self, view: View) -> &MediaPage {
    match view {
      View::Queue => &self.queue,
      View::Episodes => &self.episodes,
      View::Audiobooks => &self.audiobooks,
      View::Chapters => &self.chapters,
      View::BookSearch => &self.search,
    }
  }
  pub fn page_mut(&mut self, view: View) -> &mut MediaPage {
    match view {
      View::Queue => &mut self.queue,
      View::Episodes => &mut self.episodes,
      View::Audiobooks => &mut self.audiobooks,
      View::Chapters => &mut self.chapters,
      View::BookSearch => &mut self.search,
    }
  }
  pub fn mark_saved(&mut self, uri: &str, saved: bool) {
    for page in [
      &mut self.queue,
      &mut self.episodes,
      &mut self.audiobooks,
      &mut self.search,
    ] {
      for entry in &mut page.entries {
        if entry.uri == uri {
          entry.saved = Some(saved);
        }
      }
    }
    if !saved {
      for page in [&mut self.episodes, &mut self.audiobooks] {
        let before = page.entries.len();
        page.entries.retain(|entry| entry.uri != uri);
        page.total = page
          .total
          .saturating_sub((before - page.entries.len()) as u32);
        page.selected = page.selected.min(page.entries.len().saturating_sub(1));
      }
    }
  }
}

pub fn handler(key: Key, app: &mut App) {
  use crate::handlers::common_key_events as keys;
  let RouteId::Media(view) = app.get_current_route().id else {
    return;
  };
  if keys::left_event(key) {
    keys::handle_left_event(app);
    return;
  }
  let page = app.media.page_mut(view);
  if keys::down_event(key) {
    page.selected = if page.entries.is_empty() {
      0
    } else {
      (page.selected + 1) % page.entries.len()
    };
    return;
  }
  if keys::up_event(key) {
    page.selected = if page.selected == 0 {
      page.entries.len().saturating_sub(1)
    } else {
      page.selected - 1
    };
    return;
  }
  if keys::high_event(key) {
    page.selected = 0;
    return;
  }
  if keys::low_event(key) {
    page.selected = page.entries.len().saturating_sub(1);
    return;
  }
  if keys::middle_event(key) {
    page.selected = page.entries.len().saturating_sub(1) / 2;
    return;
  }
  let request = if key == Key::F5 {
    page.request.clone()
  } else if key == app.user_config.keys.next_page {
    page.page_request(true)
  } else if key == app.user_config.keys.previous_page {
    page.page_request(false)
  } else {
    None
  };
  if let Some(request) = request {
    app.dispatch(IoEvent::LoadMedia(request));
    return;
  }
  let Some(entry) = page.entries.get(page.selected).cloned() else {
    return;
  };
  match key {
    Key::Enter => match entry.kind {
      Kind::Audiobook => app.dispatch(IoEvent::LoadMedia(Request::chapters(entry.id, entry.title))),
      Kind::Chapter => open_entry(app, &entry),
      _ => app.dispatch(IoEvent::PlayMediaItem(entry.uri.clone(), entry.resume_ms())),
    },
    Key::Char('s') if entry.kind != Kind::Chapter => {
      app.dispatch(IoEvent::ToggleSavedItem(entry.uri))
    }
    Key::Char('O') => open_entry(app, &entry),
    k if k == app.user_config.keys.add_item_to_queue
      && matches!(entry.kind, Kind::Track | Kind::Episode) =>
    {
      app.dispatch(IoEvent::AddItemToQueue(entry.uri));
    }
    _ => {}
  }
}
fn open_entry(app: &mut App, entry: &Entry) {
  if app.development {
    app.handle_error(anyhow::anyhow!(
      "Development mode: sample books and chapters have no Spotify playback link"
    ));
    return;
  }
  if let Some(url) = entry.web_url()
    && let Err(error) = webbrowser::open(&url)
  {
    app.handle_error(error.into());
  }
}

pub fn draw(f: &mut Frame<'_>, app: &App, area: Rect, view: View) {
  let page = app.media.page(view);
  let chunks = Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).split(area);
  let mut title = format!("{} ({})", page.title, page.total);
  if let Some(now) = &page.now_playing {
    title.push_str(&format!(" | Now: {now}"));
  }
  let items: Vec<ListItem> = page
    .entries
    .iter()
    .enumerate()
    .map(|(index, entry)| {
      let saved = if entry.saved == Some(true) {
        "[saved] "
      } else {
        ""
      };
      ListItem::new(Line::raw(format!(
        "{}. {saved}{}",
        page.request.as_ref().map_or(0, |r| r.offset) as usize + index + 1,
        entry.display()
      )))
    })
    .collect();
  let color = crate::ui::util::get_color(
    (
      app.get_current_route().active_block == ActiveBlock::Media,
      app.get_current_route().hovered_block == ActiveBlock::Media,
    ),
    app.user_config.theme,
  );
  let list = List::new(items)
    .block(
      Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(color),
    )
    .style(Style::default().fg(app.user_config.theme.text))
    .highlight_style(color.add_modifier(Modifier::BOLD));
  let mut state = ListState::default();
  if !page.entries.is_empty() {
    state.select(Some(page.selected));
  }
  f.render_stateful_widget(list, chunks[0], &mut state);
  let hint = if page.entries.is_empty() {
    "No items. F5: refresh"
  } else {
    match view {
      View::Chapters => "Enter / O: open in Spotify | F5: refresh",
      View::Audiobooks | View::BookSearch => {
        "Enter: chapters | s: save/remove | O: open in Spotify | F5: refresh"
      }
      View::Queue => "Enter: play item | s: save/remove | z: queue again | F5: refresh",
      View::Episodes => "Enter: resume | s: remove saved episode | z: queue | F5: refresh",
    }
  };
  f.render_widget(
    Paragraph::new(hint).style(Style::default().fg(app.user_config.theme.hint)),
    chunks[1],
  );
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::spotify::test_support::server;
  use serde_json::json;

  fn episode() -> Entry {
    Entry::playing(PlayingItem::Episode(
      serde_json::from_value(json!({
        "id":"episode","name":"Episode","uri":"spotify:episode:episode","duration_ms":60000,
        "resume_point":{"fully_played":false,"resume_position_ms":15000}
      }))
      .unwrap(),
    ))
  }
  fn app() -> (App, tokio::sync::mpsc::Receiver<IoEvent>) {
    let (tx, rx) = tokio::sync::mpsc::channel(64);
    (App::new(tx, crate::user_config::UserConfig::new()), rx)
  }

  #[test]
  fn saved_episode_enter_resumes_and_save_uses_episode_identity() {
    let (mut app, mut rx) = app();
    app.push_navigation_stack(RouteId::Media(View::Episodes), ActiveBlock::Media);
    app.media.page_mut(View::Episodes).entries.push(episode());
    handler(Key::Enter, &mut app);
    assert!(
      matches!(rx.try_recv().unwrap(), IoEvent::PlayMediaItem(uri, 15000) if uri == "spotify:episode:episode")
    );
    handler(Key::Char('s'), &mut app);
    assert!(
      matches!(rx.try_recv().unwrap(), IoEvent::ToggleSavedItem(uri) if uri == "spotify:episode:episode")
    );
    app.media.page_mut(View::Episodes).entries[0]
      .resume
      .as_mut()
      .unwrap()
      .fully_played = true;
    handler(Key::Enter, &mut app);
    assert!(matches!(
      rx.try_recv().unwrap(),
      IoEvent::PlayMediaItem(_, 0)
    ));
  }

  #[test]
  fn opening_an_audiobook_requests_chapters_without_starting_playback() {
    let (mut app, mut rx) = app();
    app.push_navigation_stack(RouteId::Media(View::Audiobooks), ActiveBlock::Media);
    app
      .media
      .page_mut(View::Audiobooks)
      .entries
      .push(Entry::book(
        serde_json::from_value(json!({"id":"book","name":"Book","uri":"spotify:audiobook:book"}))
          .unwrap(),
      ));
    handler(Key::Enter, &mut app);
    assert!(
      matches!(rx.try_recv().unwrap(), IoEvent::LoadMedia(Request { view: View::Chapters, book_id: Some(id), .. }) if id == "book")
    );
    assert!(rx.try_recv().is_err());
    let chapter =
      Entry::chapter(serde_json::from_value(json!({"id":"chapter","name":"Chapter"})).unwrap());
    assert_eq!(
      chapter.web_url().as_deref(),
      Some("https://open.spotify.com/chapter/chapter")
    );
  }

  #[test]
  fn empty_views_and_last_page_navigation_are_safe() {
    let (mut app, mut rx) = app();
    app.push_navigation_stack(RouteId::Media(View::Episodes), ActiveBlock::Media);
    for key in [
      Key::Enter,
      Key::Up,
      Key::Down,
      Key::Char('H'),
      Key::Char('M'),
      Key::Char('L'),
      Key::Char('s'),
    ] {
      handler(key, &mut app);
    }
    assert!(rx.try_recv().is_err());
    let page = MediaPage {
      limit: 20,
      request: Some(Request {
        offset: 20,
        ..Request::new(View::Episodes)
      }),
      ..Default::default()
    };
    assert!(page.page_request(true).is_none());
    assert_eq!(page.page_request(false).unwrap().offset, 0);
  }

  #[test]
  fn removing_saved_items_updates_every_view_and_clamps_selection() {
    let mut library = MediaLibrary::default();
    library.episodes.entries.push(episode());
    library.episodes.total = 1;
    library.queue.entries.push(episode());
    library.mark_saved("spotify:episode:episode", false);
    assert!(library.episodes.entries.is_empty());
    assert_eq!(library.episodes.total, 0);
    assert_eq!(library.episodes.selected, 0);
    assert_eq!(library.queue.entries[0].saved, Some(false));
  }

  #[tokio::test]
  async fn loads_queue_episodes_audiobooks_and_chapters_with_pagination() {
    let (base, task) = server(vec![
      (200, "", json!({"currently_playing":{"type":"track","id":"now","name":"Current"},"queue":[null,{"type":"episode","id":"next","name":"Upcoming"}]}).to_string()),
      (200, "", json!({"items":[{"episode":{"id":"episode","name":"Saved","resume_point":{"fully_played":false,"resume_position_ms":45000}}}],"limit":20,"offset":20,"total":50,"next":"next"}).to_string()),
      (200, "", json!({"items":[{"id":"book","name":"Book","authors":[{"name":"Author"}]}],"limit":20,"total":1}).to_string()),
      (200, "", json!({"items":[{"id":"chapter","name":"Chapter","chapter_number":1}],"limit":20,"total":1}).to_string()),
      (200, "", json!({"audiobooks":{"items":[{"id":"book","name":"Found"}],"limit":10,"total":1}}).to_string()),
    ]).await;
    let dir = tempfile::tempdir().unwrap();
    let spotify = Spotify::for_test(base, dir.path().join("token"));
    let queue = MediaPage::load(&spotify, Request::new(View::Queue), 20)
      .await
      .unwrap();
    assert_eq!(queue.now_playing.as_deref(), Some("Current"));
    assert_eq!(queue.entries.len(), 1);
    assert_eq!(queue.entries[0].kind, Kind::Episode);
    let episodes = MediaPage::load(
      &spotify,
      Request {
        offset: 20,
        ..Request::new(View::Episodes)
      },
      20,
    )
    .await
    .unwrap();
    assert_eq!(episodes.entries[0].saved, Some(true));
    assert_eq!(episodes.page_request(true).unwrap().offset, 40);
    let books = MediaPage::load(&spotify, Request::new(View::Audiobooks), 20)
      .await
      .unwrap();
    assert_eq!(books.entries[0].creator, "Author");
    assert_eq!(books.entries[0].uri, "spotify:audiobook:book");
    let chapters = MediaPage::load(
      &spotify,
      Request::chapters("book".into(), "Book".into()),
      20,
    )
    .await
    .unwrap();
    assert_eq!(chapters.entries[0].kind, Kind::Chapter);
    let search = MediaPage::load(&spotify, Request::search("Dune".into()), 50)
      .await
      .unwrap();
    assert_eq!(search.entries[0].title, "Found");
    let requests = task.await.unwrap();
    assert!(requests[0].starts_with("GET /me/player/queue "));
    assert!(requests[1].starts_with("GET /me/episodes?limit=20&offset=20 "));
    assert!(requests[2].starts_with("GET /me/audiobooks?limit=20&offset=0 "));
    assert!(requests[3].starts_with("GET /audiobooks/book/chapters?limit=20&offset=0 "));
    assert!(requests[4].contains("type=audiobook&limit=10"));
  }

  #[test]
  fn all_new_views_render_with_empty_and_populated_results() {
    use tui::{Terminal, backend::TestBackend};
    let (mut app, _rx) = app();
    for (width, height) in [(1, 1), (30, 8), (100, 30)] {
      let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
      for view in [
        View::Queue,
        View::Episodes,
        View::Audiobooks,
        View::Chapters,
        View::BookSearch,
      ] {
        app.push_navigation_stack(RouteId::Media(view), ActiveBlock::Media);
        terminal
          .draw(|frame| draw(frame, &app, frame.area(), view))
          .unwrap();
        app.media.page_mut(view).entries.push(episode());
        terminal
          .draw(|frame| draw(frame, &app, frame.area(), view))
          .unwrap();
      }
    }
  }
}
