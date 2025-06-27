#![cfg(feature = "adafruit-feather-s3")]

#[macro_export]
macro_rules! board_def {
    ($peripherals:ident) => {
        ::board_defs::BoardDef {
            button: $peripherals.GPIO0,
            accessory_power: Some($peripherals.GPIO7),
            neopixel: ::board_defs::NeoPixelPinDef {
                data: $peripherals.GPIO33,
                power: $peripherals.GPIO21,
                power_inverted: false,
            },
            i2c: ::board_defs::I2CPinDef {
                scl: $peripherals.GPIO4,
                sda: $peripherals.GPIO3,
            },
            sd: Some(::board_defs::SdPinDef {
                sck: $peripherals.GPIO36,
                mosi: $peripherals.GPIO35,
                miso: $peripherals.GPIO37,
                cs: $peripherals.GPIO11,
                cd: $peripherals.GPIO13,
            }),
            ir: Some(::board_defs::IrPinDef {
                tx: $peripherals.GPIO39,
                rx: $peripherals.GPIO38,
                en: $peripherals.GPIO8,
            }),
        }
    };
}
