mod client;
mod demo;
pub mod model;
pub use client::Spotify;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RepeatState {
  #[default]
  Off,
  Track,
  Context,
}
#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Type {
  Album,
  Artist,
  Playlist,
  #[default]
  #[serde(other)]
  Other,
}

#[cfg(test)]
pub(crate) mod test_support;
