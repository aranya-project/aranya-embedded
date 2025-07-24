#![no_std]
#![feature(iter_array_chunks)]

extern crate alloc;

mod ir;

use alloc::sync::Arc;

use embassy_sync::blocking_mutex::CriticalSectionMutex;
use esp_hal::{
    gpio::{
        interconnect::{PeripheralInput, PeripheralOutput},
        Level, Output, OutputPin,
    },
    peripheral::{Peripheral, PeripheralRef},
    uart::{self, AnyUart, Error as UartError, Instance, Uart, UartRx, UartTx},
    Async,
};

pub use self::ir::IrUart;

/// An `IrdaTransceiver` wraps the UART connected to an IrDA transceiver and manages the
/// physical layer of sending and receiving bytes.
pub struct IrdaTransceiver<'d> {
    ir_tx: IrdaTransmitter<'d>,
    ir_rx: IrdaReceiver<'d>,
}

impl<'a> IrdaTransceiver<'a> {
    /// Create a new `IrdaTransceiver`
    pub fn new(
        uart: impl Peripheral<P = impl uart::Instance>,
        tx: impl Peripheral<P = impl PeripheralOutput> + 'a,
        rx: impl Peripheral<P = impl PeripheralInput> + 'a,
        en: impl Peripheral<P = impl OutputPin> + 'a,
    ) -> IrdaTransceiver<'a> {
        let peripheral = uart.map_into().into_ref();
        // SAFETY: We only change registers between UART calls, and we
        // only have the cloned peripheral for the UART we were given.
        let peripheral_for_irda = unsafe { peripheral.clone_unchecked() };
        let en_driver = Output::new(en, Level::High);

        let config = uart::Config::default()
            .with_baudrate(115200)
            .with_rx_fifo_full_threshold(8);
        let (uart_rx, uart_tx) = Uart::new(peripheral, config)
            .unwrap()
            .with_tx(tx)
            .with_rx(rx)
            .into_async()
            .split();

        peripheral_for_irda.set_irda_mode(true);
        let peripheral = Arc::new(CriticalSectionMutex::new(peripheral_for_irda));

        let obj = IrdaTransceiver {
            ir_tx: IrdaTransmitter {
                peripheral: Arc::clone(&peripheral),
                uart_tx,
                en_driver,
            },
            ir_rx: IrdaReceiver {
                peripheral: peripheral,
                uart_rx,
            },
        };
        // TODO(chip): enable collision detection
        //obj.set_irda_duplex(true);

        obj
    }

    /// Switch the transceiver's enable pin.
    ///
    /// When not enabled, the transceiver is turned off and cannot transmit or receive. This
    /// state does _not_ prevent you from using any other functions, If you're trying to
    /// receive while disabled, you will be waiting a long time.
    pub fn enable(&mut self, en: bool) {
        self.ir_tx.enable(en);
    }

    /// Non-blocking read from the recv buffer. Will fill `buf` with as
    /// many bytes as are available up to the size of `buf`, but will
    /// not wait for new bytes to come in.
    pub fn read_nb(&mut self, buf: &mut [u8]) -> Result<usize, uart::Error> {
        self.ir_rx.read_nb(buf)
    }

    /// Fill a buffer with bytes read from the transceiver.
    pub async fn read_all(&mut self, buf: &mut [u8]) -> Result<(), uart::Error> {
        self.ir_rx.read_all(buf).await
    }

    /// Send a sequence of bytes with collision detection. Bytes are
    /// sent in 16 byte chunks and if a data error is detected, this
    /// aborts early and returns `false`. Returns `true` if all bytes
    /// were sent.
    pub async fn send(&mut self, buf: &[u8]) -> Result<bool, uart::Error> {
        self.ir_tx.send(buf).await
    }

    pub fn split(self) -> (IrdaTransmitter<'a>, IrdaReceiver<'a>) {
        (self.ir_tx, self.ir_rx)
    }
}

pub struct IrdaTransmitter<'d> {
    peripheral: Arc<CriticalSectionMutex<PeripheralRef<'static, AnyUart>>>,
    uart_tx: UartTx<'d, Async>,
    en_driver: Output<'d>,
}

impl IrdaTransmitter<'_> {
    /// Switch the transceiver's enable pin.
    ///
    /// When not enabled, the transceiver is turned off and cannot transmit or receive. This
    /// state does _not_ prevent you from using any other functions, If you're trying to
    /// receive while disabled, you will be waiting a long time.
    pub fn enable(&mut self, en: bool) {
        self.en_driver.set_level((!en).into());
    }

    /// Send a sequence of bytes. Bytes are
    /// sent in 16 byte chunks and if a data error is detected, this
    /// aborts early and returns `false`. Returns `true` if all bytes
    /// were sent.
    pub async fn send(&mut self, buf: &[u8]) -> Result<bool, uart::Error> {
        self.peripheral.lock(|p| p.set_tx_en(true));
        self.uart_tx.write_async(buf).await?;
        // We need to wait for everything to be sent here.
        // `write_async()` only waits when the TX FIFO is full.
        // Otherwise TX_EN gets turned off below before the message gets
        // fully sent.
        self.uart_tx.flush_async().await?;
        self.peripheral.lock(|p| p.set_tx_en(false));
        Ok(true)
    }
}

pub struct IrdaReceiver<'d> {
    peripheral: Arc<CriticalSectionMutex<PeripheralRef<'static, AnyUart>>>,
    uart_rx: UartRx<'d, Async>,
}

impl IrdaReceiver<'_> {
    /// How many bytes are in the recv buffer?
    pub fn rx_fifo_count(&self) -> usize {
        self.peripheral.lock(|p| {
            p.info()
                .register_block()
                .status()
                .read()
                .rxfifo_cnt()
                .bits()
                .into()
        })
    }

    // TODO(chip): Remove this when https://github.com/esp-rs/esp-hal/pull/3190 gets released
    fn check_fifo(&self, e: &uart::Error) {
        if matches!(e, uart::Error::FifoOverflowed) {
            self.peripheral.lock(|p| p.rxfifo_reset());
        }
    }

    /// Non-blocking read from the recv buffer. Will fill `buf` with as
    /// many bytes as are available up to the size of `buf`, but will
    /// not wait for new bytes to come in.
    pub fn read_nb(&mut self, buf: &mut [u8]) -> Result<usize, uart::Error> {
        let count = usize::min(self.rx_fifo_count(), buf.len());
        self.uart_rx
            .read_bytes(&mut buf[..count])
            .inspect_err(|e| self.check_fifo(e))?;
        Ok(count)
    }

    /// Read from the recv buffer. Will fill `buf` with as
    /// many bytes as are available up to the size of `buf`, but will
    /// not wait for new bytes to come in.
    pub async fn read(&mut self, buf: &mut [u8]) -> Result<usize, uart::Error> {
        let count = self
            .uart_rx
            .read_async(buf)
            .await
            .inspect_err(|e| self.check_fifo(e))?;
        Ok(count)
    }

    /// Fill a buffer with bytes read from the transceiver.
    pub async fn read_all(&mut self, buf: &mut [u8]) -> Result<(), uart::Error> {
        let mut c = 0;
        while c < buf.len() {
            c += self
                .uart_rx
                .read_async(&mut buf[c..])
                .await
                .inspect_err(|e| self.check_fifo(e))?;
        }
        Ok(())
    }
}
