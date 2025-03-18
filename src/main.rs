#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]

extern crate alloc;

pub mod aranya;
mod built;
mod hardware;
mod net;
mod storage;
mod util;

use aranya::daemon::Daemon;
use embassy_executor::Spawner;
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::timer::timg::TimerGroup;
use esp_hal_embassy::main;
use log::info;

#[main]
async fn main(spawner: Spawner) {
    // Initialize peripherals
    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));

    // Initialize embassy timer groups
    let timer_g0 = TimerGroup::new(peripherals.TIMG0);
    let timer_g1 = TimerGroup::new(peripherals.TIMG1);
    esp_hal_embassy::init(timer_g1.timer0);

    // Initialize heaps
    esp_alloc::heap_allocator!(64 * 1024);
    esp_alloc::psram_allocator!(peripherals.PSRAM, esp_hal::psram);

    esp_println::logger::init_logger_from_env();
    info!("Embassy initialized!");

    let rng = esp_hal::rng::Rng::new(peripherals.RNG);

    #[cfg(feature = "storage-internal")]
    let storage_provider = storage::internal::init().expect("couldn't get storage");

    #[cfg(feature = "storage-sd")]
    let storage_provider = storage::sd::init(
        peripherals.SPI2,
        peripherals.GPIO36,
        peripherals.GPIO35,
        peripherals.GPIO37,
        peripherals.GPIO11,
        timer_g0.timer0,
    )
    .await
    .expect("couldn't get storage");

    let mut daemon = Daemon::init(storage_provider)
        .await
        .expect("could not create daemon");
    let graph_id = daemon.create_team().await.expect("could not create team");
    log::info!("Created graph - {graph_id}");

    #[cfg(feature = "net-wifi")]
    {
        let network = net::wifi::start(
            peripherals.WIFI,
            peripherals.RADIO_CLK,
            timer_g1.timer1,
            rng,
            spawner,
        )
        .await;
        spawner
            .spawn(aranya::syncer::sync_wifi(daemon.get_client(), network))
            .expect("could not spawn WiFi syncer task");
    }

    #[cfg(feature = "net-irda")]
    {
        let network = net::irda::start().await;
        spawner
            .spawn(aranya::syncer::sync_irda(daemon.get_client(), network))
            .expect("could not spawn IrDA syncer task");
    }

    spawner.spawn(heap_report()).ok();
}

#[embassy_executor::task]
async fn heap_report() {
    loop {
        let stats = esp_alloc::HEAP.stats();
        log::info!("{}", stats);
        embassy_time::Timer::after_secs(10).await;
    }
}
