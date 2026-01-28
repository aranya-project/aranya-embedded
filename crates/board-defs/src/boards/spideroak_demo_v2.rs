#![cfg(feature = "spideroak-demo-v2")]

#[macro_export]
macro_rules! board_def {
    ($peripherals:ident) => {{
        use $crate::esp_hal::gpio::Pin;
        $crate::BoardDef {
            button: $peripherals.GPIO0.degrade(),
            accessory_power: Some($peripherals.GPIO47.degrade()),
            neopixel: $crate::NeoPixelPinDef {
                data: $peripherals.GPIO41.degrade(),
                power: $peripherals.GPIO40.degrade(),
                power_inverted: true,
            },
            i2c: $crate::I2CPinDef {
                scl: $peripherals.GPIO4.degrade(),
                sda: $peripherals.GPIO5.degrade(),
            },
            sd: Some($crate::SdPinDef {
                sck: $peripherals.GPIO36.degrade(),
                mosi: $peripherals.GPIO35.degrade(),
                miso: $peripherals.GPIO37.degrade(),
                cs: $peripherals.GPIO38.degrade(),
                cd: $peripherals.GPIO48.degrade(),
            }),
            ir: Some($crate::IrPinDef {
                tx: $peripherals.GPIO13.degrade(),
                rx: $peripherals.GPIO14.degrade(),
                en: $peripherals.GPIO21.degrade(),
            }),
            indicators: $crate::IndicatorsPinDef {
                tx_led: Some($peripherals.GPIO10.degrade()),
                rx_led: Some($peripherals.GPIO11.degrade()),
            }
        }
    }};
}
