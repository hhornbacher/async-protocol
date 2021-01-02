use std::collections::VecDeque;
use std::io::Cursor;
use std::mem;

use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use protocol::{Error, Parcel, Settings};

#[async_trait]
pub trait Transport {
    async fn process_data<R: AsyncRead + Send + Unpin>(
        &mut self,
        read: &mut R,
        settings: &Settings,
    ) -> Result<(), Error>;

    async fn receive_raw_packet(&mut self) -> Result<Option<Vec<u8>>, Error>;

    async fn send_raw_packet<W: AsyncWrite + Send + Unpin>(
        &mut self,
        write: &mut W,
        packet: &[u8],
        settings: &Settings,
    ) -> Result<(), Error>;
}

/// The type that we use to describe packet sizes.
pub type PacketSize = u32;

/// The current state.
#[derive(Clone, Debug)]
enum State {
    /// We are awaiting packet size bytes.
    AwaitingSize(Vec<u8>),
    AwaitingPacket {
        size: PacketSize,
        received_data: Vec<u8>,
    },
}

/// A simple transport.
#[derive(Clone, Debug)]
pub struct Simple {
    state: State,
    packets: VecDeque<Vec<u8>>,
}

impl Simple {
    pub fn new() -> Self {
        Simple {
            state: State::AwaitingSize(Vec::new()),
            packets: VecDeque::new(),
        }
    }

    async fn process_bytes(&mut self, bytes: &[u8], settings: &Settings) -> Result<(), Error> {
        let mut read = Cursor::new(bytes);

        loop {
            match self.state.clone() {
                State::AwaitingSize(mut size_bytes) => {
                    let remaining_bytes = mem::size_of::<PacketSize>() - size_bytes.len();

                    let mut received_bytes = vec![0; remaining_bytes];
                    let bytes_read = std::io::Read::read(&mut read, &mut received_bytes)?;
                    received_bytes.drain(bytes_read..);

                    assert_eq!(received_bytes.len(), bytes_read);

                    size_bytes.extend(received_bytes.into_iter());

                    if size_bytes.len() == mem::size_of::<PacketSize>() {
                        let mut size_buffer = Cursor::new(size_bytes);

                        let size = PacketSize::read(&mut size_buffer, settings).unwrap();

                        // We are now ready to receive packet data.
                        self.state = State::AwaitingPacket {
                            size,
                            received_data: Vec::new(),
                        }
                    } else {
                        // Still waiting to receive the whole packet.
                        self.state = State::AwaitingSize(size_bytes);
                        break;
                    }
                }
                State::AwaitingPacket {
                    size,
                    mut received_data,
                } => {
                    let remaining_bytes = (size as usize) - received_data.len();
                    assert!(remaining_bytes > 0);

                    let mut received_bytes = vec![0; remaining_bytes];
                    let bytes_read = read.read(&mut received_bytes).await?;
                    received_bytes.drain(bytes_read..);

                    assert_eq!(received_bytes.len(), bytes_read);

                    received_data.extend(received_bytes.into_iter());

                    assert!(received_data.len() <= (size as usize));

                    if (size as usize) == received_data.len() {
                        self.packets.push_back(received_data);

                        // Start reading the next packet.
                        self.state = State::AwaitingSize(Vec::new());
                    } else {
                        // Keep reading the current packet.
                        self.state = State::AwaitingPacket {
                            size,
                            received_data,
                        };
                        break;
                    }
                }
            }
        }

        Ok(())
    }
}

const BUFFER_SIZE: usize = 10000;

#[async_trait]
impl Transport for Simple {
    async fn process_data<R: AsyncRead + Send + Unpin>(
        &mut self,
        read: &mut R,
        settings: &Settings,
    ) -> Result<(), Error> {
        // Load the data into a temporary buffer before we process it.
        loop {
            let mut buffer = [0u8; BUFFER_SIZE];
            let bytes_read = read.read(&mut buffer).await.unwrap();
            let buffer = &buffer[0..bytes_read];

            if bytes_read == 0 {
                break;
            } else {
                self.process_bytes(buffer, settings).await?;

                // We didn't fill the whole buffer so stop now.
                if bytes_read != BUFFER_SIZE {
                    break;
                }
            }
        }

        Ok(())
    }

    async fn send_raw_packet<W: AsyncWrite + Send + Unpin>(
        &mut self,
        write: &mut W,
        packet: &[u8],
        settings: &Settings,
    ) -> Result<(), Error> {
        let mut w = Cursor::new(Vec::<u8>::new());
        // Prefix the packet size.
        (packet.len() as PacketSize).write(&mut w, settings)?;
        // Write the packet data.
        w.write_all(&packet).await?;

        write.write(&w.into_inner()).await?;

        Ok(())
    }

    async fn receive_raw_packet(&mut self) -> Result<Option<Vec<u8>>, Error> {
        Ok(self.packets.pop_front())
    }
}
