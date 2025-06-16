pub mod esp32_rng;
pub mod esp32_time;
pub mod neopixel;

#[no_mangle]
pub fn custom_halt() -> ! {
    esp_hal::reset::software_reset();
    unreachable!()
}