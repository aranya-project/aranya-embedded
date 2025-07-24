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
mod watchdog;

use aranya_runtime::vm_action;
use embassy_executor::Spawner;
#[cfg(feature = "net-esp-now")]
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
use embassy_time::{Duration, TimeoutError, Timer};
use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    gpio::{AnyPin, GpioPin, Input, Level, Output, Pull},
    interrupt::{software::SoftwareInterruptControl, Priority},
    peripherals::{TIMG0, TIMG1},
    timer::timg::TimerGroup,
};
use esp_hal_embassy::{main, InterruptExecutor};
#[cfg(feature = "net-irda")]
use esp_irda_transceiver::IrdaTransceiver;
use esp_rmt_neopixel::Neopixel;
use esp_storage::FlashStorage;
#[cfg(feature = "net-esp-now")]
use esp_wifi::{
    esp_now::{EspNowManager, EspNowReceiver, EspNowSender, PeerInfo, BROADCAST_ADDRESS},
    init, EspWifiController,
};
use log::info;
use parameter_store::{EmbeddedStorageIO, ParameterStore, ParameterStoreError, Parameters, RgbU8};
use static_cell::StaticCell;

use crate::{
    aranya::daemon::{Daemon, Imp},
    hardware::neopixel::{NeopixelSink, NEOPIXEL_SIGNAL},
    net::NetworkEngine,
    watchdog::Watchdog,
};

const MAX_NETWORK_ENGINES: usize = 2;

//static NET_STACK: StaticCell<Stack<8192>> = StaticCell::new();

#[main]
async fn main(spawner: Spawner) {
    // Initialize peripherals
    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));
    let board_def = board_defs::board_def!(peripherals);

    // Initialize embassy timer groups
    let timer_g0 = TimerGroup::new(peripherals.TIMG0);
    let timer_g1 = TimerGroup::new(peripherals.TIMG1);
    esp_hal_embassy::init(timer_g1.timer0);

    // Initialize heaps
    esp_alloc::heap_allocator!(96 * 1024);
    esp_alloc::psram_allocator!(peripherals.PSRAM, esp_hal::psram);

    esp_println::logger::init_logger_from_env();
    info!("Embassy initialized!");

    let (wdt0, wdt1) = watchdog::watchdog_init(timer_g0.wdt, timer_g1.wdt);
    info!("Watchdog initialized");

    //tracing::subscriber::set_global_default(util::SimpleSubscriber::new()).expect("log subscriber");

    let mut acc_power = board_def
        .accessory_power
        .map(|pin| Output::new(pin, Level::Low));

    let neopixel = Neopixel::new(
        peripherals.RMT,
        board_def.neopixel.data,
        board_def.neopixel.power,
        board_def.neopixel.power_inverted,
    )
    .expect("could not initialize neopixel");

    let io = EmbeddedStorageIO::new(
        FlashStorage::new(),
        0x9000, /* TODO(chip): get this from the partition table */
    );

    let mut parameters = ParameterStore::new(io);
    let parameter_values = match parameters.fetch() {
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
    log::info!("p: {parameter_values:?}");

    // Auto-erase the storage when the parameters' graph ID is none
    #[cfg(feature = "storage-internal")]
    if parameter_values.graph_id.is_none() {
        storage::internal::nuke().expect("could not nuke!?");
    }

    #[cfg(feature = "storage-internal")]
    let storage_provider = storage::internal::init().expect("couldn't get storage");

    #[cfg(feature = "storage-sd")]
    let storage_provider = if let Some(sd) = board_def.sd {
        if let Some(acc_power) = &mut acc_power {
            acc_power.set_high();
        }
        storage::sd::init(
            peripherals.SPI2,
            sd.sck,
            sd.mosi,
            sd.miso,
            sd.cs,
            timer_g0.timer0,
        )
        .await
        .expect("couldn't get storage")
    } else {
        panic!("`storage-sd` configured but no SD peripheral defined on board");
    };

    let mut daemon = Daemon::init(storage_provider)
        .await
        .expect("could not create daemon");

    let graph_id = match parameter_values.graph_id {
        None => {
            let graph_id = daemon.create_team().await.expect("could not create team");
            parameters
                .update(|p| p.graph_id = Some(graph_id.into()))
                .expect("could not store parameters");
            log::info!("Created graph - {graph_id}");
            graph_id
        }
        Some(a) => a.into(),
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

    #[cfg(feature = "net-esp-now")]
    {
        let rng = esp_hal::rng::Rng::new(peripherals.RNG);
        let init = &*mk_static!(
            EspWifiController<'static>,
            init(timer_g0.timer0, rng, peripherals.RADIO_CLK).unwrap()
        );

        let wifi = peripherals.WIFI;
        let esp_now = esp_wifi::esp_now::EspNow::new(&init, wifi).unwrap();
        log::info!("esp-now version {}", esp_now.version().unwrap());

        let (manager, sender, receiver) = esp_now.split();
        let manager: &'static mut EspNowManager<'static> =
            mk_static!(EspNowManager<'static>, manager);
        let receiver = Mutex::<CriticalSectionRawMutex, _>::new(receiver);

        let sender = Mutex::<CriticalSectionRawMutex, _>::new(sender);

        let engine = net::espnow::start(sender, receiver, parameter_values.address).await;

        spawner.must_spawn(aranya::syncer::sync_esp_now(
            daemon.get_imp(graph_id, NeopixelSink::new()),
            engine.interface(),
            parameter_values.peers.clone(),
        ));

        if network_engines.push(engine).is_err() {
            log::info!("could not start ESP Now network engine");
        }
    }

    #[cfg(feature = "net-irda")]
    if let Some(ir) = board_def.ir {
        if let Some(acc_power) = &mut acc_power {
            acc_power.set_high();
        }
        let irts = IrdaTransceiver::new(peripherals.UART1, ir.tx, ir.rx, ir.en);
        let engine = net::irda::start(irts, parameter_values.address).await;
        spawner.must_spawn(aranya::syncer::sync_irda(
            daemon.get_imp(graph_id, NeopixelSink::new()),
            engine.interface(),
            parameter_values.peers.clone(),
        ));
        if network_engines.push(engine).is_err() {
            log::info!("could not start IR network engine");
        }
    }

    /* TODO(chip): re-enable this when esp-storage works multi-core
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
    */

    // Spawn a high priority InterruptExecutor to run the network engines
    {
        let sw_ints = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
        static EXECUTOR: StaticCell<InterruptExecutor<2>> = StaticCell::new();
        let executor = InterruptExecutor::new(sw_ints.software_interrupt2);
        let executor = EXECUTOR.init(executor);
        let spawner = executor.start(Priority::Priority3);
        spawner.must_spawn(net_task(network_engines, wdt1));
    }

    spawner.must_spawn(button_task(
        board_def.button,
        daemon.get_imp(graph_id, NeopixelSink::new()),
        parameter_values.color,
        parameters,
    ));

    spawner.must_spawn(led_task(neopixel, parameter_values.color));

    spawner.must_spawn(heap_report());
    spawner.must_spawn(watchdog::idle_task0(wdt0));
}

#[embassy_executor::task]
async fn net_task(
    network_engines: heapless::Vec<&'static dyn NetworkEngine, MAX_NETWORK_ENGINES>,
    wdt: &'static Watchdog<TIMG1>,
) {
    log::info!("net task started");

    let spawner = Spawner::for_current_executor().await;
    for e in network_engines {
        e.run(spawner).expect("could not start engine {e}");
    }
    spawner.must_spawn(watchdog::idle_task1(wdt));
}

#[embassy_executor::task]
async fn button_task(
    pin: AnyPin,
    imp: Imp<NeopixelSink>,
    color: RgbU8,
    mut parameters: ParameterStore<Parameters, EmbeddedStorageIO<FlashStorage>>,
) {
    let mut driver = Input::new(pin, Pull::Up);
    loop {
        driver.wait_for_falling_edge().await;
        match embassy_time::with_timeout(Duration::from_secs(5), driver.wait_for_high()).await {
            Ok(_) => {
                log::info!("led pressed");
                match imp
                    .call_action(vm_action!(set_led(
                        color.red as i64,
                        color.green as i64,
                        color.blue as i64
                    )))
                    .await
                {
                    Ok(_) => (),
                    Err(e) => log::error!("could not set LED: {e}"),
                };
            }
            Err(TimeoutError) => {
                // Button has been held for five seconds; DESTROY THE WORLD
                parameters.update(|p| p.graph_id = None).ok();
                #[cfg(feature = "storage-internal")]
                storage::internal::nuke().expect("could not nuke!?");
                log::info!("Storage nuked. Release button to reset.");
                log::info!(""); // Sometimes espflash doesn't flush the last line so the message isn't visible.

                // wait for the button to go high so we don't accidentally wind up in the
                // firmware loader. But do it in a busy loop so we don't yield to any other
                // async tasks.
                while driver.is_low() {}
                esp_hal::reset::software_reset();
            }
        }
    }
}

#[embassy_executor::task]
async fn led_task(mut neopixel: Neopixel<'static>, initial_color: parameter_store::RgbU8) {
    let mut intensity = 1.0;
    // gross - TODO(chip): find some cleaner neutral format between parameter store and neopixel.
    let mut color = <(u8, u8, u8) as From<parameter_store::RgbU8>>::from(initial_color).into();
    neopixel.set_power(true);
    loop {
        match embassy_time::with_timeout(Duration::from_millis(100), NEOPIXEL_SIGNAL.wait()).await {
            Ok(c) => {
                color = c;
                intensity = 1.0;
            }
            Err(TimeoutError) => intensity -= (intensity - 0.3) / 5.0,
        };
        let effective_color = color * intensity;
        log::trace!("effective color: {effective_color:?} (intensity {intensity:04})");
        neopixel
            .set_color(
                effective_color.red,
                effective_color.green,
                effective_color.blue,
            )
            .inspect_err(|e| log::error!("neopixel: {e}"))
            .ok();
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
