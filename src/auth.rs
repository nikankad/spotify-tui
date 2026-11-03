//! OAuth Authorization Code with PKCE for a public desktop client.
use crate::config::ClientConfig;
use anyhow::{Context, Result, anyhow, bail};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
  fs,
  io::{Read, Write},
  path::{Path, PathBuf},
  time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  net::TcpListener,
};
use url::Url;

const TOKEN_URL: &str = "https://accounts.spotify.com/api/token";
pub const SCOPES: &str = "playlist-read-collaborative playlist-read-private playlist-modify-private playlist-modify-public user-follow-read user-follow-modify user-library-modify user-library-read user-modify-playback-state user-read-currently-playing user-read-playback-state user-read-playback-position user-read-recently-played user-read-private";

#[derive(Deserialize, Serialize)]
struct StoredToken {
  client_id: String,
  access_token: String,
  refresh_token: String,
  expires_at: u64,
  scope: String,
}
#[derive(Deserialize)]
struct TokenResponse {
  access_token: String,
  refresh_token: Option<String>,
  expires_in: u64,
  scope: Option<String>,
}

pub struct Auth {
  http: Client,
  token: StoredToken,
  cache_path: PathBuf,
  token_url: String,
}

fn now() -> u64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap_or_default()
    .as_secs()
}
fn random_string() -> Result<String> {
  let mut bytes = [0u8; 32];
  getrandom::fill(&mut bytes)
    .map_err(|error| anyhow!("Cannot obtain secure random bytes: {error}"))?;
  Ok(URL_SAFE_NO_PAD.encode(bytes))
}
fn challenge(verifier: &str) -> String {
  URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// Atomic replacement, with owner-only permissions on Unix, including old caches.
pub fn write_private(path: &Path, contents: &[u8]) -> Result<()> {
  let temporary = path.with_extension(format!("{}.tmp", random_string()?));
  let mut options = fs::OpenOptions::new();
  options.write(true).create_new(true);
  #[cfg(unix)]
  {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);
  }
  let result = (|| -> Result<()> {
    let mut file = options.open(&temporary)?;
    file.write_all(contents)?;
    file.sync_all()?;
    fs::rename(&temporary, path)?;
    Ok(())
  })();
  if result.is_err() {
    let _ = fs::remove_file(temporary);
  }
  result
}

pub fn read_private(path: &Path, limit: usize) -> Result<Vec<u8>> {
  if fs::symlink_metadata(path)?.file_type().is_symlink() {
    bail!("Credential path must not be a symlink");
  }
  let mut options = fs::OpenOptions::new();
  options.read(true);
  #[cfg(unix)]
  {
    use std::os::unix::fs::OpenOptionsExt;
    options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
  }
  let file = options.open(path)?;
  let metadata = file.metadata()?;
  if !metadata.is_file() {
    bail!("Credential path is not a regular file");
  }
  if metadata.len() > limit as u64 {
    bail!("Credential file exceeds the {limit}-byte limit");
  }
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
  }
  let mut bytes = Vec::new();
  file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
  if bytes.len() > limit {
    bail!("Credential file exceeds the {limit}-byte limit");
  }
  Ok(bytes)
}

pub fn private_directory(path: &Path) -> Result<()> {
  fs::create_dir_all(path)?;
  let metadata = fs::symlink_metadata(path)?;
  if !metadata.is_dir() || metadata.file_type().is_symlink() {
    bail!("Spotify configuration directory must be a real directory");
  }
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
  }
  Ok(())
}

impl Auth {
  #[cfg(test)]
  pub(crate) fn for_test(token_url: String, cache_path: PathBuf) -> Self {
    Self {
      http: crate::http::builder()
        .https_only(false)
        .no_proxy()
        .build()
        .unwrap(),
      token_url,
      cache_path,
      token: StoredToken {
        client_id: "test-client".into(),
        access_token: "test-token".into(),
        refresh_token: "refresh-original".into(),
        expires_at: now() + 3600,
        scope: SCOPES.into(),
      },
    }
  }

  pub fn client(&self) -> Client {
    self.http.clone()
  }

  pub async fn authenticate(config: &ClientConfig) -> Result<Self> {
    let http = crate::http::client()?;
    let cache_path = config.get_or_build_paths()?.token_cache_path;
    let contents = match read_private(&cache_path, crate::http::TOKEN_BODY_LIMIT) {
      Ok(contents) => Some(contents),
      Err(error)
        if error
          .downcast_ref::<std::io::Error>()
          .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound) =>
      {
        None
      }
      Err(error) => return Err(error.context("Cannot read Spotify token cache")),
    };
    if let Some(contents) = contents
      && let Ok(token) = serde_json::from_slice::<StoredToken>(&contents)
      && token.client_id == config.client_id
      && !token.refresh_token.is_empty()
      && SCOPES
        .split_whitespace()
        .all(|scope| token.scope.split_whitespace().any(|s| s == scope))
    {
      let mut auth = Self {
        http,
        token,
        cache_path,
        token_url: TOKEN_URL.into(),
      };
      auth.access_token(false).await.context(
        "Spotify sign-in expired. Remove .spotify_token_cache.json and run spt again to sign in",
      )?;
      return Ok(auth);
    }
    // An old client-secret cache cannot be refreshed using PKCE. Reauthorize once.
    let verifier = random_string()?;
    let state = random_string()?;
    let redirect = config.get_redirect_uri();
    let mut url = Url::parse("https://accounts.spotify.com/authorize")?;
    url.query_pairs_mut().extend_pairs([
      ("client_id", config.client_id.as_str()),
      ("response_type", "code"),
      ("redirect_uri", redirect.as_str()),
      ("state", state.as_str()),
      ("scope", SCOPES),
      ("code_challenge_method", "S256"),
      ("code_challenge", challenge(&verifier).as_str()),
    ]);
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, config.get_port())).await;
    println!("Open this URL to sign in to Spotify:\n{url}");
    let _ = webbrowser::open(url.as_str());
    let code = match listener {
      Ok(listener) => tokio::time::timeout(
        Duration::from_secs(180),
        receive_callback(listener, &redirect, &state),
      )
      .await
      .context("Spotify sign-in timed out; run spt again")??,
      Err(error) => {
        println!("Cannot listen for the callback ({error}). Paste the full redirect URL:");
        let input = tokio::task::spawn_blocking(|| -> std::io::Result<String> {
          let mut input = String::new();
          std::io::stdin().read_line(&mut input)?;
          Ok(input)
        })
        .await??;
        callback_code(input.trim(), &redirect, &state)?.context("Missing authorization code")?
      }
    };
    let response = http
      .post(TOKEN_URL)
      .form(&[
        ("grant_type", "authorization_code"),
        ("client_id", config.client_id.as_str()),
        ("code", code.as_str()),
        ("redirect_uri", redirect.as_str()),
        ("code_verifier", verifier.as_str()),
      ])
      .send()
      .await?;
    let response = token_response(response).await?;
    let mut auth = Self {
      http,
      token: StoredToken {
        client_id: config.client_id.clone(),
        access_token: String::new(),
        refresh_token: String::new(),
        expires_at: 0,
        scope: SCOPES.to_string(),
      },
      cache_path,
      token_url: TOKEN_URL.into(),
    };
    auth.update(response)?;
    Ok(auth)
  }

  fn update(&mut self, response: TokenResponse) -> Result<()> {
    if response.access_token.is_empty() || response.expires_in == 0 {
      bail!("Spotify returned an unusable token");
    }
    self.token.access_token = response.access_token;
    if let Some(refresh_token) = response.refresh_token {
      self.token.refresh_token = refresh_token;
    }
    if self.token.refresh_token.is_empty() {
      bail!("Spotify did not return a refresh token; sign in again");
    }
    if let Some(scope) = response.scope {
      self.token.scope = scope;
    }
    self.token.expires_at = now().saturating_add(response.expires_in);
    write_private(&self.cache_path, &serde_json::to_vec(&self.token)?)
  }

  pub async fn access_token(&mut self, force: bool) -> Result<String> {
    if force || now().saturating_add(60) >= self.token.expires_at {
      let response = self
        .http
        .post(&self.token_url)
        .form(&[
          ("grant_type", "refresh_token"),
          ("client_id", self.token.client_id.as_str()),
          ("refresh_token", self.token.refresh_token.as_str()),
        ])
        .send()
        .await?;
      self.update(token_response(response).await?)?;
    }
    Ok(self.token.access_token.clone())
  }
}

async fn token_response(response: reqwest::Response) -> Result<TokenResponse> {
  let status = response.status();
  if !status.is_success() {
    bail!(
      "Spotify authorization failed ({status}); check the app's Client ID, redirect URI and allowed users, then sign in again"
    );
  }
  Ok(serde_json::from_slice(
    &crate::http::body(response, crate::http::TOKEN_BODY_LIMIT).await?,
  )?)
}

fn callback_code(input: &str, redirect: &str, state: &str) -> Result<Option<String>> {
  let expected = Url::parse(redirect)?;
  let parsed = if input.starts_with('/') {
    expected.join(input)
  } else {
    Url::parse(input)
  };
  let Ok(url) = parsed else { return Ok(None) };
  if url.origin() != expected.origin()
    || url.path() != expected.path()
    || url.fragment().is_some()
    || !url.username().is_empty()
    || url.password().is_some()
  {
    return Ok(None);
  }
  let pairs: Vec<_> = url.query_pairs().collect();
  let value = |key| {
    pairs
      .iter()
      .find(|(k, _)| k == key)
      .map(|(_, v)| v.to_string())
  };
  if ["state", "code", "error"]
    .iter()
    .any(|key| pairs.iter().filter(|(k, _)| k == key).count() > 1)
    || value("state").as_deref() != Some(state)
  {
    return Ok(None);
  }
  if value("error").is_some() {
    bail!("Spotify authorization was declined; run spt to try again");
  }
  Ok(value("code").filter(|s| !s.is_empty()))
}

async fn receive_callback(listener: TcpListener, redirect: &str, state: &str) -> Result<String> {
  loop {
    let (mut stream, _) = listener.accept().await?;
    let request = tokio::time::timeout(Duration::from_secs(5), async {
      let mut request = Vec::new();
      let mut chunk = [0; 1024];
      while request.len() < 8192 && !request.windows(4).any(|s| s == b"\r\n\r\n") {
        let size = stream.read(&mut chunk).await?;
        if size == 0 {
          break;
        }
        request.extend_from_slice(&chunk[..size]);
      }
      Ok::<_, std::io::Error>(request)
    })
    .await;
    let Ok(Ok(request)) = request else { continue };
    if request.len() > 8192 || !request.windows(4).any(|part| part == b"\r\n\r\n") {
      continue;
    }
    let line = std::str::from_utf8(&request)
      .unwrap_or_default()
      .lines()
      .next()
      .unwrap_or_default();
    let mut parts = line.split_whitespace();
    let result = if parts.next() == Some("GET") {
      callback_code(parts.next().unwrap_or_default(), redirect, state)
    } else {
      Ok(None)
    };
    let (status, body) = match &result {
      Ok(Some(_)) => ("200 OK", include_str!("redirect_uri.html")),
      _ => (
        "400 Bad Request",
        "Invalid Spotify callback. Return to the terminal and try again.",
      ),
    };
    let response = format!(
      "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
      body.len()
    );
    let _ = tokio::time::timeout(
      Duration::from_secs(5),
      stream.write_all(response.as_bytes()),
    )
    .await;
    if let Some(code) = result? {
      return Ok(code);
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  #[tokio::test]
  async fn refresh_rotates_tokens_and_persists_them_privately() {
    let (url, task) = crate::spotify::test_support::server(vec![(
      200,
      "",
      r#"{"access_token":"new-access","refresh_token":"new-refresh","expires_in":3600}"#.into(),
    )])
    .await;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("token.json");
    fs::write(&path, "old cache").unwrap();
    let mut auth = Auth::for_test(url, path.clone());
    auth.token.expires_at = 0;
    assert_eq!(auth.access_token(false).await.unwrap(), "new-access");
    assert_eq!(auth.token.refresh_token, "new-refresh");
    assert_eq!(auth.access_token(false).await.unwrap(), "new-access");
    let persisted: StoredToken = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(persisted.refresh_token, "new-refresh");
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt;
      assert_eq!(
        fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o600
      );
    }
    assert_eq!(task.await.unwrap().len(), 1);
  }

  #[cfg(unix)]
  #[test]
  fn private_storage_rejects_symlinks_and_fixes_legacy_permissions() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("token");
    fs::write(&target, "private-token").unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(read_private(&target, 64).unwrap(), b"private-token");
    assert_eq!(
      fs::metadata(&target).unwrap().permissions().mode() & 0o777,
      0o600
    );
    let link = directory.path().join("link");
    symlink(&target, &link).unwrap();
    assert!(read_private(&link, 64).is_err());
    let dir_link = directory.path().join("dir-link");
    symlink(directory.path(), &dir_link).unwrap();
    assert!(private_directory(&dir_link).is_err());
    assert!(read_private(&target, 4).is_err());
  }

  #[test]
  fn rejects_ambiguous_oauth_callbacks() {
    let redirect = "http://127.0.0.1:8888/callback";
    for path in [
      "/callback?code=a&code=b&state=ok",
      "/callback?code=a&state=ok#fragment",
      "http://user@127.0.0.1:8888/callback?code=a&state=ok",
      "not a url",
    ] {
      assert_eq!(callback_code(path, redirect, "ok").unwrap(), None);
    }
  }

  #[test]
  fn pkce_matches_rfc7636() {
    assert_eq!(
      challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
      "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    );
  }
  #[test]
  fn validates_callback_before_accepting_code() {
    let redirect = "http://127.0.0.1:8888/callback";
    assert_eq!(
      callback_code("/callback?code=a%2Bb&state=expected", redirect, "expected").unwrap(),
      Some("a+b".into())
    );
    assert_eq!(
      callback_code("/callback?code=stolen&state=wrong", redirect, "expected").unwrap(),
      None
    );
    assert!(
      callback_code(
        "/callback?error=access_denied&state=expected",
        redirect,
        "expected"
      )
      .is_err()
    );
    assert_eq!(
      callback_code("/favicon.ico", redirect, "expected").unwrap(),
      None
    );
    assert_eq!(
      callback_code(
        "http://example.com/callback?code=x&state=expected",
        redirect,
        "expected"
      )
      .unwrap(),
      None
    );
  }
  #[tokio::test]
  async fn listener_ignores_unrelated_requests() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let redirect = format!("http://127.0.0.1:{port}/callback");
    let server = tokio::spawn(async move { receive_callback(listener, &redirect, "ok").await });
    for path in [
      "/favicon.ico",
      "/callback?code=bad&state=wrong",
      "/callback?code=bad&state=ok&state=wrong",
      "/callback?code=accepted&state=ok",
    ] {
      let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .unwrap();
      stream
        .write_all(format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").as_bytes())
        .await
        .unwrap();
      let mut response = String::new();
      stream.read_to_string(&mut response).await.unwrap();
      assert!(
        response.starts_with(if path != "/callback?code=accepted&state=ok" {
          "HTTP/1.1 400"
        } else {
          "HTTP/1.1 200"
        })
      );
    }
    assert_eq!(server.await.unwrap().unwrap(), "accepted");
  }
}
