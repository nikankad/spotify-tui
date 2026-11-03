# Spotify TUI compatibility in 2026

The application targets the public Spotify Web API available to personal
Development Mode apps. It uses the listener's own app registration. Spotifast's
[capability reference](_reference/what-spotify-allows.md) was supplied with this
project and guided the distinction between Web API features and features that
need a separate playback session.

## Authentication

Authorization Code with PKCE replaces the old client-secret flow. The callback
uses `http://127.0.0.1:PORT/callback`, validates the OAuth state and path, and
has a bounded wait. Register the exact loopback URI in the Spotify dashboard.
Existing client settings remain readable; the unused secret is removed on load.
Old caches require one browser sign-in. New caches bind the refresh token to
its Client ID and granted scopes. Refresh runs on demand before requests and
once after a rejected access token.

## Endpoint and response changes

- Playlist contents use `/playlists/{id}/items`. Models accept both `items` /
  `item` and older `tracks` / `track` response names.
- Library save, remove and contains operations use `/me/library` and
  `/me/library/contains`, with typed Spotify URIs for tracks, albums, artists,
  shows and playlists. Empty batches are skipped and large batches are split.
- Search requests all five UI result types in one call and limits each type
  to 1–10 results. Other paged reads use 1–50.
- Removed popularity, followers, country and available-market metadata is not
  required to decode responses. Missing publisher and nullable device metadata
  are tolerated. Playback distinguishes episodes from tracks using their type.
- Rate limits share a cooldown across requests. `Retry-After` is respected;
  `QUOTA_EXCEEDED` gets a separate error and a cooldown of at least an hour.
  Playback polling cannot bypass the cooldown. Failed writes are not replayed
  on rate limits or network errors.

## Added features

- A queue viewer with mixed tracks and podcast episodes, a current-item label,
  manual refresh, and CLI JSON output.
- Saved episode browsing with paging, save/remove operations, episode queuing,
  and resume playback using Spotify's stored listening position. Finished
  episodes restart at zero. Saving a podcast never uses a track URI.
- Audiobook library browsing, saving/removing, catalogue search, and paged
  chapter lists. Book/chapter links open in Spotify for playback.
- Device volume controls respect Spotify's `supports_volume` capability.

See [feature coverage](feature-coverage.md) for public API support and the
features that require a different playback integration.

## Removed UI features

Audio analysis, recommendations/radio, related artists, artist top tracks and
Made for You discovery have been removed, along with their handlers, routes,
help entries and settings. Artist pages show album releases. No private
Spotify endpoint, shared app identity or librespot fallback is introduced.
Playback continues to require a separate active Spotify Connect device.

## Tooling

Rust edition 2024 replaces edition 2018. A toolchain file pins Rust 1.95.0.
Ratatui replaces the unmaintained tui crate; Tokio 1 and Reqwest/Rustls replace
the old RSpotify/Tokio 0.2 networking stack. Clap 4 handles CLI parsing and
completions. YAML uses serde-saphyr. Clipboard support uses arboard 3 without
image support. CI checks Linux, macOS and Windows. Release builds include
Apple Silicon and Intel macOS separately and no longer install OpenSSL 1.1.

## Verification

Run the commands in the README. Regression tests cover PKCE, callback
validation, token refresh, current endpoint paths, search limits, rate limits,
empty playback, response changes and CLI parsing. Tests use loopback HTTP
servers and require no Spotify account. Actual Spotify permissions and Connect
playback must still be checked with a real account.

## Sources

- [Spotify: February 2026 API changes](https://developer.spotify.com/documentation/web-api/references/changes/february-2026)
- [Spotify: 2026 migration guide](https://developer.spotify.com/documentation/web-api/tutorials/february-2026-migration-guide)
- [Spotify: redirect URI requirements](https://developer.spotify.com/documentation/web-api/concepts/redirect_uri)
- [Spotify: Authorization Code with PKCE](https://developer.spotify.com/documentation/web-api/tutorials/code-pkce-flow)
- [Spotify: July 2026 quota changes](https://developer.spotify.com/blog/2026-07-23-web-api-quota-updates)
- [Spotifast](https://github.com/crmne/spotifast), especially `src/api/models.rs`,
  `src/api/client.rs` and `docs/_reference/what-spotify-allows.md`.
