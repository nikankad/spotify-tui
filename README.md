# Spotify TUI

A terminal interface and CLI for controlling Spotify. This fork updates the
original Spotify TUI for Spotify's 2026 Web API.

Spotify Premium, your own Spotify developer app, and an active playback device
are required. This application controls playback; it does not stream audio.

## Installation

Build this checkout to get the updated client. Existing packages named
`spotify-tui` may still contain the original 2021 release.

```bash
cargo install --path . --locked
```

The binary is `spt`. Rustup reads the pinned toolchain from `rust-toolchain.toml`.
The HTTP client uses Rustls, so OpenSSL development packages are no longer needed.

## Connecting to Spotify

1. Sign in to the [Spotify developer dashboard](https://developer.spotify.com/dashboard)
   with a Premium account and create an app with Web API access.
2. Add `http://127.0.0.1:8888/callback` as a redirect URI and save. If you use a
   different port, register that exact URI instead. Spotify no longer accepts
   `localhost` redirect URIs.
3. Add any other accounts that will sign in to the app's allowed users.
4. Run `spt`, enter the app's Client ID, and confirm port 8888.
5. Open the printed authorization URL if the browser does not open automatically.
   Approve access and return to the terminal. If the callback port is occupied,
   paste the full redirect URL when prompted.
6. Open Spotify on a playback device. Press `d` in the terminal to select it.

Sign-in uses Authorization Code with PKCE. No client secret is requested or
stored. Tokens refresh automatically, including for CLI commands.

Existing users must register the new `127.0.0.1` redirect URI. The first run
reauthorizes old token caches; existing Client IDs, device selections and ports
are retained. An old `client_secret` key is removed when `client.yml` is loaded.

Client settings are in `~/.config/spotify-tui/client.yml`:

```yaml
client_id: "your-32-character-client-id"
device_id: null
port: 8888
```

Tokens are cached separately in
`~/.config/spotify-tui/.spotify_token_cache.json`. Delete that file to sign in
again. Client settings and tokens are written atomically with owner-only
permissions on Unix. Credential reads reject symlinks and oversized files.
See the [security and memory review](docs/security-memory-review.md) for
transport protections, cache limits and validation results.

## Usage

Run `spt` for the terminal UI and press `?` for the current keybindings.
Use `spt --help` or a subcommand's `--help` for the CLI.

```bash
spt --completions zsh
spt play --name "Your Playlist" --playlist --random
spt play --name "A song" --track
spt playback --like --shuffle
spt playback --toggle
spt list --liked --limit 50
spt search "A song" --tracks --format "%t from %b" --limit 10
```

Search is capped at 10 results per type. Library pages support up to 50 items.
Search requests all supported types in one call to reduce quota usage.

## Queue, saved episodes and audiobooks

Select **Queue**, **Saved Episodes**, or **Audiobooks** from the Library.
Queue shows what is playing and the upcoming tracks and episodes. Press `F5`
to refresh; `Enter` starts the selected item. The public API supports adding
items to the queue, but does not provide queue removal or reordering.

In Saved Episodes, `Enter` resumes from the saved listening position. Finished
episodes start from the beginning. Press `s` to save/remove a selected track,
episode or audiobook in the new views; `z` queues a track or episode. On an
existing podcast's episode page, `w` saves/removes the episode separately from
following the show. Playback `--like` and `--dislike` also work for episodes.

In Audiobooks, `Enter` opens the chapter list. Press `O` to open a book in
Spotify, or `Enter` on a chapter to open that chapter. Audiobook playback uses
the official Spotify app; the Web API's playback endpoints do not document
chapter playback. Availability depends on your region and account access.
Use the configured next/previous page keys to browse paged results.

To search the audiobook catalogue in the TUI, type `audiobook Dune` into the
search input. You can also paste a Spotify audiobook URL or URI to open its
chapters directly.

```bash
spt queue --json
spt episodes --limit 20 --offset 20
spt episodes --save spotify:episode:EPISODE_ID
spt episodes --resume EPISODE_ID
spt audiobooks --search "Dune" --limit 10
spt audiobooks --chapters AUDIOBOOK_ID
spt audiobooks --save AUDIOBOOK_ID
spt audiobooks --open AUDIOBOOK_ID
```

Read commands support `--json`; paged reads accept `--limit` and `--offset`.
See [feature coverage](docs/feature-coverage.md) for the current API additions
and Spotify app features that cannot be implemented through this connection.

## Configuration

A configuration file is located at `${HOME}/.config/spotify-tui/config.yml`, for snap `${HOME}/snap/spt/current/.config/spotify-tui/config.yml`
(not to be confused with client.yml which handles spotify authentication)

The following is a sample config.yml file:

```yaml
# Sample config file

# The theme colours can be an rgb string of the form "255, 255, 255" or a string that references the colours from your terminal theme: Reset, Black, Red, Green, Yellow, Blue, Magenta, Cyan, Gray, DarkGray, LightRed, LightGreen, LightYellow, LightBlue, LightMagenta, LightCyan, White.
theme:
  active: Cyan # current playing song in list
  banner: LightCyan # the "spotify-tui" banner on launch
  error_border: Red # error dialog border
  error_text: LightRed # error message text (e.g. "Spotify API reported error 404")
  hint: Yellow # hint text in errors
  hovered: Magenta # hovered pane border
  inactive: Gray # borders of inactive panes
  playbar_background: Black # background of progress bar
  playbar_progress: LightCyan # filled-in part of the progress bar
  playbar_progress_text: Cyan # song length and time played/left indicator in the progress bar
  playbar_text: White # artist name in player pane
  selected: LightCyan # a) selected pane border, b) hovered item in list, & c) track title in player
  text: "255, 255, 255" # text in panes
  header: White # header text in panes (e.g. 'Title', 'Artist', etc.)

behavior:
  seek_milliseconds: 5000
  volume_increment: 10
  # The lower the number the higher the "frames per second". Lower values use more CPU.
  tick_rate_milliseconds: 250
  # Enable text emphasis (typically italic/bold text styling). Disabling this might be important if the terminal config is otherwise restricted and rendering text escapes interferes with the UI.
  enable_text_emphasis: true
  # Controls whether to show a loading indicator in the top right of the UI whenever communicating with Spotify API
  show_loading_indicator: true
  # Disables the responsive layout that makes the search bar smaller on bigger
  # screens and enforces a wide search bar
  enforce_wide_search_bar: false
  # Determines the text icon to display next to "liked" Spotify items, such as
  # liked songs and albums, or followed artists. Can be any length string.
  # These icons require a patched nerd font.
  liked_icon: ♥
  shuffle_icon: 🔀
  repeat_track_icon: 🔂
  repeat_context_icon: 🔁
  playing_icon: ▶
  paused_icon: ⏸
  # Sets the window title to "spt - Spotify TUI" via ANSI escape code.
  set_window_title: true

keybindings:
  # Key stroke can be used if it only uses two keys:
  # ctrl-q works,
  # ctrl-alt-q doesn't.
  back: "ctrl-q"

  jump_to_album: "a"

  # Shift modifiers use a capital letter (also applies with other modifier keys
  # like ctrl-A)
  jump_to_artist_album: "A"

  manage_devices: "d"
  decrease_volume: "-"
  increase_volume: "+"
  toggle_playback: " "
  seek_backwards: "<"
  seek_forwards: ">"
  next_track: "n"
  previous_track: "p"
  copy_song_url: "c"
  copy_album_url: "C"
  help: "?"
  shuffle: "ctrl-s"
  repeat: "r"
  search: "/"
  jump_to_context: "o"
  basic_view: "B"
  add_item_to_queue: "z"
```

## Spotify API limits

Playlists, liked songs, saved albums, followed artists, podcasts, recently
played tracks, catalogue search, artist releases and playback controls remain
available subject to the app's Spotify permissions.

Removed features are audio analysis, song/artist recommendations, related
artists, artist top tracks, and the Made for You playlist-name search. Their
endpoints are unavailable to new development apps. Artist pages now show
releases. Old configuration keys for removed features are ignored.

Development apps cannot read every playlist, including Spotify-owned mixes and
some playlists owned by other users. Failed requests show an error instead of
leaving the previous playlist on screen. Unlike Spotifast, this client has no
librespot session to provide additional playlist or discovery data.

Spotify's Development Mode has account and quota restrictions. A `429` response
pauses requests for `Retry-After`; exhausted development quota is reported
separately and pauses requests for at least an hour. Apps registered by the
same developer share their quota. No failed writes are replayed automatically.

See [2026 compatibility notes](docs/2026-compatibility.md) for migration details
and sources. The supplied [Spotifast capability reference](docs/_reference/what-spotify-allows.md)
describes Spotifast's additional playback and private-session integrations;
it is not a feature list for Spotify TUI.

## Playback devices

Keep an official Spotify client or a configured
[spotifyd](https://github.com/Spotifyd/spotifyd) instance running. Press `d` to
select an available device. With no explicit selection, playback requests use
Spotify's active device. Devices marked restricted by Spotify cannot be
controlled through the Web API.

## Development

Start a populated, offline session without a Spotify account:

```bash
cargo run --locked -- --dev
```

`spt --dev` (alias `--demo`) starts in Liked Songs with sample playlists,
albums, artists, podcasts, saved episodes, audiobooks, a mixed queue and three
devices. Search, pagination, playback controls, saving/removing items, queuing
and device selection run against an in-memory backend. The search box displays
`DEV - offline`. Playback advances a simulated clock and produces no audio.
All changes reset when the process exits.

Development mode skips OAuth and the saved client, token and user configuration
files. It never connects to Spotify or saves settings. Pass `--config PATH`
explicitly to preview a theme or keybindings. Sample audiobook/chapter links
do not open a browser.

The same flag works with CLI commands, before or after the subcommand:

```bash
cargo run --locked -- --dev queue --json
cargo run --locked -- episodes --dev --json --offset 20
cargo run --locked -- --dev audiobooks --search observatory
cargo run --locked -- --dev playback --volume 70
```

Inspired by [Spotifast's demo mode](https://github.com/crmne/spotifast/blob/main/src/demo.rs).
The generated catalogue lives in `src/spotify/demo.rs`; it uses the normal
response models, UI handlers and CLI paths, without extra dependencies or
downloaded assets.

```bash
cargo check --locked --all-targets
cargo test --locked
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
```

The client uses Ratatui, Crossterm, Tokio 1, Clap 4 and Reqwest with Rustls.
Tests use local HTTP fixtures and do not require Spotify credentials. A live
account and playback device are needed to verify end-to-end playback.

## Contributors

Thanks goes to these wonderful people ([emoji key](https://allcontributors.org/docs/en/emoji-key)):

<!-- ALL-CONTRIBUTORS-LIST:START - Do not remove or modify this section -->
<!-- prettier-ignore-start -->
<!-- markdownlint-disable -->
<table>
  <tr>
    <td align="center"><a href="https://keliris.dev/"><img src="https://avatars2.githubusercontent.com/u/12150276?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Alexander Keliris</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=Rigellute" title="Code">💻</a> <a href="https://github.com/Rigellute/spotify-tui/commits?author=Rigellute" title="Documentation">📖</a> <a href="#design-Rigellute" title="Design">🎨</a> <a href="#blog-Rigellute" title="Blogposts">📝</a> <a href="#ideas-Rigellute" title="Ideas, Planning, & Feedback">🤔</a> <a href="#infra-Rigellute" title="Infrastructure (Hosting, Build-Tools, etc)">🚇</a> <a href="#maintenance-Rigellute" title="Maintenance">🚧</a> <a href="#platform-Rigellute" title="Packaging/porting to new platform">📦</a> <a href="https://github.com/Rigellute/spotify-tui/pulls?q=is%3Apr+reviewed-by%3ARigellute" title="Reviewed Pull Requests">👀</a></td>
    <td align="center"><a href="https://github.com/mikepombal"><img src="https://avatars3.githubusercontent.com/u/6864231?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Mickael Marques</b></sub></a><br /><a href="#financial-mikepombal" title="Financial">💵</a></td>
    <td align="center"><a href="https://github.com/HakierGrzonzo"><img src="https://avatars0.githubusercontent.com/u/36668331?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Grzegorz Koperwas</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=HakierGrzonzo" title="Documentation">📖</a></td>
    <td align="center"><a href="https://github.com/amgassert"><img src="https://avatars2.githubusercontent.com/u/22896005?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Austin Gassert</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=amgassert" title="Code">💻</a></td>
    <td align="center"><a href="https://robinette.dev"><img src="https://avatars2.githubusercontent.com/u/30757528?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Calen Robinette</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=calenrobinette" title="Code">💻</a></td>
    <td align="center"><a href="https://mcofficer.me"><img src="https://avatars0.githubusercontent.com/u/22377202?v=4?s=100" width="100px;" alt=""/><br /><sub><b>M*C*O</b></sub></a><br /><a href="#infra-MCOfficer" title="Infrastructure (Hosting, Build-Tools, etc)">🚇</a></td>
    <td align="center"><a href="https://github.com/eminence"><img src="https://avatars0.githubusercontent.com/u/402454?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Andrew Chin</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=eminence" title="Code">💻</a></td>
  </tr>
  <tr>
    <td align="center"><a href="https://www.samnaser.com/"><img src="https://avatars0.githubusercontent.com/u/4377348?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Sam Naser</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=Monkeyanator" title="Code">💻</a></td>
    <td align="center"><a href="https://github.com/radogost"><img src="https://avatars0.githubusercontent.com/u/15713820?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Micha</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=radogost" title="Code">💻</a></td>
    <td align="center"><a href="https://github.com/neriglissar"><img src="https://avatars2.githubusercontent.com/u/53038761?v=4?s=100" width="100px;" alt=""/><br /><sub><b>neriglissar</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=neriglissar" title="Code">💻</a></td>
    <td align="center"><a href="https://github.com/TimonPost"><img src="https://avatars3.githubusercontent.com/u/19969910?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Timon</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=TimonPost" title="Code">💻</a></td>
    <td align="center"><a href="https://github.com/echoSayonara"><img src="https://avatars2.githubusercontent.com/u/54503126?v=4?s=100" width="100px;" alt=""/><br /><sub><b>echoSayonara</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=echoSayonara" title="Code">💻</a></td>
    <td align="center"><a href="https://github.com/D-Nice"><img src="https://avatars1.githubusercontent.com/u/2888248?v=4?s=100" width="100px;" alt=""/><br /><sub><b>D-Nice</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=D-Nice" title="Documentation">📖</a> <a href="#infra-D-Nice" title="Infrastructure (Hosting, Build-Tools, etc)">🚇</a></td>
    <td align="center"><a href="http://gpawlik.com"><img src="https://avatars3.githubusercontent.com/u/6296883?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Grzegorz Pawlik</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=gpawlik" title="Code">💻</a></td>
  </tr>
  <tr>
    <td align="center"><a href="http://lenny.ninja"><img src="https://avatars1.githubusercontent.com/u/4027243?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Lennart Bernhardt</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=LennyPenny" title="Code">💻</a></td>
    <td align="center"><a href="https://github.com/BlackYoup"><img src="https://avatars3.githubusercontent.com/u/6098160?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Arnaud Lefebvre</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=BlackYoup" title="Code">💻</a></td>
    <td align="center"><a href="https://github.com/tem1029"><img src="https://avatars3.githubusercontent.com/u/57712713?v=4?s=100" width="100px;" alt=""/><br /><sub><b>tem1029</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=tem1029" title="Code">💻</a></td>
    <td align="center"><a href="http://peter.moss.dk"><img src="https://avatars2.githubusercontent.com/u/12544579?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Peter K. Moss</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=Peterkmoss" title="Code">💻</a></td>
    <td align="center"><a href="http://www.zephyrizing.net/"><img src="https://avatars1.githubusercontent.com/u/113102?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Geoff Shannon</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=RadicalZephyr" title="Code">💻</a></td>
    <td align="center"><a href="http://zacklukem.info"><img src="https://avatars0.githubusercontent.com/u/8787486?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Zachary Mayhew</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=zacklukem" title="Code">💻</a></td>
    <td align="center"><a href="http://jfaltis.de"><img src="https://avatars2.githubusercontent.com/u/45465572?v=4?s=100" width="100px;" alt=""/><br /><sub><b>jfaltis</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=jfaltis" title="Code">💻</a></td>
  </tr>
  <tr>
    <td align="center"><a href="https://marcelschr.me"><img src="https://avatars3.githubusercontent.com/u/19377618?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Marcel Schramm</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=Bios-Marcel" title="Documentation">📖</a></td>
    <td align="center"><a href="https://github.com/fangyi-zhou"><img src="https://avatars3.githubusercontent.com/u/7815439?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Fangyi Zhou</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=fangyi-zhou" title="Code">💻</a></td>
    <td align="center"><a href="https://github.com/synth-ruiner"><img src="https://avatars1.githubusercontent.com/u/8642013?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Max</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=synth-ruiner" title="Code">💻</a></td>
    <td align="center"><a href="https://github.com/svenvNL"><img src="https://avatars1.githubusercontent.com/u/13982006?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Sven van der Vlist</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=svenvNL" title="Code">💻</a></td>
    <td align="center"><a href="https://github.com/jacobchrismarsh"><img src="https://avatars2.githubusercontent.com/u/15932179?v=4?s=100" width="100px;" alt=""/><br /><sub><b>jacobchrismarsh</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=jacobchrismarsh" title="Code">💻</a></td>
    <td align="center"><a href="https://github.com/TheWalkingLeek"><img src="https://avatars2.githubusercontent.com/u/36076343?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Nils Rauch</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=TheWalkingLeek" title="Code">💻</a></td>
    <td align="center"><a href="https://github.com/sputnick1124"><img src="https://avatars1.githubusercontent.com/u/8843309?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Nick Stockton</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=sputnick1124" title="Code">💻</a> <a href="https://github.com/Rigellute/spotify-tui/issues?q=author%3Asputnick1124" title="Bug reports">🐛</a> <a href="#maintenance-sputnick1124" title="Maintenance">🚧</a> <a href="#question-sputnick1124" title="Answering Questions">💬</a> <a href="https://github.com/Rigellute/spotify-tui/commits?author=sputnick1124" title="Documentation">📖</a></td>
  </tr>
  <tr>
    <td align="center"><a href="http://stuarth.github.io"><img src="https://avatars3.githubusercontent.com/u/7055?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Stuart Hinson</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=stuarth" title="Code">💻</a></td>
    <td align="center"><a href="https://github.com/samcal"><img src="https://avatars3.githubusercontent.com/u/2117940?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Sam Calvert</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=samcal" title="Code">💻</a> <a href="https://github.com/Rigellute/spotify-tui/commits?author=samcal" title="Documentation">📖</a></td>
    <td align="center"><a href="https://github.com/jwijenbergh"><img src="https://avatars0.githubusercontent.com/u/46386452?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Jeroen Wijenbergh</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=jwijenbergh" title="Documentation">📖</a></td>
    <td align="center"><a href="https://twitter.com/KimberleyCook91"><img src="https://avatars3.githubusercontent.com/u/2683270?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Kimberley Cook</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=KimberleyCook" title="Documentation">📖</a></td>
    <td align="center"><a href="https://github.com/baxtea"><img src="https://avatars0.githubusercontent.com/u/22502477?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Audrey Baxter</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=baxtea" title="Code">💻</a></td>
    <td align="center"><a href="https://koehr.in"><img src="https://avatars2.githubusercontent.com/u/246402?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Norman</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=nkoehring" title="Documentation">📖</a></td>
    <td align="center"><a href="https://github.com/blackwolf12333"><img src="https://avatars0.githubusercontent.com/u/1572975?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Peter Maatman</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=blackwolf12333" title="Code">💻</a></td>
  </tr>
  <tr>
    <td align="center"><a href="https://github.com/AlexandreSi"><img src="https://avatars1.githubusercontent.com/u/32449369?v=4?s=100" width="100px;" alt=""/><br /><sub><b>AlexandreS</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=AlexandreSi" title="Code">💻</a></td>
    <td align="center"><a href="https://github.com/fiinnnn"><img src="https://avatars2.githubusercontent.com/u/5011796?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Finn Vos</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=fiinnnn" title="Code">💻</a></td>
    <td align="center"><a href="https://github.com/hurricanehrndz"><img src="https://avatars0.githubusercontent.com/u/5804237?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Carlos Hernandez</b></sub></a><br /><a href="#platform-hurricanehrndz" title="Packaging/porting to new platform">📦</a></td>
    <td align="center"><a href="https://github.com/pedrohva"><img src="https://avatars3.githubusercontent.com/u/33297928?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Pedro Alves</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=pedrohva" title="Code">💻</a></td>
    <td align="center"><a href="https://gitlab.com/jtagcat/"><img src="https://avatars1.githubusercontent.com/u/38327267?v=4?s=100" width="100px;" alt=""/><br /><sub><b>jtagcat</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=jtagcat" title="Documentation">📖</a></td>
    <td align="center"><a href="https://github.com/BKitor"><img src="https://avatars0.githubusercontent.com/u/16880850?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Benjamin Kitor</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=BKitor" title="Code">💻</a></td>
    <td align="center"><a href="https://ales.rocks"><img src="https://avatars0.githubusercontent.com/u/544082?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Aleš Najmann</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=littleli" title="Documentation">📖</a> <a href="#platform-littleli" title="Packaging/porting to new platform">📦</a></td>
  </tr>
  <tr>
    <td align="center"><a href="https://github.com/jeremystucki"><img src="https://avatars3.githubusercontent.com/u/7629727?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Jeremy Stucki</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=jeremystucki" title="Code">💻</a></td>
    <td align="center"><a href="http://pt2121.github.io"><img src="https://avatars0.githubusercontent.com/u/616399?v=4?s=100" width="100px;" alt=""/><br /><sub><b>(´⌣`ʃƪ)</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=pt2121" title="Code">💻</a></td>
    <td align="center"><a href="https://github.com/tim77"><img src="https://avatars0.githubusercontent.com/u/5614476?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Artem Polishchuk</b></sub></a><br /><a href="#platform-tim77" title="Packaging/porting to new platform">📦</a></td>
    <td align="center"><a href="https://github.com/slumber"><img src="https://avatars2.githubusercontent.com/u/48099298?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Chris Sosnin</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=slumber" title="Code">💻</a></td>
    <td align="center"><a href="http://www.benbuhse.com"><img src="https://avatars1.githubusercontent.com/u/21225303?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Ben Buhse</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=bwbuhse" title="Documentation">📖</a></td>
    <td align="center"><a href="https://github.com/ilnaes"><img src="https://avatars1.githubusercontent.com/u/20805499?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Sean Li</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=ilnaes" title="Code">💻</a></td>
    <td align="center"><a href="https://github.com/TimotheeGerber"><img src="https://avatars3.githubusercontent.com/u/37541513?v=4?s=100" width="100px;" alt=""/><br /><sub><b>TimotheeGerber</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=TimotheeGerber" title="Code">💻</a> <a href="https://github.com/Rigellute/spotify-tui/commits?author=TimotheeGerber" title="Documentation">📖</a></td>
  </tr>
  <tr>
    <td align="center"><a href="https://github.com/fratajczak"><img src="https://avatars2.githubusercontent.com/u/33835579?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Ferdinand Ratajczak</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=fratajczak" title="Code">💻</a></td>
    <td align="center"><a href="https://github.com/sheelc"><img src="https://avatars0.githubusercontent.com/u/1355710?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Sheel Choksi</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=sheelc" title="Code">💻</a></td>
    <td align="center"><a href="http://fnanp.in-ulm.de/microblog/"><img src="https://avatars1.githubusercontent.com/u/414112?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Michael Hellwig</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=mhellwig" title="Documentation">📖</a></td>
    <td align="center"><a href="https://github.com/oliver-daniel"><img src="https://avatars2.githubusercontent.com/u/17235417?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Oliver Daniel</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=oliver-daniel" title="Code">💻</a></td>
    <td align="center"><a href="https://github.com/Drewsapple"><img src="https://avatars2.githubusercontent.com/u/4532572?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Drew Fisher</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=Drewsapple" title="Code">💻</a></td>
    <td align="center"><a href="https://github.com/ncoder-1"><img src="https://avatars0.githubusercontent.com/u/7622286?v=4?s=100" width="100px;" alt=""/><br /><sub><b>ncoder-1</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=ncoder-1" title="Documentation">📖</a></td>
    <td align="center"><a href="http://macguire.me"><img src="https://avatars3.githubusercontent.com/u/18323154?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Macguire Rintoul</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=macguirerintoul" title="Documentation">📖</a></td>
  </tr>
  <tr>
    <td align="center"><a href="http://ricardohe97.github.io"><img src="https://avatars3.githubusercontent.com/u/28399979?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Ricardo Holguin</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=RicardoHE97" title="Code">💻</a></td>
    <td align="center"><a href="https://ksk.netlify.com"><img src="https://avatars3.githubusercontent.com/u/13160198?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Keisuke Toyota</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=ksk001100" title="Code">💻</a></td>
    <td align="center"><a href="https://jackson15j.github.io"><img src="https://avatars1.githubusercontent.com/u/3226988?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Craig Astill</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=jackson15j" title="Code">💻</a></td>
    <td align="center"><a href="https://github.com/onielfa"><img src="https://avatars0.githubusercontent.com/u/4358172?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Onielfa</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=onielfa" title="Code">💻</a></td>
    <td align="center"><a href="https://usrme.xyz"><img src="https://avatars3.githubusercontent.com/u/5902545?v=4?s=100" width="100px;" alt=""/><br /><sub><b>usrme</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=usrme" title="Documentation">📖</a></td>
    <td align="center"><a href="https://github.com/murlakatamenka"><img src="https://avatars2.githubusercontent.com/u/7361274?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Sergey A.</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=murlakatamenka" title="Code">💻</a></td>
    <td align="center"><a href="https://github.com/elcih17"><img src="https://avatars3.githubusercontent.com/u/17084445?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Hideyuki Okada</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=elcih17" title="Code">💻</a></td>
  </tr>
  <tr>
    <td align="center"><a href="https://github.com/kepae"><img src="https://avatars2.githubusercontent.com/u/4238598?v=4?s=100" width="100px;" alt=""/><br /><sub><b>kepae</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=kepae" title="Code">💻</a> <a href="https://github.com/Rigellute/spotify-tui/commits?author=kepae" title="Documentation">📖</a></td>
    <td align="center"><a href="https://github.com/ericonr"><img src="https://avatars0.githubusercontent.com/u/34201958?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Érico Nogueira Rolim</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=ericonr" title="Code">💻</a></td>
    <td align="center"><a href="https://github.com/BeneCollyridam"><img src="https://avatars2.githubusercontent.com/u/15802915?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Alexander Meinhardt Scheurer</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=BeneCollyridam" title="Code">💻</a></td>
    <td align="center"><a href="https://github.com/Toaster192"><img src="https://avatars0.githubusercontent.com/u/14369229?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Ondřej Kinšt</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=Toaster192" title="Code">💻</a></td>
    <td align="center"><a href="https://github.com/Kryan90"><img src="https://avatars3.githubusercontent.com/u/18740821?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Kryan90</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=Kryan90" title="Documentation">📖</a></td>
    <td align="center"><a href="https://github.com/n-ivanov"><img src="https://avatars3.githubusercontent.com/u/11470871?v=4?s=100" width="100px;" alt=""/><br /><sub><b>n-ivanov</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=n-ivanov" title="Code">💻</a></td>
    <td align="center"><a href="http://matthewbilyeu.com/resume/"><img src="https://avatars3.githubusercontent.com/u/1185129?v=4?s=100" width="100px;" alt=""/><br /><sub><b>bi1yeu</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=bi1yeu" title="Code">💻</a> <a href="https://github.com/Rigellute/spotify-tui/commits?author=bi1yeu" title="Documentation">📖</a></td>
  </tr>
  <tr>
    <td align="center"><a href="https://github.com/Utagai"><img src="https://avatars2.githubusercontent.com/u/10730394?v=4?s=100" width="100px;" alt=""/><br /><sub><b>May</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=Utagai" title="Code">💻</a></td>
    <td align="center"><a href="https://mucinoab.github.io/"><img src="https://avatars1.githubusercontent.com/u/28630268?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Bruno A. Muciño</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=mucinoab" title="Code">💻</a></td>
    <td align="center"><a href="https://github.com/OrangeFran"><img src="https://avatars2.githubusercontent.com/u/55061632?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Finn Hediger</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=OrangeFran" title="Code">💻</a></td>
    <td align="center"><a href="https://github.com/dp304"><img src="https://avatars1.githubusercontent.com/u/34493835?v=4?s=100" width="100px;" alt=""/><br /><sub><b>dp304</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=dp304" title="Code">💻</a></td>
    <td align="center"><a href="http://marcomicera.github.io"><img src="https://avatars0.githubusercontent.com/u/13918587?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Marco Micera</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=marcomicera" title="Documentation">📖</a></td>
    <td align="center"><a href="http://marcoieni.com"><img src="https://avatars3.githubusercontent.com/u/11428655?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Marco Ieni</b></sub></a><br /><a href="#infra-MarcoIeni" title="Infrastructure (Hosting, Build-Tools, etc)">🚇</a></td>
    <td align="center"><a href="https://github.com/ArturKovacs"><img src="https://avatars3.githubusercontent.com/u/8320264?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Artúr Kovács</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=ArturKovacs" title="Code">💻</a></td>
  </tr>
  <tr>
    <td align="center"><a href="https://github.com/aokellermann"><img src="https://avatars.githubusercontent.com/u/26678747?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Antony Kellermann</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=aokellermann" title="Code">💻</a></td>
    <td align="center"><a href="https://github.com/rasmuspeders1"><img src="https://avatars.githubusercontent.com/u/1898960?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Rasmus Pedersen</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=rasmuspeders1" title="Code">💻</a></td>
    <td align="center"><a href="https://github.com/noir-Z"><img src="https://avatars.githubusercontent.com/u/45096516?v=4?s=100" width="100px;" alt=""/><br /><sub><b>noir-Z</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=noir-Z" title="Documentation">📖</a></td>
    <td align="center"><a href="https://davidbailey.codes/"><img src="https://avatars.githubusercontent.com/u/4248177?v=4?s=100" width="100px;" alt=""/><br /><sub><b>David Bailey</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=davidbailey00" title="Documentation">📖</a></td>
    <td align="center"><a href="https://github.com/sheepwall"><img src="https://avatars.githubusercontent.com/u/22132993?v=4?s=100" width="100px;" alt=""/><br /><sub><b>sheepwall</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=sheepwall" title="Code">💻</a></td>
    <td align="center"><a href="https://github.com/Hwatwasthat"><img src="https://avatars.githubusercontent.com/u/29790143?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Hwatwasthat</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=Hwatwasthat" title="Code">💻</a></td>
    <td align="center"><a href="https://github.com/Jesse-Bakker"><img src="https://avatars.githubusercontent.com/u/22473248?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Jesse</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=Jesse-Bakker" title="Code">💻</a></td>
  </tr>
  <tr>
    <td align="center"><a href="https://github.com/hantatsang"><img src="https://avatars.githubusercontent.com/u/11912225?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Sang</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=hantatsang" title="Documentation">📖</a></td>
    <td align="center"><a href="https://yktakaha4.github.io/"><img src="https://avatars.githubusercontent.com/u/20282867?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Yuuki Takahashi</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=yktakaha4" title="Documentation">📖</a></td>
    <td align="center"><a href="https://alejandr0angul0.dev/"><img src="https://avatars.githubusercontent.com/u/5242883?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Alejandro Angulo</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=alejandro-angulo" title="Code">💻</a></td>
    <td align="center"><a href="http://t.me/lego1as"><img src="https://avatars.githubusercontent.com/u/11005780?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Anton Kostin</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=masguit42" title="Documentation">📖</a></td>
    <td align="center"><a href="https://justinsexton.net"><img src="https://avatars.githubusercontent.com/u/20236003?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Justin Sexton</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=JSextonn" title="Code">💻</a></td>
    <td align="center"><a href="https://github.com/lejiati"><img src="https://avatars.githubusercontent.com/u/6442124?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Jiati Le</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=lejiati" title="Documentation">📖</a></td>
    <td align="center"><a href="https://github.com/cobbinma"><img src="https://avatars.githubusercontent.com/u/578718?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Matthew Cobbing</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=cobbinma" title="Code">💻</a></td>
  </tr>
  <tr>
    <td align="center"><a href="https://milo123459.vercel.app"><img src="https://avatars.githubusercontent.com/u/50248166?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Milo</b></sub></a><br /><a href="#infra-Milo123459" title="Infrastructure (Hosting, Build-Tools, etc)">🚇</a></td>
    <td align="center"><a href="https://www.diegoveralli.com"><img src="https://avatars.githubusercontent.com/u/297206?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Diego Veralli</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=diegov" title="Code">💻</a></td>
    <td align="center"><a href="https://github.com/majabojarska"><img src="https://avatars.githubusercontent.com/u/33836570?v=4?s=100" width="100px;" alt=""/><br /><sub><b>Maja Bojarska</b></sub></a><br /><a href="https://github.com/Rigellute/spotify-tui/commits?author=majabojarska" title="Code">💻</a></td>
  </tr>
</table>

<!-- markdownlint-restore -->
<!-- prettier-ignore-end -->

<!-- ALL-CONTRIBUTORS-LIST:END -->

This project follows the [all-contributors](https://github.com/all-contributors/all-contributors) specification. Contributions of any kind welcome!
