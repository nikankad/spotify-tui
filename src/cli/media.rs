use crate::{
  config::ClientConfig,
  media::{Entry, Kind, MediaPage, Request, View},
  spotify::Spotify,
};
use anyhow::{Result, bail};
use clap::{Arg, ArgAction, ArgGroup, ArgMatches, Command};

pub fn commands() -> Vec<Command> {
  let json = || {
    Arg::new("json")
      .long("json")
      .action(ArgAction::SetTrue)
      .help("Print a JSON snapshot")
  };
  let limit = || {
    Arg::new("limit")
      .long("limit")
      .value_parser(clap::value_parser!(u32).range(1..=50))
      .default_value("20")
  };
  let offset = || {
    Arg::new("offset")
      .long("offset")
      .value_parser(clap::value_parser!(u32))
      .default_value("0")
  };
  let item = |name, help| Arg::new(name).long(name).value_name("ID_OR_URI").help(help);
  vec![
    Command::new("queue")
      .about("Show the current track and upcoming tracks or episodes")
      .arg(json()),
    Command::new("episodes")
      .about("List, save, remove or resume podcast episodes")
      .arg(json())
      .arg(limit())
      .arg(offset())
      .arg(item("save", "Save an episode to Your Episodes"))
      .arg(item("remove", "Remove an episode from Your Episodes"))
      .arg(item(
        "resume",
        "Resume an episode at its saved listening position",
      ))
      .arg(item("play", "Play an episode from the beginning"))
      .group(ArgGroup::new("action").args(["save", "remove", "resume", "play"])),
    Command::new("audiobooks")
      .about("Browse saved audiobooks, search the catalogue or list chapters")
      .arg(json())
      .arg(limit())
      .arg(offset())
      .arg(
        Arg::new("search")
          .long("search")
          .value_name("QUERY")
          .help("Search for audiobooks (maximum 10 per page)"),
      )
      .arg(item("chapters", "List an audiobook's chapters"))
      .arg(item("save", "Save an audiobook to your library"))
      .arg(item("remove", "Remove an audiobook from your library"))
      .arg(item("open", "Open an audiobook in Spotify for playback"))
      .group(ArgGroup::new("action").args(["search", "chapters", "save", "remove", "open"])),
  ]
}

pub async fn handle(
  name: &str,
  matches: &ArgMatches,
  spotify: &Spotify,
  config: &ClientConfig,
) -> Result<String> {
  let kind = if name == "episodes" {
    "episode"
  } else {
    "audiobook"
  };
  for (action, save) in [("save", true), ("remove", false)] {
    if let Some(input) = value(matches, action) {
      let uri = item_uri(kind, input)?;
      spotify.set_saved_item(&uri, save).await?;
      return Ok(format!("{} {uri}", if save { "Saved" } else { "Removed" }));
    }
  }
  for action in ["resume", "play"] {
    if let Some(input) = value(matches, action) {
      let uri = item_uri("episode", input)?;
      let position = if action == "resume" {
        let episode = spotify.episode(&uri).await?;
        Entry::playing(crate::spotify::model::PlayingItem::Episode(episode)).resume_ms()
      } else {
        0
      };
      spotify
        .play_item(&uri, position, config.device_id.clone())
        .await?;
      return Ok(format!("Playing {uri} at {} ms", position));
    }
  }
  if let Some(input) = value(matches, "open") {
    let uri = item_uri("audiobook", input)?;
    let id = uri.rsplit(':').next().unwrap();
    let url = format!("https://open.spotify.com/audiobook/{id}");
    if spotify.is_development() {
      return Ok(format!("Development mode: sample audiobook link {url}"));
    }
    webbrowser::open(&url)?;
    return Ok(format!("Opened {url}"));
  }
  let mut request = match name {
    "queue" => Request::new(View::Queue),
    "episodes" => Request::new(View::Episodes),
    "audiobooks" => {
      if let Some(query) = value(matches, "search") {
        Request::search(query.into())
      } else if let Some(id) = value(matches, "chapters") {
        let uri = item_uri("audiobook", id)?;
        let book = spotify.audiobook(&uri).await?;
        Request::chapters(book.id, book.name)
      } else {
        Request::new(View::Audiobooks)
      }
    }
    _ => bail!("Unknown media command"),
  };
  request.offset = matches
    .try_get_one::<u32>("offset")
    .ok()
    .flatten()
    .copied()
    .unwrap_or(0);
  let limit = matches
    .try_get_one::<u32>("limit")
    .ok()
    .flatten()
    .copied()
    .unwrap_or(20);
  let page = MediaPage::load(spotify, request, limit).await?;
  if matches.get_flag("json") {
    return Ok(serde_json::to_string_pretty(&serde_json::json!({
      "title": page.title, "currently_playing": page.now_playing,
      "total": page.total, "offset": page.request.as_ref().map_or(0, |r| r.offset),
      "next_offset": page.page_request(true).map(|request| request.offset),
      "items": page.entries.iter().map(|entry| serde_json::json!({
        "type": match entry.kind { Kind::Track => "track", Kind::Episode => "episode", Kind::Audiobook => "audiobook", Kind::Chapter => "chapter" },
        "id": entry.id, "uri": entry.uri, "name": entry.title, "creator": entry.creator,
        "duration_ms": entry.duration_ms, "resume_position_ms": entry.resume_ms(),
        "fully_played": entry.resume.as_ref().map(|point| point.fully_played), "saved": entry.saved,
      })).collect::<Vec<_>>()
    }))?);
  }
  let mut lines = Vec::new();
  if let Some(now) = page.now_playing {
    lines.push(format!("Now: {now}"));
  }
  lines.extend(
    page
      .entries
      .iter()
      .map(|entry| format!("{}\t{}", entry.display(), entry.uri)),
  );
  if lines.is_empty() {
    lines.push("No items".into());
  }
  Ok(lines.join("\n"))
}
fn value<'a>(matches: &'a ArgMatches, key: &str) -> Option<&'a str> {
  matches
    .try_get_one::<String>(key)
    .ok()
    .flatten()
    .map(String::as_str)
}
fn item_uri(kind: &str, input: &str) -> Result<String> {
  let prefix = format!("spotify:{kind}:");
  let id = if let Some(id) = input.strip_prefix(&prefix) {
    id
  } else if input.contains(':') {
    bail!("Expected a Spotify {kind} ID or URI");
  } else {
    input
  };
  if id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric()) {
    bail!("Invalid Spotify {kind} ID");
  }
  Ok(format!("{prefix}{id}"))
}

#[cfg(test)]
mod tests {
  use super::*;
  #[tokio::test]
  async fn queue_json_reads_without_needing_a_playback_device() {
    let (url, task) = crate::spotify::test_support::server(vec![(
      200,
      "",
      r#"{"currently_playing":null,"queue":[{"type":"episode","id":"episode","name":"Next"}]}"#
        .into(),
    )])
    .await;
    let dir = tempfile::tempdir().unwrap();
    let spotify = Spotify::for_test(url, dir.path().join("token"));
    let matches = commands()
      .remove(0)
      .try_get_matches_from(["queue", "--json"])
      .unwrap();
    let output = handle("queue", &matches, &spotify, &ClientConfig::new())
      .await
      .unwrap();
    let json: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(json["items"][0]["type"], "episode");
    assert_eq!(json["items"][0]["uri"], "spotify:episode:episode");
    assert_eq!(task.await.unwrap().len(), 1);
  }

  #[tokio::test]
  async fn episode_resume_reads_progress_before_starting_playback() {
    let (url, task) = crate::spotify::test_support::server(vec![
      (200, "", r#"{"id":"episode","uri":"spotify:episode:episode","duration_ms":60000,"resume_point":{"fully_played":false,"resume_position_ms":15000}}"#.into()),
      (204, "", "".into()),
    ]).await;
    let dir = tempfile::tempdir().unwrap();
    let spotify = Spotify::for_test(url, dir.path().join("token"));
    let matches = commands()
      .remove(1)
      .try_get_matches_from(["episodes", "--resume", "episode"])
      .unwrap();
    handle("episodes", &matches, &spotify, &ClientConfig::new())
      .await
      .unwrap();
    let requests = task.await.unwrap();
    assert!(requests[0].starts_with("GET /episodes/episode "));
    let body: serde_json::Value =
      serde_json::from_str(requests[1].split("\r\n\r\n").nth(1).unwrap()).unwrap();
    assert_eq!(body["position_ms"], 15000);
  }

  #[test]
  fn validates_commands_and_mutually_exclusive_actions() {
    for command in commands() {
      command.debug_assert();
    }
    let episodes = || commands().remove(1);
    assert!(
      episodes()
        .try_get_matches_from(["episodes", "--save", "id", "--remove", "id"])
        .is_err()
    );
    assert!(
      episodes()
        .try_get_matches_from(["episodes", "--resume", "spotify:episode:id"])
        .is_ok()
    );
    assert_eq!(item_uri("episode", "id").unwrap(), "spotify:episode:id");
    assert!(item_uri("episode", "spotify:track:id").is_err());
    assert!(item_uri("audiobook", "../me").is_err());
  }
}
