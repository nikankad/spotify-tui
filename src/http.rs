//! Shared HTTPS policy and bounded response buffering.
use anyhow::{Result, bail};
use reqwest::{Client, Response, redirect::Policy};
use std::time::Duration;

pub const API_BODY_LIMIT: usize = 8 * 1024 * 1024;
pub const TOKEN_BODY_LIMIT: usize = 64 * 1024;

pub fn builder() -> reqwest::ClientBuilder {
  Client::builder()
    .https_only(true)
    .redirect(Policy::none())
    .connect_timeout(Duration::from_secs(10))
    .timeout(Duration::from_secs(30))
    .pool_max_idle_per_host(2)
}
pub fn client() -> Result<Client> {
  Ok(builder().build()?)
}

pub async fn body(mut response: Response, limit: usize) -> Result<Vec<u8>> {
  if response
    .content_length()
    .is_some_and(|length| length > limit as u64)
  {
    bail!("Spotify response exceeds the {limit}-byte limit");
  }
  let mut body = Vec::new();
  while let Some(chunk) = response.chunk().await? {
    if chunk.len() > limit.saturating_sub(body.len()) {
      bail!("Spotify response exceeds the {limit}-byte limit");
    }
    body.extend_from_slice(&chunk);
  }
  Ok(body)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::spotify::test_support::server;

  #[tokio::test]
  async fn rejects_oversized_responses_before_buffering() {
    let (url, task) = server(vec![(200, "", "x".repeat(32))]).await;
    let response = builder()
      .https_only(false)
      .no_proxy()
      .build()
      .unwrap()
      .get(url)
      .send()
      .await
      .unwrap();
    assert!(
      body(response, 16)
        .await
        .unwrap_err()
        .to_string()
        .contains("limit")
    );
    task.await.unwrap();
  }

  #[tokio::test]
  async fn rejects_oversized_chunked_responses_without_a_content_length() {
    use tokio::{
      io::{AsyncReadExt, AsyncWriteExt},
      net::TcpListener,
    };
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
      let (mut socket, _) = listener.accept().await.unwrap();
      let mut request = [0; 2048];
      assert!(socket.read(&mut request).await.unwrap() > 0);
      socket.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n9\r\n123456789\r\n9\r\n123456789\r\n0\r\n\r\n").await.unwrap();
    });
    let response = builder()
      .https_only(false)
      .no_proxy()
      .build()
      .unwrap()
      .get(url)
      .send()
      .await
      .unwrap();
    assert!(
      body(response, 16)
        .await
        .unwrap_err()
        .to_string()
        .contains("limit")
    );
    task.await.unwrap();
  }

  #[tokio::test]
  async fn never_follows_redirects_with_token_form_data() {
    let (url, task) = server(vec![(
      307,
      "Location: http://127.0.0.1:1/token-thief\r\n",
      "".into(),
    )])
    .await;
    let response = builder()
      .https_only(false)
      .no_proxy()
      .build()
      .unwrap()
      .post(url)
      .form(&[("refresh_token", "private-test-token")])
      .send()
      .await
      .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(task.await.unwrap().len(), 1);
  }

  #[tokio::test]
  async fn production_transport_rejects_plain_http() {
    let error = client()
      .unwrap()
      .get("http://127.0.0.1:1")
      .send()
      .await
      .unwrap_err();
    assert!(error.is_builder());
  }
}
