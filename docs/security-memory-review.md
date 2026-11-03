# Security, memory and bloat review

Reviewed on 2026-11-03 against the local 2026 modernization changes. This
review covered authentication, credential files, HTTP transport, response
decoding, terminal output, configuration parsing, background work, retained
UI data, dependencies and release workflows.

## Findings and fixes

| Area | Finding | Resolution |
| --- | --- | --- |
| Credentials | Legacy files could retain broad permissions; reads could follow symlinks or consume oversized files. | Reject symlinked credential paths, require regular files, limit reads to 64 KiB, and enforce mode 0600 for files and 0700 for the application directory on Unix. Keep atomic writes. |
| OAuth callback | Invalid or ambiguous callbacks could terminate the sign-in attempt. | Ignore incorrect state, duplicate parameters, wrong origins/paths and incomplete requests; accept only the expected callback. Keep request-size and time limits. |
| HTTP | Redirects could replay token form data; response bodies were buffered without a size limit. | Require HTTPS, disable redirects, set connection/request timeouts, and limit streamed bodies to 8 MiB for successful API responses or 64 KiB for token/error responses. |
| Terminal output | Spotify text could contain terminal escape/control characters. | Strip control characters from decoded display strings and API error messages. Browser links use validated IDs and a fixed Spotify HTTPS origin. |
| Configuration | Invalid shortcuts could panic, YAML expansion was not explicitly bounded, and parse diagnostics could expose an old client secret. | Validate shortcuts safely, bound file size and YAML nesting/aliases/nodes/scalars, and use credential-safe error messages. |
| Dependencies | The YAML dependency pulled in the unmaintained `unsafe-libyaml` backend. | Replace it with `serde-saphyr`, enabling only serialization/deserialization. Remove unused dependency features and the older random-number stack. |
| Background work | Event queues and multiple runtime pools could retain unnecessary work and threads; the UI held its shared lock while waiting for input. | Use one two-worker Tokio runtime, bounded 64-item queues, nonblocking dispatch, dropped redundant ticks, and release the application lock before waiting. |
| Retained data | Browsing retained all fetched library pages and navigation routes. | Cache eight payload pages per collection, retain at most 2,048 navigation bookmarks and 64 routes, and refetch evicted pages when navigating backward. Refreshing a library resets its cache. |
| Allocations | Rendering and input actions cloned complete playback/show/page objects and rebuilt static help text. | Borrow objects, clone only selected IDs/items, decode JSON directly into models, share the HTTP pool, and reuse static home text. |
| Release/workflows | Unused code/features and default release settings inflated the binary; actions used mutable version references. | Remove unused handlers/events, bound search input, optimize and strip release binaries, pin action commits, and disable persisted checkout credentials. |

The dependency finding was
[RUSTSEC-2024-0449](https://rustsec.org/advisories/RUSTSEC-2024-0449.html), an
unmaintained-library advisory, rather than a demonstrated application exploit.

## Validation

- `cargo test --locked`: 97 tests pass, including oversized/chunked responses,
  redirect rejection, OAuth callback validation, credential permissions and
  symlinks, terminal controls, YAML budgets, queue saturation and cache eviction.
- `cargo clippy --locked --all-targets -- -D warnings`, formatting, locked
  release build and CLI help/completion smoke checks pass.
- `cargo audit`: zero known vulnerabilities and no advisory warnings using the
  RustSec database fetched on 2026-11-03.
- `cargo machete`: no unused direct dependencies found.

The local macOS release executable measured 8,058,000 bytes before this review
and 5,231,600 bytes after the fixes, using `cargo build --locked --release` on
the same checkout/toolchain. This measures executable size, not installation
size or runtime memory.

## Remaining limits

Cached response payloads, work queues and navigation are bounded; this is not
a fixed cap on total process memory. Decoded objects have allocation overhead,
and membership sets retain saved item IDs encountered during the session.
No live-account session or long-running heap profile was performed. Regression
tests use local mock servers; other operating systems require CI verification.

Tokens remain plaintext in the local cache. Unix permissions protect access;
Windows uses inherited filesystem ACLs. This does not protect secrets from
another process running as the same user or from a compromised machine. The
dependency audit reports known advisories at the review date, not proof that
all code is free of security defects.
