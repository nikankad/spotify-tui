//! Local HTTP fixtures used by authentication and transport regression tests.
use tokio::{
  io::{AsyncReadExt, AsyncWriteExt},
  net::TcpListener,
  task::JoinHandle,
};

pub async fn server(
  responses: Vec<(u16, &'static str, String)>,
) -> (String, JoinHandle<Vec<String>>) {
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let address = format!("http://{}", listener.local_addr().unwrap());
  let task = tokio::spawn(async move {
    let mut requests = Vec::new();
    for (status, headers, body) in responses {
      let (mut stream, _) =
        tokio::time::timeout(std::time::Duration::from_secs(5), listener.accept())
          .await
          .unwrap()
          .unwrap();
      let mut bytes = Vec::new();
      let mut chunk = [0; 4096];
      loop {
        let read = stream.read(&mut chunk).await.unwrap();
        assert_ne!(read, 0, "request ended before its body");
        bytes.extend_from_slice(&chunk[..read]);
        if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
          let header = String::from_utf8_lossy(&bytes[..end]);
          let length = header
            .lines()
            .find_map(|line| {
              let (name, value) = line.split_once(':')?;
              name
                .eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().unwrap())
            })
            .unwrap_or(0);
          if bytes.len() >= end + 4 + length {
            break;
          }
        }
      }
      requests.push(String::from_utf8(bytes).unwrap());
      let response = format!(
        "HTTP/1.1 {status} Test\r\n{headers}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
      );
      stream.write_all(response.as_bytes()).await.unwrap();
    }
    requests
  });
  (address, task)
}
