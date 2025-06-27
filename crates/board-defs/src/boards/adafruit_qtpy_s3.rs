#![cfg(feature = "adafruit-qtpy-s3")]

#[macro_export]
macro_rules! board_def {
    ($peripherals:ident) => {
        ::board_defs::BoardDef {
            button: $peripherals.GPIO0,
            accessory_power: None,
            neopixel: ::board_defs::NeoPixelPinDef {
                data: $peripherals.GPIO39,
                power: $peripherals.GPIO38,
                power_inverted: false,
            },
            i2c: ::board_defs::I2CPinDef {
                scl: $peripherals.GPIO40,
                sda: $peripherals.GPIO41,
            },
            sd: None,
            ir: None,
        }
    };
}
