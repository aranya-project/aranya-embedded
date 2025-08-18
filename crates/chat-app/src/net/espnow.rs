#![cfg(feature = "net-esp-now")]
//! This implements a networking interface over EspNow hardware.
//! Calling [`start`] will give you a [`EspNowNetworkInterface`] instance that implements [`Network`].
//!
//! ## Theory of Operation
//!
//! A [`Message`]'s payload is split up into chunks with [`raptorq`], which adds redundancy to
//! allow reconstruction from damaged packets. Then each chunk is packaged in an [`EspNowPacket`] and
//! sent over the EspNow Interface.
//!
//! On the receiving end, it reads the Esp Now interface until a valid packet header is found,
//! then the packet is read and given to an [`EspNowMessageReconstructor`], which collects packets
//! sent to this address until it can reconstruct the original [`Message`]. Once a packet is
//! successfully reconstructed, it is returned to the caller.
//!
//! ## [`EspNowPacket`] on-wire format
//!
//! The packet is a header followed by the payload bytes and finally by a 16-bit CRC. The header
//! is 12 bytes and looks like this:
//!
//! ```
//! |  0  |  1  |  2  |  3  |  4  |  5  |  6  |  7          |  8  |  9  | 10  | 11  |
//! | magic           | recipient |  sender   | message_seq | chunk_len | total_len |
//! | F0h | 0Fh | F0h |    u16    |    u16    |      u8     |    u16    |    u16    |
//! ```
//!

use alloc::{collections::btree_map::BTreeMap, vec::Vec};
use core::{
    io::BorrowedBuf,
    mem::MaybeUninit,
    sync::atomic::{AtomicU32, Ordering},
};

use crc::{self, Crc};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_time::{Instant, Timer};
use esp_hal::gpio::Output;
use esp_wifi::esp_now::{EspNowReceiver, EspNowSender, BROADCAST_ADDRESS};
use raptorq::{EncodingPacket, ObjectTransmissionInformation};

use super::{Message, NetworkEngine, NetworkError, NetworkInterface};
use crate::{mk_static, util::SliceCursor};

const ESP_NOW_PACKET_QUEUE_SIZE: usize = 2;
type Mutex<T> = embassy_sync::mutex::Mutex<CriticalSectionRawMutex, T>;
type Channel<T> =
    embassy_sync::channel::Channel<CriticalSectionRawMutex, T, ESP_NOW_PACKET_QUEUE_SIZE>;
type Sender<'a, T> =
    embassy_sync::channel::Sender<'a, CriticalSectionRawMutex, T, ESP_NOW_PACKET_QUEUE_SIZE>;
type Receiver<'a, T> =
    embassy_sync::channel::Receiver<'a, CriticalSectionRawMutex, T, ESP_NOW_PACKET_QUEUE_SIZE>;

const CRC: Crc<u16> = Crc::<u16>::new(&crc::CRC_16_XMODEM); // XMODEM seems appropriate. :D
const ESP_NOW_MAGIC: [u8; 3] = [0xF0, 0x0F, 0xF0];
const ESP_NOW_CHUNK_SIZE: usize = 64; // needs to be less than 65536 because this will become the raptorq MTU which is u16
const ESP_NOW_HEADER_SIZE: usize = 9; // recipient, sender, chunk_seq, chunk_len, total_len
const ESP_NOW_CRC_SIZE: usize = (CRC.algorithm.width / 8) as usize;
const RAPTORQ_OVERHEAD: usize = 4; // determined empirically - I don't know if there's a way to ask raptorq for this
const ESP_NOW_PACKET_SIZE: usize = ESP_NOW_MAGIC.len()
    + ESP_NOW_HEADER_SIZE
    + ESP_NOW_CHUNK_SIZE
    + ESP_NOW_CRC_SIZE
    + RAPTORQ_OVERHEAD;

/// The minimum time to wait between packets.
const RANDOM_MIN: u32 = 25;
/// The distance between the minimum and maximum times to wait. Time between packets is then
/// unformly distributed between `RANDOM_MIN` and `RANDOM_MIN + RANDOM_SPREAD`.
const RANDOM_SPREAD: u32 = 100;
/// How long to wait to retry after a failed send.
const SEND_RETRY_DELAY_MS: u64 = 50;

#[derive(Debug, thiserror::Error)]
pub enum EspNowError {
    #[error("EspNow Error")]
    EspNow,
}

/// `EspNowPacket` is one link-layer packet on the ESP Now interface
pub struct EspNowPacket {
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
    pub contents: heapless::Vec<u8, { ESP_NOW_CHUNK_SIZE + RAPTORQ_OVERHEAD }>,
}

/// An EspNowMessageReconstructor consumes a series of packets to reconstruct the message
/// encoded within.
pub struct EspNowMessageReconstructor {
    decoder: raptorq::Decoder,
    message_seq: u8,
    total_len: u16,
    packets_recvd: usize,
    finished: bool,
}

impl EspNowMessageReconstructor {
    /// Create a new message reconstructor. The `initial_packet` argument is just used to
    /// set up some decoder parameters. You should still call [`add_packet`](Self::add_packet)
    /// after creating the reconstructor.
    pub fn new(initial_packet: &EspNowPacket) -> EspNowMessageReconstructor {
        EspNowMessageReconstructor {
            decoder: raptorq::Decoder::new(ObjectTransmissionInformation::with_defaults(
                initial_packet.total_len as u64,
                ESP_NOW_CHUNK_SIZE as u16,
            )),
            message_seq: initial_packet.message_seq,
            total_len: initial_packet.total_len,
            packets_recvd: 0,
            finished: false,
        }
    }

    /// Add a packet to the reconstructor. When enough packets are added to reproduce the
    /// original message, this will return `Some(data)`. Until then it will return `None`.
    pub fn add_packet(&mut self, packet: EspNowPacket) -> Option<Vec<u8>> {
        if packet.message_seq != self.message_seq || packet.total_len != self.total_len {
            // sequence or length id different; this is a new packet sequence.
            // Reset our state.
            if !self.finished {
                log::info!(
                    "reconstructor reset with {}/{} est. packets",
                    self.packets_recvd,
                    (self.total_len - 1) / ESP_NOW_CHUNK_SIZE as u16 + 1
                );
            }
            *self = EspNowMessageReconstructor::new(&packet);
        } else if self.finished {
            // We are done but this is part of a message we've already completed
            return None;
        }
        self.packets_recvd += 1;
        self.decoder
            .decode(EncodingPacket::deserialize(&packet.contents))
            .inspect(|_| self.finished = true)
    }
}

/// `EspNowNetworkEngine` manages turning a message into a series of packets and back again.
pub(crate) struct EspNowNetworkEngine<'a> {
    sender: Mutex<EspNowSender<'a>>,
    receiver: Mutex<EspNowReceiver<'a>>,
    my_address: u16,
    send_channel: Channel<EspNowPacket>,
    receive_channel: Channel<EspNowPacket>,
    last_rx: AtomicU32,
    leds: Mutex<(Output<'a>, Output<'a>)>, // tx, rx
}

impl<'o> EspNowNetworkEngine<'o> {
    /// Create a new `EspNowNetworkInterface`.
    fn new(
        sender: Mutex<EspNowSender<'o>>,
        receiver: Mutex<EspNowReceiver<'o>>,
        my_address: u16,
        tx_led: Output<'o>,
        rx_led: Output<'o>,
    ) -> EspNowNetworkEngine<'o> {
        EspNowNetworkEngine {
            sender,
            receiver,
            my_address,
            send_channel: Channel::new(),
            receive_channel: Channel::new(),
            last_rx: AtomicU32::new(0),
            leds: Mutex::new((tx_led, rx_led)),
        }
    }

    fn random_delay(crc: u16) -> u32 {
        let crc_extended: u32 = crc.into();
        RANDOM_MIN + crc_extended % RANDOM_SPREAD
    }

    /// Send a message to a recipient
    async fn send_packet(&self, packet: EspNowPacket) -> Result<u16, EspNowError> {
        self.leds.lock().await.0.set_high();

        let mut output_buf: [MaybeUninit<u8>; ESP_NOW_PACKET_SIZE] =
            [MaybeUninit::uninit(); ESP_NOW_PACKET_SIZE];
        let mut bb = BorrowedBuf::from(&mut output_buf[..]);
        {
            let mut bc = bb.unfilled();
            // SAFETY: This shouldn't overflow as we should be writing at most `ESP_NOW_PACKET_SIZE`
            // bytes.
            bc.append(&ESP_NOW_MAGIC);
            bc.append(&u16::to_be_bytes(packet.recipient));
            bc.append(&u16::to_be_bytes(self.my_address));
            bc.append(&u8::to_be_bytes(packet.message_seq));
            bc.append(&u16::to_be_bytes(packet.chunk_len));
            bc.append(&u16::to_be_bytes(packet.total_len));
            bc.append(&packet.contents);
        }
        let crc = CRC.checksum(&bb.filled()[3..]); // do not CRC magic bytes
        {
            let mut bc = bb.unfilled();
            bc.append(&u16::to_be_bytes(crc));
        }

        self.sender
            .lock()
            .await
            .send_async(&BROADCAST_ADDRESS, bb.filled())
            .await
            .map_err(|_| EspNowError::EspNow)?;

        Ok(crc)
    }

    fn update_last_rx(&self) {
        self.last_rx
            .store(Instant::now().as_ticks() as u32, Ordering::Relaxed);
    }

    /// Read data from the transceiver until we find a packet.
    async fn recv_packet(&self) -> Result<EspNowPacket, EspNowError> {
        loop {
            let received = self.receiver.lock().await.receive_async().await;
            self.leds.lock().await.1.set_high();
            let receive_data = received.data();
            log::debug!("EspNow: reseave info {:?}", received.info);

            if receive_data[0..3] != ESP_NOW_MAGIC {
                log::debug!("EspNow: magic did not match {:?}", &receive_data[0..3]);
                continue;
            }

            let input_buf = &receive_data[3..];
            let mut crc = CRC.digest();
            self.update_last_rx();
            crc.update(&input_buf[0..ESP_NOW_HEADER_SIZE]);
            let (sender, chunk_seq, chunk_len, total_len) = {
                let mut sc = SliceCursor::new(&input_buf[0..ESP_NOW_HEADER_SIZE]);
                let recipient = sc.next_u16_be();
                if recipient != self.my_address && recipient != EspNowNetworkInterface::BROADCAST {
                    log::debug!(
                        "recv_packet: packet not for me (address: {}); for {} ",
                        self.my_address,
                        recipient
                    );
                    continue;
                }
                let sender = sc.next_u16_be();
                let chunk_seq = sc.next_u8();
                let chunk_len = sc.next_u16_be() as usize;
                if chunk_len > ESP_NOW_CHUNK_SIZE + RAPTORQ_OVERHEAD {
                    log::info!("recv_packet: malformed chunk of size {chunk_len}");
                    continue;
                }
                let total_len = sc.next_u16_be();
                // log::info!("recv_packet: chunk len {chunk_len} seq {chunk_seq}");

                assert_eq!(sc.remaining(), 0);

                (sender, chunk_seq, chunk_len, total_len)
            };

            let input_buf = &input_buf[ESP_NOW_HEADER_SIZE..];
            self.update_last_rx();
            let checksum =
                u16::from_be_bytes(input_buf[chunk_len..chunk_len + 2].try_into().unwrap());
            crc.update(&input_buf[..chunk_len]);
            if crc.finalize() != checksum {
                log::error!("bad checksum");
                continue;
            }
            let contents = input_buf[..chunk_len].try_into().expect("packet too large");
            return Ok(EspNowPacket {
                recipient: self.my_address,
                sender,
                message_seq: chunk_seq,
                chunk_len: chunk_len as u16,
                total_len,
                contents,
            });
        }
    }

    pub fn interface(&self) -> EspNowNetworkInterface<'_> {
        EspNowNetworkInterface {
            send_tx: self.send_channel.sender(),
            receive_rx: self.receive_channel.receiver(),
            my_address: self.my_address,
            message_seq: 0,
            reconstructors: BTreeMap::new(),
        }
    }

    async fn run_sender(&self) -> ! {
        loop {
            log::debug!("EspNow: Waiting for Packet");
            let packet = self.send_channel.receive().await;
            log::debug!("EspNow: Got Packet");

            match self.send_packet(packet).await {
                Ok(crc) => {
                    self.leds.lock().await.0.set_low();
                    Timer::after_millis(Self::random_delay(crc) as u64).await;
                }
                Err(e) => {
                    self.leds.lock().await.0.set_low();
                    log::error!("EspNow: send error: {e}");
                    Timer::after_millis(SEND_RETRY_DELAY_MS).await;
                }
            }
        }
    }

    async fn run_receiver(&self) -> ! {
        loop {
            match self.recv_packet().await {
                Ok(packet) => self.receive_channel.send(packet).await,
                Err(e) => {
                    log::error!("EspNow: recv error: {e}");
                }
            }
            self.leds.lock().await.1.set_low();
        }
    }
}

#[embassy_executor::task]
async fn run_esp_now_engine(engine: &'static EspNowNetworkEngine<'static>) -> ! {
    embassy_futures::join::join(engine.run_receiver(), engine.run_sender()).await;
    // This tells the compiler to not worry about the return type
    unreachable!();
}

impl NetworkEngine for EspNowNetworkEngine<'_> {
    fn run(&'static self, spawner: embassy_executor::Spawner) -> Result<(), NetworkError> {
        spawner
            .spawn(run_esp_now_engine(self))
            .expect("could not spawn ESP Now receiver");
        Ok(())
    }
}

pub struct EspNowNetworkInterface<'a> {
    send_tx: Sender<'a, EspNowPacket>,
    receive_rx: Receiver<'a, EspNowPacket>,
    my_address: u16,
    message_seq: u8,
    reconstructors: BTreeMap<u16, EspNowMessageReconstructor>,
}

impl EspNowNetworkInterface<'_> {
    /// Send a message to a recipient
    async fn send(&mut self, msg: Message<u16>) -> Result<(), EspNowError> {
        let total_len = msg.contents.len();
        let (message_seq, _) = self.message_seq.overflowing_add(1);
        self.message_seq = message_seq;
        let encoder = raptorq::Encoder::with_defaults(&msg.contents, ESP_NOW_CHUNK_SIZE as u16);
        let repair_packets = (total_len / ESP_NOW_CHUNK_SIZE) * 12 / 10; // 20% extra packets
        for packet in encoder.get_encoded_packets(repair_packets as u32) {
            let enc_packet = packet.serialize();
            let packet = EspNowPacket {
                recipient: msg.recipient,
                sender: self.my_address,
                message_seq,
                chunk_len: enc_packet.len() as u16,
                total_len: total_len as u16,
                contents: enc_packet.into_iter().collect(),
            };
            log::debug!("EspNow: Sending Packet");
            self.send_tx.send(packet).await;
            log::debug!("EspNow: Sent Packet");
        }
        Ok(())
    }

    /// Read packets until we assemble a message, then return it
    async fn recv(&mut self) -> Result<Message<u16>, EspNowError> {
        loop {
            log::debug!("EspNow: Waiting for Packet");
            let packet = self.receive_rx.receive().await;
            log::debug!("EspNow: Received Packet");

            let sender = packet.sender;
            let recipient = packet.recipient;
            let reconstructor = self
                .reconstructors
                .entry(sender)
                .or_insert(EspNowMessageReconstructor::new(&packet));
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

impl NetworkInterface for EspNowNetworkInterface<'_> {
    type Addr = u16;
    const BROADCAST: Self::Addr = 0;

    async fn send_message(&mut self, msg: Message<u16>) -> Result<(), NetworkError> {
        match self.send(msg).await {
            Ok(_) => (),
            Err(e) => log::error!("esp now send: {e}"),
        }
        Ok(())
    }

    async fn recv_message(&mut self) -> Result<Message<u16>, NetworkError> {
        let msg = self
            .recv()
            .await
            .map_err(|e| NetworkError::Receive(alloc::format!("esp now recv: {e}")))?;
        Ok(msg)
    }

    fn my_address(&self) -> Self::Addr {
        self.my_address
    }
}

/// Starts the Esp Now networking engine and returns and interface to it.
pub(crate) async fn start(
    sender: Mutex<EspNowSender<'static>>,
    receiver: Mutex<EspNowReceiver<'static>>,
    my_address: u16,
    tx_led: Output<'static>,
    rx_led: Output<'static>,
) -> &'static EspNowNetworkEngine<'static> {
    mk_static!(
        EspNowNetworkEngine,
        EspNowNetworkEngine::new(sender, receiver, my_address, tx_led, rx_led)
    )
}
