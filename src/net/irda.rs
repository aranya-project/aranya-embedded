#![cfg(feature = "net-irda")]
//! This implements a networking interface over IrDA hardware provided by `esp_irda_transceiver`.
//! Calling [`start`] will give you a [`IrNetworkInterface`] instance that implements [`Network`].
//!
//! ## Theory of Operation
//!
//! A [`Message`]'s payload is split up into chunks with [`raptorq`], which adds redundancy to
//! allow reconstruction from damaged packets. Then each chunk is packaged in an [`IrPacket`] and
//! sent over the [`IrdaTransceiver`].
//!
//! On the receiving end, it reads the [`IrdaTransceiver`] until a valid packet header is found,
//! then the packet is read and given to an [`IrMessageReconstructor`], which collects packets
//! sent to this address until it can reconstruct the original [`Message`]. Once a packet is
//! successfully reconstructed, it is returned to the caller.
//!
//! ## [`IrPacket`] on-wire format
//!
//! The packet is a header followed by the payload bytes. The header is 12 bytes and looks like
//! this:
//!
//! ```
//! |  0  |  1  |  2  |  3  |  4  |  5  |  6  |  7        |  8  |  9  | 10  | 11  |
//! | magic           | recipient |  sender   | chunk_seq | chunk_len | total_len |
//! | F0h | 0Fh | F0h |    u16    |    u16    |    u8     |    u16    |    u16    |
//! ```
//!
//! The magic bytes are chosen to allow some dead time during transmission. If another device is
//! transmitting at the same time, that transmission might corrupt these bytes, causing it to be
//! ignored by receivers[^uart]. For more details on the rest of the packet, see the [`IrPacket`]
//! documentation.
//!
//! [^uart]: Because of various historical quirks of UART transmission, IrDA SIR transmits a 0 bit
//!          as a pulse and a 1 bit as no pulse. So a simultaneously transmitted 0 colliding with a
//!          1 will cause the 1 to flip to a 0.

use core::io::BorrowedBuf;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicU8, Ordering};

use alloc::{collections::btree_map::BTreeMap, vec::Vec};
use crc::{self, Crc};
use embassy_time::Timer;
use esp_irda_transceiver::{IrdaTransceiver, UartError};
use raptorq::{EncodingPacket, ObjectTransmissionInformation};

use super::{Message, Network, NetworkError};
use crate::util::SliceCursor;

type Mutex<T> =
    embassy_sync::mutex::Mutex<embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex, T>;

const CRC: Crc<u16> = Crc::<u16>::new(&crc::CRC_16_XMODEM); // XMODEM seems appropriate. :D
const IR_MAGIC: [u8; 3] = [0xF0, 0x0F, 0xF0];
const IR_CHUNK_SIZE: usize = 64; // needs to be less than 65536 because this will become the raptorq MTU which is u16
const IR_HEADER_SIZE: usize = 9; // recipient, sender, chunk_seq, chunk_len, total_len
const IR_CRC_SIZE: usize = (CRC.algorithm.width / 8) as usize;
const IR_REPAIR_PACKETS: usize = 3; // chosen arbitrarily. Should probably be determined dynamically by message size.
const RAPTORQ_OVERHEAD: usize = 4; // determined empirically - I don't know if there's a way to ask raptorq for this
const IR_PACKET_SIZE: usize =
    IR_MAGIC.len() + IR_HEADER_SIZE + IR_CHUNK_SIZE + IR_CRC_SIZE + RAPTORQ_OVERHEAD;

/// UART speed
const UART_BAUD_RATE: u64 = 115200;
/// How long it takes to transmit one byte (10 bit times) in microseconds
// This really should divide and take the ceiling but all we want is an upper bound and the
// integer math is less troublesome in const context.
const UART_BYTE_DELAY_US: u64 = 10 * (1_000_000 / UART_BAUD_RATE) + 1;

/// The minimum time to wait between packets.
const RANDOM_MIN: u32 = 100;
/// The distance between the minimum and maximum times to wait. Time between packets is then
/// unformly distributed between `RANDOM_MIN` and `RANDOM_MIN + RANDOM_SPREAD`.
const RANDOM_SPREAD: u32 = 200;
/// How long to wait to retry after a failed send.
const SEND_RETRY_DELAY_MS: u64 = 50;

#[derive(Debug, thiserror::Error)]
pub enum IrError {
    #[error("UART error: {0}")]
    Uart(#[from] UartError),
}

/// `IrPacket` is one link-layer packet on the IR interface
pub struct IrPacket {
    /// Recipient address.
    pub recipient: u16,
    /// Sender address.
    pub sender: u16,
    /// Identifier for this sequence of packets. All packets in the same message have the
    /// same `message_seq`.
    pub message_seq: u8,
    // `chunk_len` is never read but the field exists for documentary purposes and because it
    // will be used in the likely event we refactor to bincode 2.0.
    #[allow(dead_code)]
    /// Length of the `contents`` in this packet.
    pub chunk_len: u16,
    /// Total length of the message encoded by these packets.
    pub total_len: u16,
    /// The encoded payload of this packet.
    pub contents: heapless::Vec<u8, { IR_CHUNK_SIZE + RAPTORQ_OVERHEAD }>,
}

/// An IrMessageReconstructor consumes a series of packets to reconstruct the message
/// encoded within.
pub struct IrMessageReconstructor {
    decoder: raptorq::Decoder,
    message_seq: u8,
    total_len: u16,
    finished: bool,
}

impl IrMessageReconstructor {
    /// Create a new message reconstructor. The `initial_packet` argument is just used to
    /// set up some decoder parameters. You should still call [`add_packet`](Self::add_packet)
    /// after creating the reconstructor.
    pub fn new(initial_packet: &IrPacket) -> IrMessageReconstructor {
        IrMessageReconstructor {
            decoder: raptorq::Decoder::new(ObjectTransmissionInformation::with_defaults(
                initial_packet.total_len as u64,
                IR_CHUNK_SIZE as u16,
            )),
            message_seq: initial_packet.message_seq,
            total_len: initial_packet.total_len,
            finished: false,
        }
    }

    /// Add a packet to the reconstructor. When enough packets are added to reproduce the
    /// original message, this will return `Some(data)`. Until then it will return `None`.
    pub fn add_packet(&mut self, packet: IrPacket) -> Option<Vec<u8>> {
        if packet.message_seq != self.message_seq || packet.total_len != self.total_len {
            // sequence or length id different; this is a new packet sequence.
            // Reset our state.
            *self = IrMessageReconstructor::new(&packet);
        } else if self.finished {
            // We are done but this is part of a message we've already completed
            return None;
        }
        self.decoder
            .decode(EncodingPacket::deserialize(&packet.contents))
            .inspect(|_| self.finished = true)
    }
}

/// `IrNetworkInterface` manages turning a message into a series of packets and back again.
pub struct IrNetworkInterface<'a> {
    irts: Mutex<IrdaTransceiver<'a>>,
    my_address: u16,
    message_seq: AtomicU8,
    input_buf: Mutex<[u8; IR_CHUNK_SIZE + IR_CRC_SIZE + RAPTORQ_OVERHEAD]>,
    reconstructors: Mutex<BTreeMap<u16, IrMessageReconstructor>>,
}

impl<'o> IrNetworkInterface<'o> {
    /// Create a new `IrNetworkInterface`.
    fn new(mut irts: IrdaTransceiver<'o>, my_address: u16) -> IrNetworkInterface<'o> {
        irts.enable(true);
        IrNetworkInterface {
            irts: Mutex::new(irts),
            my_address,
            message_seq: AtomicU8::new(0),
            input_buf: Mutex::new([0u8; IR_CHUNK_SIZE + IR_CRC_SIZE + RAPTORQ_OVERHEAD]),
            reconstructors: Mutex::new(BTreeMap::new()),
        }
    }

    fn random_delay(crc: u16) -> u32 {
        let crc_extended: u32 = crc.into();
        RANDOM_MIN + crc_extended % RANDOM_SPREAD
    }

    /// Send a message to a recipient
    async fn send(&self, message: &[u8], recipient: u16) -> Result<bool, IrError> {
        let total_len = message.len();
        let chunk_seq = self.message_seq.fetch_add(1, Ordering::Relaxed);
        let encoder = raptorq::Encoder::with_defaults(message, IR_CHUNK_SIZE as u16);
        for packet in encoder.get_encoded_packets(IR_REPAIR_PACKETS as u32) {
            let mut output_buf: [MaybeUninit<u8>; IR_PACKET_SIZE] =
                [MaybeUninit::uninit(); IR_PACKET_SIZE];
            let mut bb = BorrowedBuf::from(&mut output_buf[..]);
            {
                let mut bc = bb.unfilled();
                // SAFETY: This shouldn't overflow as we should be writing at most `IR_PACKET_SIZE`
                // bytes.
                bc.append(&IR_MAGIC);
                bc.append(&u16::to_be_bytes(recipient));
                bc.append(&u16::to_be_bytes(self.my_address));
                bc.append(&u8::to_be_bytes(chunk_seq));
                let enc_packet = packet.serialize();
                bc.append(&u16::to_be_bytes(enc_packet.len() as u16));
                bc.append(&u16::to_be_bytes(total_len as u16));
                bc.append(&enc_packet);
            }
            let crc = CRC.checksum(&bb.filled()[3..]); // do not CRC magic bytes
            {
                let mut bc = bb.unfilled();
                bc.append(&u16::to_be_bytes(crc));
            }
            if !self.irts.lock().await.send(bb.filled()).await? {
                return Ok(false);
            }
            Timer::after_millis(Self::random_delay(crc) as u64).await;
        }
        Ok(true)
    }

    async fn wait_for_byte(&self) -> Result<u8, IrError> {
        let mut byte_buf = [0u8; 1];

        loop {
            let r = self.irts.lock().await.read(&mut byte_buf)?;
            if r > 0 {
                return Ok(byte_buf[0]);
            }
            Timer::after_micros(UART_BYTE_DELAY_US).await;
        }
    }

    /// Read data from the transceiver until we find a packet.
    async fn recv_packet(&self) -> Result<IrPacket, IrError> {
        let mut input_buf_guard = self.input_buf.lock().await;
        let input_buf = input_buf_guard.as_mut();

        let packet = loop {
            for b in &mut input_buf[0..3] {
                *b = self.wait_for_byte().await?;
            }
            while input_buf[0..3] != IR_MAGIC {
                input_buf[0] = input_buf[1];
                input_buf[1] = input_buf[2];
                input_buf[2] = self.wait_for_byte().await?;
            }
            let mut crc = CRC.digest();
            let mut irts = self.irts.lock().await;
            irts.read_all(&mut input_buf[0..IR_HEADER_SIZE]).await?;
            crc.update(&input_buf[0..IR_HEADER_SIZE]);
            let (sender, chunk_seq, chunk_len, total_len) = {
                let mut sc = SliceCursor::new(&input_buf[0..IR_HEADER_SIZE]);
                let recipient = sc.next_u16_be();
                if recipient != self.my_address {
                    log::info!("packet not for me; for {recipient}");
                    continue;
                }
                let sender = sc.next_u16_be();
                let chunk_seq = sc.next_u8();
                let chunk_len = sc.next_u16_be() as usize;
                if chunk_len > IR_CHUNK_SIZE + RAPTORQ_OVERHEAD {
                    log::info!("malformed chunk of size {chunk_len}");
                    continue;
                }
                let total_len = sc.next_u16_be();
                log::info!("chunk len {chunk_len} seq {chunk_seq}");

                assert_eq!(sc.remaining(), 0);

                (sender, chunk_seq, chunk_len, total_len)
            };

            irts.read_all(&mut input_buf[0..chunk_len + IR_CRC_SIZE])
                .await?;
            let checksum =
                u16::from_be_bytes(input_buf[chunk_len..chunk_len + 2].try_into().unwrap());
            crc.update(&input_buf[..chunk_len]);
            if crc.finalize() != checksum {
                log::error!("bad checksum");
                continue;
            }
            let contents = input_buf[..chunk_len].try_into().expect("packet too large");
            break IrPacket {
                recipient: self.my_address,
                sender,
                message_seq: chunk_seq,
                chunk_len: chunk_len as u16,
                total_len,
                contents,
            };
        };

        Ok(packet)
    }

    /// Read packets until we assemble a message, then return it
    async fn recv(&self) -> Result<Message<u16>, IrError> {
        loop {
            let packet = self.recv_packet().await?;
            let sender = packet.sender;
            let recipient = packet.recipient;
            let mut reconstructors = self.reconstructors.lock().await;
            let reconstructor = reconstructors
                .entry(sender)
                .or_insert(IrMessageReconstructor::new(&packet));
            if let Some(p) = reconstructor.add_packet(packet) {
                return Ok(Message {
                    recipient,
                    sender,
                    contents: p.into(),
                });
            }
        }
    }
}

impl Network for IrNetworkInterface<'_> {
    type Addr = u16;

    async fn send_message(&self, msg: Message<u16>) -> Result<(), NetworkError> {
        while !self
            .send(&msg.contents, msg.recipient)
            .await
            .map_err(|e| NetworkError::Send(alloc::format!("Send error: {e}")))?
        {
            Timer::after_millis(SEND_RETRY_DELAY_MS).await;
        }
        Ok(())
    }

    async fn recv_message(&self) -> Result<Message<u16>, NetworkError> {
        self.recv()
            .await
            .map_err(|e| NetworkError::Receive(alloc::format!("Receive Error: {e}")))
    }
}

/// Starts the IR networking interface and returns it.
pub async fn start(irts: IrdaTransceiver<'_>, my_address: u16) -> IrNetworkInterface<'_> {
    IrNetworkInterface::new(irts, my_address)
}
