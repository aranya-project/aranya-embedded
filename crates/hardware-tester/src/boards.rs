use esp_hal::gpio::GpioPin;
use esp_hal::peripherals::Peripherals;

/// The peripherals used by this application
pub struct AppPeripherals {
    pub timg0: esp_hal::peripherals::TIMG0,
    pub timg1: esp_hal::peripherals::TIMG1,
    pub psram: Option<esp_hal::peripherals::PSRAM>,
    pub rmt: esp_hal::peripherals::RMT,
    pub i2c0: esp_hal::peripherals::I2C0,
    pub spi2: esp_hal::peripherals::SPI2,
    pub uart1: esp_hal::peripherals::UART1,
}

pub struct NeoPixelPinDef<const ND: u8, const NP: u8> {
    pub data: GpioPin<ND>,
    pub power: GpioPin<NP>,
    pub power_inverted: bool,
}

pub struct I2CPinDef<const SCL: u8, const SDA: u8> {
    pub scl: GpioPin<SCL>,
    pub sda: GpioPin<SDA>,
}

pub struct SdPinDef<const SCK: u8, const MOSI: u8, const MISO: u8, const CS: u8, const CD: u8> {
    pub sck: GpioPin<SCK>,
    pub mosi: GpioPin<MOSI>,
    pub miso: GpioPin<MISO>,
    pub cs: GpioPin<CS>,
    pub cd: GpioPin<CD>,
}

pub struct IrPinDef<const IRTX: u8, const IRRX: u8, const IREN: u8> {
    pub tx: GpioPin<IRTX>,
    pub rx: GpioPin<IRRX>,
    pub en: GpioPin<IREN>,
}

pub struct BoardDef<
    const B: u8,
    const AP: u8,
    const ND: u8,
    const NP: u8,
    const SCL: u8,
    const SDA: u8,
    const SCK: u8,
    const SDO: u8,
    const SDI: u8,
    const CS: u8,
    const CD: u8,
    const IRTX: u8,
    const IRRX: u8,
    const IREN: u8,
> {
    pub peripherals: AppPeripherals,
    pub button: GpioPin<B>,
    pub accessory_power: Option<GpioPin<AP>>,
    pub neopixel: NeoPixelPinDef<ND, NP>,
    pub i2c: I2CPinDef<SCL, SDA>,
    pub sd: Option<SdPinDef<SCK, SDO, SDI, CS, CD>>,
    pub ir: Option<IrPinDef<IRTX, IRRX, IREN>>,
}

#[cfg(feature = "feather-s3")]
pub fn board_def(
    peripherals: Peripherals,
) -> BoardDef<0, 7, 33, 21, 4, 3, 36, 35, 37, 11, 13, 39, 38, 8> {
    BoardDef {
        peripherals: AppPeripherals {
            timg0: peripherals.TIMG0,
            timg1: peripherals.TIMG1,
            psram: Some(peripherals.PSRAM),
            rmt: peripherals.RMT,
            i2c0: peripherals.I2C0,
            spi2: peripherals.SPI2,
            uart1: peripherals.UART1,
        },
        button: peripherals.GPIO0,
        accessory_power: Some(peripherals.GPIO7),
        neopixel: NeoPixelPinDef {
            data: peripherals.GPIO33,
            power: peripherals.GPIO21,
            power_inverted: false,
        },
        i2c: I2CPinDef {
            scl: peripherals.GPIO4,
            sda: peripherals.GPIO3,
        },
        sd: Some(SdPinDef {
            sck: peripherals.GPIO36,
            mosi: peripherals.GPIO35,
            miso: peripherals.GPIO37,
            cs: peripherals.GPIO11,
            cd: peripherals.GPIO13,
        }),
        ir: Some(IrPinDef {
            tx: peripherals.GPIO39,
            rx: peripherals.GPIO38,
            en: peripherals.GPIO8,
        }),
    }
}

#[cfg(feature = "spideroak-demo-v2")]
pub fn board_def(
    peripherals: Peripherals,
) -> BoardDef<0, 47, 41, 40, 4, 5, 36, 35, 37, 38, 48, 13, 14, 21> {
    BoardDef {
        peripherals: AppPeripherals {
            timg0: peripherals.TIMG0,
            timg1: peripherals.TIMG1,
            psram: Some(peripherals.PSRAM),
            rmt: peripherals.RMT,
            i2c0: peripherals.I2C0,
            spi2: peripherals.SPI2,
            uart1: peripherals.UART1,
        },
        button: peripherals.GPIO0,
        accessory_power: Some(peripherals.GPIO47),
        neopixel: NeoPixelPinDef {
            data: peripherals.GPIO41,
            power: peripherals.GPIO40,
            power_inverted: true,
        },
        i2c: I2CPinDef {
            scl: peripherals.GPIO4,
            sda: peripherals.GPIO5,
        },
        sd: Some(SdPinDef {
            sck: peripherals.GPIO36,
            mosi: peripherals.GPIO35,
            miso: peripherals.GPIO37,
            cs: peripherals.GPIO38,
            cd: peripherals.GPIO48,
        }),
        ir: Some(IrPinDef {
            tx: peripherals.GPIO13,
            rx: peripherals.GPIO14,
            en: peripherals.GPIO21,
        }),
    }
}
