use std::io::Cursor;

use protocol::wire::middleware::pipeline::{self, Pipeline};
use protocol::{Error, Parcel, Settings};
use tokio::io::{split, AsyncRead, AsyncWrite, ReadHalf, WriteHalf};

use super::transport::{Simple, Transport};

async fn receive_packet<P: Parcel, S: AsyncRead + Send + Unpin>(
    transport: &mut Simple,
    stream: &mut S,
    settings: &Settings,
    middleware: &mut pipeline::Default,
) -> Result<Option<P>, Error> {
    transport.process_data(stream, &settings).await?;

    if let Some(raw_packet) = transport.receive_raw_packet().await? {
        let mut packet_data = Cursor::new(middleware.decode_data(raw_packet)?);

        let packet = P::read(&mut packet_data, settings)?;

        Ok(Some(packet))
    } else {
        Ok(None)
    }
}

async fn send_packet<P: Parcel, S: AsyncWrite + Send + Unpin>(
    transport: &mut Simple,
    stream: &mut S,
    settings: &Settings,
    middleware: &mut pipeline::Default,
    packet: &P,
) -> Result<(), Error> {
    let raw_packet = middleware.encode_data(packet.raw_bytes(settings)?)?;
    transport
        .send_raw_packet(stream, &raw_packet, settings)
        .await
}

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
        receive_packet(
            &mut self.transport,
            &mut self.stream,
            &self.settings,
            &mut self.middleware,
        )
        .await
    }

    /// Sends a packet.
    pub async fn send_packet(&mut self, packet: &P) -> Result<(), Error> {
        send_packet(
            &mut self.transport,
            &mut self.stream,
            &self.settings,
            &mut self.middleware,
            packet,
        )
        .await
    }

    pub fn into_inner(self) -> S {
        self.stream
    }

    pub fn split(self) -> (ReceiveConnection<P, S>, SendConnection<P, S>) {
        let settings = self.settings.clone();
        let (receiver, sender) = split(self.into_inner());

        (
            ReceiveConnection::new(receiver, settings.clone()),
            SendConnection::new(sender, settings),
        )
    }
}

/// A stream-based connection.
#[derive(Debug)]
pub struct SendConnection<P: Parcel, S: AsyncWrite + Send + Unpin> {
    pub writer: WriteHalf<S>,
    pub transport: Simple,
    pub middleware: pipeline::Default,
    pub settings: Settings,

    pub _parcel: std::marker::PhantomData<P>,
}

impl<P, S> SendConnection<P, S>
where
    P: Parcel,
    S: AsyncWrite + Send + Unpin,
{
    /// Creates a new connection.
    pub fn new(writer: WriteHalf<S>, settings: Settings) -> Self {
        Self {
            writer,
            transport: Simple::new(),
            middleware: pipeline::default(),
            settings,
            _parcel: std::marker::PhantomData,
        }
    }

    /// Sends a packet.
    pub async fn send_packet(&mut self, packet: &P) -> Result<(), Error> {
        send_packet(
            &mut self.transport,
            &mut self.writer,
            &self.settings,
            &mut self.middleware,
            packet,
        )
        .await
    }

    pub fn into_inner(self) -> WriteHalf<S> {
        self.writer
    }
}

/// A stream-based connection.
#[derive(Debug)]
pub struct ReceiveConnection<P: Parcel, S: AsyncRead + Send + Unpin> {
    pub reader: ReadHalf<S>,
    pub transport: Simple,
    pub middleware: pipeline::Default,
    pub settings: Settings,

    pub _parcel: std::marker::PhantomData<P>,
}

impl<P, S> ReceiveConnection<P, S>
where
    P: Parcel,
    S: AsyncRead + Send + Unpin,
{
    /// Creates a new connection.
    pub fn new(reader: ReadHalf<S>, settings: Settings) -> Self {
        Self {
            reader,
            transport: Simple::new(),
            middleware: pipeline::default(),
            settings,
            _parcel: std::marker::PhantomData,
        }
    }

    /// Attempts to receive a packet.
    pub async fn receive_packet(&mut self) -> Result<Option<P>, Error> {
        receive_packet(
            &mut self.transport,
            &mut self.reader,
            &self.settings,
            &mut self.middleware,
        )
        .await
    }

    pub fn into_inner(self) -> ReadHalf<S> {
        self.reader
    }
}
