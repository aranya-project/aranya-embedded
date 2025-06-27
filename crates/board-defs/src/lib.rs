#![no_std]

mod boards;

use esp_hal::gpio::GpioPin;

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
    pub button: GpioPin<B>,
    pub accessory_power: Option<GpioPin<AP>>,
    pub neopixel: NeoPixelPinDef<ND, NP>,
    pub i2c: I2CPinDef<SCL, SDA>,
    pub sd: Option<SdPinDef<SCK, SDO, SDI, CS, CD>>,
    pub ir: Option<IrPinDef<IRTX, IRRX, IREN>>,
}
