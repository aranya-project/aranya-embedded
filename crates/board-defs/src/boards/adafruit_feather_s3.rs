#![cfg(feature = "adafruit-feather-s3")]

#[macro_export]
macro_rules! board_def {
    ($peripherals:ident) => {{
        use $crate::esp_hal::gpio::Pin;
        $crate::BoardDef {
            button: $peripherals.GPIO0.degrade(),
            accessory_power: Some($peripherals.GPIO7.degrade()),
            neopixel: $crate::NeoPixelPinDef {
                data: $peripherals.GPIO33.degrade(),
                power: $peripherals.GPIO21.degrade(),
                power_inverted: false,
            },
            i2c: $crate::I2CPinDef {
                scl: $peripherals.GPIO4.degrade(),
                sda: $peripherals.GPIO3.degrade(),
            },
            sd: Some($crate::SdPinDef {
                sck: $peripherals.GPIO36.degrade(),
                mosi: $peripherals.GPIO35.degrade(),
                miso: $peripherals.GPIO37.degrade(),
                cs: $peripherals.GPIO11.degrade(),
                cd: $peripherals.GPIO13.degrade(),
            }),
            ir: Some($crate::IrPinDef {
                tx: $peripherals.GPIO39.degrade(),
                rx: $peripherals.GPIO38.degrade(),
                en: $peripherals.GPIO8.degrade(),
            }),
            indicators: $crate::IndicatorsPinDef {
                tx_led: None,
                rx_led: None,
            }
        }
    }};
}
