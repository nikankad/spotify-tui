# Spotify feature coverage

The original repository's last release was 0.25.0 in August 2021. This update
adds later public API capabilities and fills the original client's podcast
library gaps. It keeps the existing Web API / Spotify Connect architecture.

| Capability | Support in this client |
| --- | --- |
| Read playback queue | Queue library view and `spt queue`; current item and upcoming tracks/episodes, JSON output, manual refresh |
| Add tracks and episodes to queue | `z` on supported rows, including saved episodes and podcast episode pages |
| Saved podcast episodes | Paged library view; save/remove by episode URI; CLI `episodes` command |
| Resume podcasts | Saved episode `Enter` and CLI `episodes --resume`; uses saved progress, restarts finished episodes |
| Audiobook library | Paged books, save/remove operations, authors and Spotify links |
| Audiobook catalogue | `audiobook QUERY` in the TUI search input; CLI `audiobooks --search` |
| Audiobook chapters | Paged chapter lists, listening progress when returned, links to Spotify for playback |
| Device volume capability | Disable unsupported volume requests; display unavailable volume control |
| 2026 library API | Save/remove/check using typed Spotify URIs through the generic library endpoints |
| 2026 playlist API | Read playlist items through the current endpoint and accept both response formats |

## Limits of this connection

The public Web API has no documented controls for Jam sessions, Smart Shuffle,
starting Blend, DJ, lyrics, Canvas/video playback, playlist folders, listening
parties, crossfade, playback speed, or selecting lossless formats. These need a
separate playback integration or private protocols, and are not added here.
An official playback device may support its own audio settings independently.

Queue reading and adding do not provide removal or reordering. Starting a
selected queue item issues an explicit play request; it does not reposition or
edit Spotify's queue. No queue writes happen while browsing or refreshing.

The documented playback API does not list audiobook/chapter playback. The
client opens those items in Spotify instead of sending unsupported playback
requests. Audiobook availability and entitlements depend on account and market.
The Web API does not support offline downloads for this client.

The May 2026 `account_id` addition is intended for linking users to a service.
This terminal client has no service-side account database, so it continues to
use OAuth tokens and Spotify item IDs rather than introducing account linking.

## Sources

- [Spotify: queue](https://developer.spotify.com/documentation/web-api/reference/get-queue)
- [Spotify: add to queue](https://developer.spotify.com/documentation/web-api/reference/add-to-queue)
- [Spotify: saved episodes](https://developer.spotify.com/documentation/web-api/reference/get-users-saved-episodes)
- [Spotify: saved audiobooks](https://developer.spotify.com/documentation/web-api/reference/get-users-saved-audiobooks)
- [Spotify: audiobook chapters](https://developer.spotify.com/documentation/web-api/reference/get-audiobook-chapters)
- [Spotify: playback](https://developer.spotify.com/documentation/web-api/reference/start-a-users-playback)
- [Spotify: device capabilities](https://developer.spotify.com/documentation/web-api/reference/get-a-users-available-devices)
- [Spotify: May 2026 changes](https://developer.spotify.com/documentation/web-api/references/changes/may-2026)
- [Supplied Spotifast capability reference](_reference/what-spotify-allows.md)
