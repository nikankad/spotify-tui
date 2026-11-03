//! Keep a small cache of page contents and lightweight navigation bookmarks.
use crate::spotify::model::{CursorBasedPage, Page};
use std::collections::BTreeMap;

pub const CACHED_PAGES: usize = 8;
const BOOKMARKS: usize = 2048;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Bookmark {
  Offset(u32),
  After(Option<String>),
}
pub trait PageIdentity {
  fn bookmark(&self) -> Bookmark;
}
impl<T> PageIdentity for Page<T> {
  fn bookmark(&self) -> Bookmark {
    Bookmark::Offset(self.offset)
  }
}
impl<T> PageIdentity for CursorBasedPage<T> {
  fn bookmark(&self) -> Bookmark {
    Bookmark::After(self.request_after.clone())
  }
}

#[derive(Clone)]
pub struct ScrollableResultPages<T> {
  pub index: usize,
  pages: BTreeMap<usize, T>,
  bookmarks: Vec<Bookmark>,
}
impl<T: PageIdentity> ScrollableResultPages<T> {
  pub fn new() -> Self {
    Self {
      index: 0,
      pages: BTreeMap::new(),
      bookmarks: Vec::new(),
    }
  }
  pub fn get_results(&self, at_index: Option<usize>) -> Option<&T> {
    self.pages.get(&at_index.unwrap_or(self.index))
  }
  pub fn get_mut_results(&mut self, at_index: Option<usize>) -> Option<&mut T> {
    self.pages.get_mut(&at_index.unwrap_or(self.index))
  }
  pub fn previous_bookmark(&self) -> Option<&Bookmark> {
    self
      .index
      .checked_sub(1)
      .and_then(|index| self.bookmarks.get(index))
  }
  pub fn add_pages(&mut self, page: T) {
    let bookmark = page.bookmark();
    let index = if let Some(index) = self.bookmarks.iter().position(|saved| saved == &bookmark) {
      index
    } else {
      if self.bookmarks.len() == BOOKMARKS {
        self.bookmarks.remove(0);
        self.pages = std::mem::take(&mut self.pages)
          .into_iter()
          .filter_map(|(index, page)| index.checked_sub(1).map(|index| (index, page)))
          .collect();
      }
      let index = self.bookmarks.len();
      self.bookmarks.push(bookmark);
      index
    };
    self.index = index;
    self.pages.insert(index, page);
    if self.pages.len() > CACHED_PAGES
      && let Some(farthest) = self
        .pages
        .keys()
        .copied()
        .max_by_key(|&key| key.abs_diff(index))
    {
      self.pages.remove(&farthest);
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  #[test]
  fn bookmark_history_is_bounded_too() {
    let mut cache = ScrollableResultPages::new();
    for offset in 0..(BOOKMARKS as u32 + 30) {
      cache.add_pages(Page::<String> {
        offset,
        ..Default::default()
      });
    }
    assert_eq!(cache.bookmarks.len(), BOOKMARKS);
    assert_eq!(cache.pages.len(), CACHED_PAGES);
    assert_eq!(
      cache.get_results(None).unwrap().offset,
      BOOKMARKS as u32 + 29
    );
  }

  #[test]
  fn bounds_payloads_and_replaces_revisited_pages() {
    let mut cache = ScrollableResultPages::new();
    for offset in 0..100u32 {
      cache.add_pages(Page::<String> {
        offset,
        items: vec!["payload".repeat(1024)],
        ..Default::default()
      });
    }
    assert_eq!(cache.pages.len(), CACHED_PAGES);
    assert_eq!(cache.previous_bookmark(), Some(&Bookmark::Offset(98)));
    let count = cache.bookmarks.len();
    cache.add_pages(Page::<String> {
      offset: 0,
      items: vec!["refetched".into()],
      ..Default::default()
    });
    assert_eq!(cache.bookmarks.len(), count);
    assert_eq!(cache.index, 0);
    assert_eq!(cache.get_results(None).unwrap().items, ["refetched"]);
  }
  #[test]
  fn retains_cursor_bookmarks_after_payload_eviction() {
    let mut cache = ScrollableResultPages::new();
    for index in 0..20 {
      cache.add_pages(CursorBasedPage::<String> {
        request_after: Some(index.to_string()),
        ..Default::default()
      });
    }
    cache.add_pages(CursorBasedPage::<String> {
      request_after: Some("1".into()),
      ..Default::default()
    });
    assert_eq!(
      cache.previous_bookmark(),
      Some(&Bookmark::After(Some("0".into())))
    );
    assert_eq!(cache.pages.len(), CACHED_PAGES);
  }
}
