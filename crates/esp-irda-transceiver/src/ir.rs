use esp32s3::uart0::RegisterBlock;
use esp_hal::{
    peripheral::PeripheralRef,
    uart::{AnyUart, Instance},
};

pub trait IrUart {
    fn set_irda_mode(&self, en: bool);
    fn set_tx_en(&self, tx: bool);
    fn set_irda_duplex(&self, duplex: bool);
    fn rxfifo_reset(&self);
}

impl<'a> IrUart for PeripheralRef<'a, AnyUart> {
    /// Enable or disable the IrDA mode (`UART_IRDA_EN` in `UART_CONF0_REG`)
    fn set_irda_mode(&self, en: bool) {
        self.info()
            .register_block()
            .conf0()
            .modify(|_, w| w.irda_en().bit(en));
        sync_regs(self.info().register_block());
    }

    /// Enable or disable the IrDA transmit mode (`UART_IRDA_TX_EN` in `UART_CONF0_REG`)
    fn set_tx_en(&self, tx: bool) {
        self.info()
            .register_block()
            .conf0()
            .modify(|_, w| w.irda_tx_en().bit(tx));
        sync_regs(self.info().register_block());
    }

    /// Enable or disable IrDA duplex mode (`UART_IRDA_DPLX` in `UART_CONF0_REG`)
    fn set_irda_duplex(&self, duplex: bool) {
        self.info()
            .register_block()
            .conf0()
            .modify(|_, w| w.irda_dplx().bit(duplex));
        sync_regs(self.info().register_block());
    }

    // TODO(chip): remove this when https://github.com/esp-rs/esp-hal/pull/3190 gets released
    fn rxfifo_reset(&self) {
        fn rxfifo_rst(reg_block: &RegisterBlock, enable: bool) {
            reg_block.conf0().modify(|_, w| w.rxfifo_rst().bit(enable));
            sync_regs(reg_block);
        }

        rxfifo_rst(self.info().register_block(), true);
        rxfifo_rst(self.info().register_block(), false);
    }
}

#[inline(always)]
fn sync_regs(register_block: &RegisterBlock) {
    register_block.id().modify(|_, w| w.reg_update().set_bit());

    while register_block.id().read().reg_update().bit_is_set() {
        // wait
    }
}
