#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]
#![feature(core_io_borrowed_buf)]
#![feature(new_zeroed_alloc)]

extern crate alloc;

pub mod aranya;
mod built;
mod hardware;
mod net;
mod storage;
mod util;

use aranya::daemon::{Daemon, Imp};
use aranya_runtime::vm_action;
use embassy_executor::Spawner;
use embassy_time::Timer;
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::cpu_control::{CpuControl, Stack};
use esp_hal::gpio::{GpioPin, Input, Pull};
use esp_hal::timer::timg::TimerGroup;
use esp_hal_embassy::{main, Executor};
use esp_irda_transceiver::IrdaTransceiver;
use esp_storage::FlashStorage;
use log::info;
use net::NetworkEngine;
use parameter_store::{EmbeddedStorageIO, ParameterStore, ParameterStoreError, Parameters};
use static_cell::StaticCell;

const MAX_NETWORK_ENGINES: usize = 2;

static NET_STACK: StaticCell<Stack<8192>> = StaticCell::new();

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

    tracing::subscriber::set_global_default(util::SimpleSubscriber::new()).expect("log subscriber");

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

    #[cfg(feature = "storage-internal")]
    let io = EmbeddedStorageIO::new(
        FlashStorage::new(),
        0x9000, /* TODO(chip): get this from the partition table */
    );

    let mut parameters = ParameterStore::new(io);
    let p = match parameters.fetch() {
        Ok(p) => p,
        Err(e) => match e {
            ParameterStoreError::Corrupt => {
                log::info!("Parameters corrupt; writing defaults");
                parameters
                    .store(&Parameters::default())
                    .expect("could not store parameters")
            }
            e => panic!("{e}"),
        },
    };
    log::info!("p: {p:?}");

    let mut daemon = Daemon::init(storage_provider)
        .await
        .expect("could not create daemon");

    let graph_id = match p.graph_id {
        None => {
            let graph_id = daemon.create_team().await.expect("could not create team");
            parameters
                .update(|p| p.graph_id = Some(graph_id))
                .expect("could not store parameters");
            log::info!("Created graph - {graph_id}");
            graph_id
        }
        Some(a) => a,
    };

    let mut network_engines: heapless::Vec<&'static dyn NetworkEngine, MAX_NETWORK_ENGINES> =
        heapless::Vec::new();

    #[cfg(feature = "net-wifi")]
    {
        let rng = esp_hal::rng::Rng::new(peripherals.RNG);
        let engine = net::wifi::start(
            peripherals.WIFI,
            peripherals.RADIO_CLK,
            timer_g1.timer1,
            rng,
            spawner,
        )
        .await;
        spawner
            .spawn(aranya::syncer::sync_wifi(
                daemon.get_client(),
                engine.interface(),
            ))
            .expect("could not spawn WiFi syncer task");
        network_engines.push(engine);
    }

    #[cfg(feature = "net-irda")]
    {
        let irts = IrdaTransceiver::new(
            peripherals.UART1,
            peripherals.GPIO39,
            peripherals.GPIO38,
            peripherals.GPIO8,
        );
        let engine = net::irda::start(irts, p.address).await;
        spawner
            .spawn(aranya::syncer::sync_irda(
                daemon.get_imp(graph_id),
                engine.interface(),
                p.peers.clone(),
            ))
            .expect("could not spawn IrDA syncer task");
        if network_engines.push(engine).is_err() {
            log::info!("could not start IR network engine");
        }
    }

    // Spawn a task on the second CPU to run the network engines
    let mut cpu_control = CpuControl::new(peripherals.CPU_CTRL);
    let stack = NET_STACK.init(Stack::new());
    let app_core_guard = cpu_control
        .start_app_core(stack, move || {
            static EXECUTOR: StaticCell<Executor> = StaticCell::new();
            let executor = EXECUTOR.init(Executor::new());
            executor.run(|spawner| {
                spawner
                    .spawn(net_task(spawner, network_engines))
                    .expect("could not spawn net task");
            })
        })
        .expect("could not start on second core");
    // Don't drop the guard so we don't stop the second core
    core::mem::forget(app_core_guard);

    spawner
        .spawn(button_task(peripherals.GPIO0, daemon.get_imp(graph_id)))
        .ok();

    spawner.spawn(heap_report()).ok();
}

#[embassy_executor::task]
async fn net_task(
    spawner: Spawner,
    network_engines: heapless::Vec<&'static dyn NetworkEngine, MAX_NETWORK_ENGINES>,
) {
    log::info!("net task started");
    for e in network_engines {
        e.run(spawner).expect("could not start engine {e}");
    }
}

#[embassy_executor::task]
async fn button_task(pin: GpioPin<0>, imp: Imp) {
    let mut driver = Input::new(pin, Pull::Up);
    loop {
        driver.wait_for_falling_edge().await;
        match imp.call_action(vm_action!(set_led(255, 255, 0))).await {
            Ok(_) => (),
            Err(e) => log::error!("could not set LED: {e}"),
        };
    }
}

#[embassy_executor::task]
async fn heap_report() {
    loop {
        let stats = esp_alloc::HEAP.stats();
        log::info!("{}", stats);
        Timer::after_secs(10).await;
    }
}
