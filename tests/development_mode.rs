//! Exercise the actual executable with deliberately unusable saved credentials.
use std::{
  fs,
  process::{Command, Output},
};

fn run(root: &std::path::Path, args: &[&str]) -> Output {
  Command::new(env!("CARGO_BIN_EXE_spt"))
    .args(args)
    .env("HOME", root)
    .env("USERPROFILE", root)
    .env("HTTPS_PROXY", "http://127.0.0.1:1")
    .stdin(std::process::Stdio::null())
    .output()
    .unwrap()
}

#[test]
fn development_cli_ignores_login_and_preserves_settings() {
  let root = tempfile::tempdir().unwrap();
  let directory = root.path().join(".config/spotify-tui");
  fs::create_dir_all(&directory).unwrap();
  let names = ["client.yml", ".spotify_token_cache.json", "config.yml"];
  for name in names {
    fs::write(directory.join(name), "deliberately invalid: [\n").unwrap();
  }
  for args in [
    vec!["--dev", "queue", "--json"],
    vec!["episodes", "--dev", "--json", "--offset", "20"],
    vec!["--demo", "audiobooks", "--json"],
    vec![
      "--dev",
      "audiobooks",
      "--chapters",
      "b000000000000000000000",
      "--json",
    ],
    vec!["--dev", "audiobooks", "--search", "observatory", "--json"],
    vec!["--dev", "playback", "--status"],
    vec!["--dev", "playback", "--transfer", "Demo Living Room"],
    vec!["--dev", "playback", "--device", "Demo Desktop", "--toggle"],
    vec!["--dev", "episodes", "--resume", "e000000000000000000001"],
    vec!["--dev", "audiobooks", "--open", "b000000000000000000000"],
  ] {
    let output = run(root.path(), &args);
    assert!(
      output.status.success(),
      "{args:?}: {}",
      String::from_utf8_lossy(&output.stderr)
    );
    assert!(!output.stdout.is_empty(), "{args:?}");
    if args.contains(&"--json") {
      let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
      assert!(!value["items"].as_array().unwrap().is_empty(), "{args:?}");
    }
  }
  for name in names {
    assert_eq!(
      fs::read_to_string(directory.join(name)).unwrap(),
      "deliberately invalid: [\n"
    );
  }
  assert_eq!(fs::read_dir(&directory).unwrap().count(), names.len());
}

#[test]
fn development_cli_does_not_create_a_configuration_directory() {
  let root = tempfile::tempdir().unwrap();
  let output = run(root.path(), &["--dev", "playback", "--volume", "70"]);
  assert!(
    output.status.success(),
    "{}",
    String::from_utf8_lossy(&output.stderr)
  );
  assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
}
