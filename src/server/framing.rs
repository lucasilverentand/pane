use anyhow::{bail, Result};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

/// Maximum frame size: 16 MiB. Prevents memory exhaustion from bad data.
const MAX_FRAME_SIZE: u32 = 16 * 1024 * 1024;

/// Write a length-prefixed frame to the stream.
pub async fn write_frame(stream: &mut UnixStream, data: &[u8]) -> std::io::Result<()> {
    let len = data.len() as u32;
    stream.write_all(&len.to_be_bytes()).await?;
    stream.write_all(data).await?;
    stream.flush().await?;
    Ok(())
}

/// Read a length-prefixed frame from the stream.
/// Returns `Ok(None)` on clean EOF.
pub async fn read_frame(stream: &mut UnixStream) -> std::io::Result<Option<Vec<u8>>> {
    let mut len_buf = [0u8; 4];
    match stream.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_be_bytes(len_buf);
    if len > MAX_FRAME_SIZE {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("frame too large: {} bytes", len),
        ));
    }
    let mut buf = vec![0u8; len as usize];
    stream.read_exact(&mut buf).await?;
    Ok(Some(buf))
}

/// Serialize a message as JSON and write it as a length-prefixed frame.
pub async fn send<T: Serialize>(stream: &mut UnixStream, msg: &T) -> Result<()> {
    let json = serde_json::to_vec(msg)?;
    write_frame(stream, &json).await?;
    Ok(())
}

/// Read a length-prefixed frame and deserialize it from JSON.
/// Returns `Ok(None)` on clean EOF.
pub async fn recv<T: DeserializeOwned>(stream: &mut UnixStream) -> Result<Option<T>> {
    match read_frame(stream).await? {
        Some(data) => {
            let msg = serde_json::from_slice(&data)?;
            Ok(Some(msg))
        }
        None => Ok(None),
    }
}

/// Read a length-prefixed frame and deserialize, returning an error on EOF.
pub async fn recv_required<T: DeserializeOwned>(stream: &mut UnixStream) -> Result<T> {
    match recv(stream).await? {
        Some(msg) => Ok(msg),
        None => bail!("connection closed unexpectedly"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_frame_roundtrip() {
        let (mut a, mut b) = UnixStream::pair().unwrap();
        let data = b"hello world";
        write_frame(&mut a, data).await.unwrap();
        let received = read_frame(&mut b).await.unwrap().unwrap();
        assert_eq!(received, data);
    }

    #[tokio::test]
    async fn test_empty_frame() {
        let (mut a, mut b) = UnixStream::pair().unwrap();
        write_frame(&mut a, b"").await.unwrap();
        let received = read_frame(&mut b).await.unwrap().unwrap();
        assert!(received.is_empty());
    }

    #[tokio::test]
    async fn test_multiple_frames() {
        let (mut a, mut b) = UnixStream::pair().unwrap();
        write_frame(&mut a, b"first").await.unwrap();
        write_frame(&mut a, b"second").await.unwrap();
        write_frame(&mut a, b"third").await.unwrap();

        let f1 = read_frame(&mut b).await.unwrap().unwrap();
        let f2 = read_frame(&mut b).await.unwrap().unwrap();
        let f3 = read_frame(&mut b).await.unwrap().unwrap();
        assert_eq!(f1, b"first");
        assert_eq!(f2, b"second");
        assert_eq!(f3, b"third");
    }

    #[tokio::test]
    async fn test_eof_returns_none() {
        let (a, mut b) = UnixStream::pair().unwrap();
        drop(a); // close the writer
        let result = read_frame(&mut b).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_send_recv_json_roundtrip() {
        use serde::{Deserialize, Serialize};

        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        struct TestMsg {
            name: String,
            value: u32,
        }

        let (mut a, mut b) = UnixStream::pair().unwrap();
        let msg = TestMsg {
            name: "test".to_string(),
            value: 42,
        };
        send(&mut a, &msg).await.unwrap();
        let received: TestMsg = recv_required(&mut b).await.unwrap();
        assert_eq!(received, msg);
    }

    #[tokio::test]
    async fn test_recv_eof_returns_none() {
        let (a, mut b) = UnixStream::pair().unwrap();
        drop(a);
        let result: Option<String> = recv(&mut b).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_recv_required_eof_returns_error() {
        let (a, mut b) = UnixStream::pair().unwrap();
        drop(a);
        let result: Result<String> = recv_required(&mut b).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_large_frame() {
        let (mut a, mut b) = UnixStream::pair().unwrap();
        let data = vec![0xABu8; 65536]; // 64 KiB
        // Must write and read concurrently: the socket buffer may be smaller
        // than 64 KiB, so write_all would block waiting for the reader to drain.
        let write_handle = tokio::spawn(async move {
            write_frame(&mut a, &data).await.unwrap();
        });
        let received = read_frame(&mut b).await.unwrap().unwrap();
        write_handle.await.unwrap();
        assert_eq!(received.len(), 65536);
        assert_eq!(received[0], 0xAB);
    }

    #[tokio::test]
    async fn test_protocol_messages_roundtrip() {
        use crate::server::protocol::{ClientRequest, ServerResponse};

        let (mut a, mut b) = UnixStream::pair().unwrap();

        let req = ClientRequest::Resize {
            width: 120,
            height: 40,
        };
        send(&mut a, &req).await.unwrap();
        let received: ClientRequest = recv_required(&mut b).await.unwrap();
        let json1 = serde_json::to_string(&req).unwrap();
        let json2 = serde_json::to_string(&received).unwrap();
        assert_eq!(json1, json2);

        let resp = ServerResponse::Error("test error".to_string());
        send(&mut b, &resp).await.unwrap();
        let received: ServerResponse = recv_required(&mut a).await.unwrap();
        let json1 = serde_json::to_string(&resp).unwrap();
        let json2 = serde_json::to_string(&received).unwrap();
        assert_eq!(json1, json2);
    }
}
