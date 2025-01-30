use esp_hal::rng::Rng;
use getrandom::register_custom_getrandom;
use getrandom::Error;

// TODO: https://github.com/rust-random/getrandom/issues/397 use aranya-core's CSPRNG?

// We need to provide a custom get random implementation for ESP32 due to Crypto's dependency getrandom not providing one for ESP32 (though it provides Rng providers for most OSes "https://docs.rs/getrandom/0.2.15/getrandom/")
// If we don't provide an alternative Rng generator we get compiler error "target is not supported, for more information see: https://docs.rs/getrandom/#unsupported-targets"
pub fn esp32_getrandom(buf: &mut [u8]) -> Result<(), Error> {
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

register_custom_getrandom!(esp32_getrandom);
