use std::io::Cursor;

use protocol::wire::middleware::pipeline::{self, Pipeline};
use protocol::{Error, Parcel, Settings};
use tokio::io::{AsyncRead, AsyncWrite};

use super::transport::{Simple, Transport};

/// A stream-based connection.
#[derive(Debug)]
pub struct Connection<P: Parcel, S: AsyncRead + AsyncWrite + Send + Unpin> {
    pub stream: S,
    pub transport: Simple,
    pub middleware: pipeline::Default,
    pub settings: Settings,

    pub _parcel: std::marker::PhantomData<P>,
}

impl<P, S> Connection<P, S>
where
    P: Parcel,
    S: AsyncRead + AsyncWrite + Send + Unpin,
{
    /// Creates a new connection.
    pub fn new(stream: S, settings: Settings) -> Self {
        Self {
            stream,
            transport: Simple::new(),
            middleware: pipeline::default(),
            settings,
            _parcel: std::marker::PhantomData,
        }
    }

    /// Attempts to receive a packet.
    pub async fn receive_packet(&mut self) -> Result<Option<P>, Error> {
        self.transport
            .process_data(&mut self.stream, &self.settings)
            .await?;
        if let Some(raw_packet) = self.transport.receive_raw_packet().await? {
            let mut packet_data = Cursor::new(self.middleware.decode_data(raw_packet)?);
            let packet = P::read(&mut packet_data, &self.settings)?;
            Ok(Some(packet))
        } else {
            Ok(None)
        }
    }

    /// Sends a packet.
    pub async fn send_packet(&mut self, packet: &P) -> Result<(), Error> {
        let raw_packet = self
            .middleware
            .encode_data(packet.raw_bytes(&self.settings)?)?;
        self.transport
            .send_raw_packet(&mut self.stream, &raw_packet, &self.settings)
            .await
    }

    pub fn into_inner(self) -> S {
        self.stream
    }
}
