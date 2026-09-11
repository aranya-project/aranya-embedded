use esp_hal::rng::Rng;
use getrandom::Error;

#[unsafe(no_mangle)]
unsafe extern "Rust" fn __getrandom_v03_custom(dest: *mut u8, len: usize) -> Result<(), Error> {
    let buf = unsafe {
        core::ptr::write_bytes(dest, 0, len);
        core::slice::from_raw_parts_mut(dest, len)
    };

    esp32_getrandom(buf)
}

// We need to provide a custom get random implementation for ESP32 due to Crypto's dependency getrandom not providing one for ESP32 (though it provides Rng providers for most OSes "https://docs.rs/getrandom/0.2.15/getrandom/")
// If we don't provide an alternative Rng generator we get compiler error "target is not supported, for more information see: https://docs.rs/getrandom/#unsupported-targets"
fn esp32_getrandom(buf: &mut [u8]) -> Result<(), Error> {
    // Initialize the ESP32 RNG
    // Stealing here isn't the best idea as if we try to get and use the RNG peripheral elsewhere it can cause unexpected crashes but it is the easiest
    let peripherals = unsafe { esp_hal::peripherals::RNG::steal() };
    let mut rng = Rng::new(peripherals);

    // Fill the buffer with random bytes
    for chunk in buf.chunks_mut(4) {
        let random_word = rng.random();
        let bytes = random_word.to_le_bytes();
        chunk.copy_from_slice(&bytes[..chunk.len()]);
    }

    Ok(())
}
