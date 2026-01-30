//! Request-response codec for the Nodalync protocol.
//!
//! This module implements the libp2p request-response codec for
//! sending and receiving Nodalync wire messages.

use async_trait::async_trait;
use futures::prelude::*;
use libp2p::request_response;
use nodalync_types::constants::MAX_MESSAGE_SIZE;
use std::io;

/// Protocol name for Nodalync request-response.
pub const PROTOCOL_NAME: &str = "/nodalync/1.0.0";

/// Request type for the request-response protocol.
#[derive(Debug, Clone)]
pub struct NodalyncRequest(pub Vec<u8>);

/// Response type for the request-response protocol.
#[derive(Debug, Clone)]
pub struct NodalyncResponse(pub Vec<u8>);

/// Codec for encoding/decoding Nodalync messages.
///
/// Uses length-prefixed framing: 4-byte big-endian length + payload.
#[derive(Debug, Clone, Default)]
pub struct NodalyncCodec;

impl NodalyncCodec {
    /// Create a new codec instance.
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl request_response::Codec for NodalyncCodec {
    type Protocol = &'static str;
    type Request = NodalyncRequest;
    type Response = NodalyncResponse;

    async fn read_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        let data = read_length_prefixed(io).await?;
        Ok(NodalyncRequest(data))
    }

    async fn read_response<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        let data = read_length_prefixed(io).await?;
        Ok(NodalyncResponse(data))
    }

    async fn write_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
        NodalyncRequest(data): Self::Request,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        write_length_prefixed(io, &data).await
    }

    async fn write_response<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
        NodalyncResponse(data): Self::Response,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        write_length_prefixed(io, &data).await
    }
}

/// Read a length-prefixed message from the stream.
async fn read_length_prefixed<T>(io: &mut T) -> io::Result<Vec<u8>>
where
    T: AsyncRead + Unpin + Send,
{
    // Read 4-byte big-endian length
    let mut len_buf = [0u8; 4];
    io.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;

    // Validate length
    if len > MAX_MESSAGE_SIZE as usize {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("message too large: {} > {}", len, MAX_MESSAGE_SIZE),
        ));
    }

    // Read payload
    let mut data = vec![0u8; len];
    io.read_exact(&mut data).await?;

    Ok(data)
}

/// Write a length-prefixed message to the stream.
async fn write_length_prefixed<T>(io: &mut T, data: &[u8]) -> io::Result<()>
where
    T: AsyncWrite + Unpin + Send,
{
    // Validate length
    if data.len() > MAX_MESSAGE_SIZE as usize {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("message too large: {} > {}", data.len(), MAX_MESSAGE_SIZE),
        ));
    }

    // Write 4-byte big-endian length
    let len_buf = (data.len() as u32).to_be_bytes();
    io.write_all(&len_buf).await?;

    // Write payload
    io.write_all(data).await?;
    io.flush().await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::io::Cursor;

    #[tokio::test]
    async fn test_length_prefixed_roundtrip() {
        let original = b"hello world";
        let mut buf = Vec::new();

        // Write
        write_length_prefixed(&mut buf, original).await.unwrap();

        // Verify format: 4-byte length + data
        assert_eq!(&buf[..4], &[0, 0, 0, 11]); // 11 bytes
        assert_eq!(&buf[4..], original);

        // Read back
        let mut cursor = Cursor::new(buf);
        let read_back = read_length_prefixed(&mut cursor).await.unwrap();
        assert_eq!(read_back, original);
    }

    #[tokio::test]
    async fn test_empty_message() {
        let original = b"";
        let mut buf = Vec::new();

        write_length_prefixed(&mut buf, original).await.unwrap();
        assert_eq!(&buf[..4], &[0, 0, 0, 0]); // 0 bytes

        let mut cursor = Cursor::new(buf);
        let read_back = read_length_prefixed(&mut cursor).await.unwrap();
        assert!(read_back.is_empty());
    }

    #[tokio::test]
    async fn test_request_response_types() {
        let req = NodalyncRequest(vec![1, 2, 3]);
        let resp = NodalyncResponse(vec![4, 5, 6]);

        assert_eq!(req.0, vec![1, 2, 3]);
        assert_eq!(resp.0, vec![4, 5, 6]);
    }
}
