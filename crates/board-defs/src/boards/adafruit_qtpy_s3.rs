#![cfg(feature = "adafruit-qtpy-s3")]

#[macro_export]
macro_rules! board_def {
    ($peripherals:ident) => {{
        use $crate::esp_hal::gpio::Pin;
        $crate::BoardDef {
            button: $peripherals.GPIO0.degrade(),
            accessory_power: None,
            neopixel: $crate::NeoPixelPinDef {
                data: $peripherals.GPIO39.degrade(),
                power: $peripherals.GPIO38.degrade(),
                power_inverted: false,
            },
            i2c: $crate::I2CPinDef {
                scl: $peripherals.GPIO40.degrade(),
                sda: $peripherals.GPIO41.degrade(),
            },
            sd: None,
            ir: None,
            indicators: $crate::IndicatorsPinDef {
                tx_led: None,
                rx_led: None,
            }
        }
    }};
}
