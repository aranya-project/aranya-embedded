#![cfg(feature = "spideroak-demo-v2")]

#[macro_export]
macro_rules! board_def {
    ($peripherals:ident) => {
        ::board_defs::BoardDef {
            button: $peripherals.GPIO0,
            accessory_power: Some($peripherals.GPIO47),
            neopixel: ::board_defs::NeoPixelPinDef {
                data: $peripherals.GPIO41,
                power: $peripherals.GPIO40,
                power_inverted: true,
            },
            i2c: ::board_defs::I2CPinDef {
                scl: $peripherals.GPIO4,
                sda: $peripherals.GPIO5,
            },
            sd: Some(::board_defs::SdPinDef {
                sck: $peripherals.GPIO36,
                mosi: $peripherals.GPIO35,
                miso: $peripherals.GPIO37,
                cs: $peripherals.GPIO38,
                cd: $peripherals.GPIO48,
            }),
            ir: Some(::board_defs::IrPinDef {
                tx: $peripherals.GPIO13,
                rx: $peripherals.GPIO14,
                en: $peripherals.GPIO21,
            }),
        }
    };
}
