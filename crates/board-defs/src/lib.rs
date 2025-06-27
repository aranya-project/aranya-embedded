#![no_std]

mod boards;

pub use esp_hal;
use esp_hal::gpio::AnyPin;

pub struct NeoPixelPinDef {
    pub data: AnyPin,
    pub power: AnyPin,
    pub power_inverted: bool,
}

pub struct I2CPinDef {
    pub scl: AnyPin,
    pub sda: AnyPin,
}

pub struct SdPinDef {
    pub sck: AnyPin,
    pub mosi: AnyPin,
    pub miso: AnyPin,
    pub cs: AnyPin,
    pub cd: AnyPin,
}

pub struct IrPinDef {
    pub tx: AnyPin,
    pub rx: AnyPin,
    pub en: AnyPin,
}

pub struct BoardDef {
    pub button: AnyPin,
    pub accessory_power: Option<AnyPin>,
    pub neopixel: NeoPixelPinDef,
    pub i2c: I2CPinDef,
    pub sd: Option<SdPinDef>,
    pub ir: Option<IrPinDef>,
}
