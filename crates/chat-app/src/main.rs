#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]
#![feature(core_io_borrowed_buf)]
#![feature(new_zeroed_alloc)]

extern crate alloc;

mod application;
pub mod aranya;
mod built;
mod hardware;
mod net;
mod storage;
mod util;
mod watchdog;

use aranya::daemon::Daemon;
use aranya_crypto::{id::IdExt, DeviceId, Rng};
use embassy_executor::Spawner;
#[cfg(feature = "net-esp-now")]
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
use embassy_time::{Duration, Timer};
use esp_backtrace as _;
#[cfg(any(feature = "net-irda", feature = "storage-sd"))]
use esp_hal::gpio::{Level, Output};
use esp_hal::{
    clock::CpuClock,
    gpio::{AnyPin, Input, Pull},
    interrupt::{software::SoftwareInterruptControl, Priority},
    peripherals::TIMG1,
    timer::timg::TimerGroup,
};
use esp_hal_embassy::{main, InterruptExecutor};
#[cfg(feature = "net-irda")]
use esp_irda_transceiver::IrdaTransceiver;
use esp_rmt_neopixel::{Neopixel, RgbU8};
use esp_storage::FlashStorage;
#[cfg(feature = "net-esp-now")]
use esp_wifi::{esp_now::EspNowManager, init, EspWifiController};
use hardware::neopixel::NEOPIXEL_SIGNAL;
use log::info;
use net::NetworkEngine;
use parameter_store::{EmbeddedStorageIO, ParameterStore, ParameterStoreError, Parameters};
use static_cell::StaticCell;

use crate::{
    application::BUTTON_CHANNEL,
    aranya::policy,
    hardware::neopixel::{rainbow_at, MessageState, NeopixelMessage},
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

    #[cfg(any(feature = "net-irda", feature = "storage-sd"))]
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

    let device_id = match parameter_values.device_id {
        None => {
            let device_id = DeviceId::random(&mut Rng::default());
            parameters
                .update(|p| p.device_id = Some(device_id.into()))
                .expect("could not update device ID");
            device_id
        }
        Some(id) => id.into(),
    };
    log::info!("Device ID is {device_id}");

    let mut network_engines: heapless::Vec<&'static dyn NetworkEngine, MAX_NETWORK_ENGINES> =
        heapless::Vec::new();

    #[cfg(feature = "net-esp-now")]
    {
        use esp_hal::gpio::{Level, Output};

        let rng = esp_hal::rng::Rng::new(peripherals.RNG);
        let init = &*mk_static!(
            EspWifiController<'static>,
            init(timer_g0.timer0, rng, peripherals.RADIO_CLK).unwrap()
        );

        let wifi = peripherals.WIFI;
        let esp_now = esp_wifi::esp_now::EspNow::new(&init, wifi).unwrap();
        log::info!("esp-now version {}", esp_now.version().unwrap());

        let (manager, sender, receiver) = esp_now.split();
        let _manager: &'static mut EspNowManager<'static> =
            mk_static!(EspNowManager<'static>, manager);
        let receiver = Mutex::<CriticalSectionRawMutex, _>::new(receiver);
        let sender = Mutex::<CriticalSectionRawMutex, _>::new(sender);
        let tx_led = Output::new(peripherals.GPIO10, Level::Low);
        let rx_led = Output::new(peripherals.GPIO11, Level::Low);

        let engine =
            net::espnow::start(sender, receiver, parameter_values.address, tx_led, rx_led).await;

        daemon.add_esp_now_interface(engine.interface(), graph_id);

        if network_engines.push(engine).is_err() {
            log::info!("could not start ESP Now network engine");
        }
    };

    #[cfg(feature = "net-irda")]
    if let Some(ir) = board_def.ir {
        if let Some(acc_power) = &mut acc_power {
            acc_power.set_high();
        }
        let irts = IrdaTransceiver::new(peripherals.UART1, ir.tx, ir.rx, ir.en);
        let engine = net::irda::start(irts, parameter_values.address).await;

        daemon.add_irda_interface(engine.interface(), graph_id);

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

    spawner.must_spawn(watchdog::idle_task0(wdt0));

    spawner.must_spawn(aranya::daemon::daemon_task(daemon, graph_id));

    spawner.must_spawn(application::app_task(device_id));

    spawner.must_spawn(application::serial::usb_serial_task(
        peripherals.USB0,
        peripherals.GPIO20,
        peripherals.GPIO19,
        device_id,
    ));

    spawner.must_spawn(button_task(board_def.button, parameters));
    spawner.must_spawn(led_task(neopixel));

    spawner.must_spawn(heap_report());
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
    mut parameters: ParameterStore<Parameters, EmbeddedStorageIO<FlashStorage>>,
) {
    let mut driver = Input::new(pin, Pull::Up);
    loop {
        driver.wait_for_falling_edge().await;
        match embassy_time::with_timeout(Duration::from_secs(10), driver.wait_for_high()).await {
            Ok(_) => {
                BUTTON_CHANNEL.send(()).await;
            }
            Err(_te) => {
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

const MENTION_CURVE: [u8; 8] = [5, 15, 25, 34, 50, 40, 25, 10];
const MESSAGES_CURVE: [u8; 4] = [10, 20, 10, 0];

#[embassy_executor::task]
async fn led_task(mut neopixel: Neopixel<'static>) {
    let mut state = MessageState::default();
    let mut ambient_color = RgbU8::default();
    let mut phase = 0usize;
    let mut counter = 0;
    let mut output_color = RgbU8::default();
    let mut new_color = RgbU8::default();

    neopixel.set_power(true);
    neopixel.set_color(40, 10, 0).ok();
    Timer::after_millis(500).await;
    neopixel.set_color(0, 0, 0).ok();

    loop {
        match embassy_time::with_timeout(Duration::from_millis(100), NEOPIXEL_SIGNAL.wait()).await {
            Ok(nm) => match nm {
                NeopixelMessage::MessageState(ms) => {
                    state = ms;
                    phase = 0;
                    counter = 10;
                }
                NeopixelMessage::Rainbow => {
                    for _i in 0..3 {
                        for hue in (0..360).step_by(6) {
                            let (red, green, blue) = rainbow_at(hue);
                            neopixel.set_color(red, green, blue).ok();
                            Timer::after_millis(30).await;
                        }
                    }
                    neopixel
                        .set_color(ambient_color.red, ambient_color.green, ambient_color.blue)
                        .ok();
                }
                NeopixelMessage::Ambient { color } => {
                    ambient_color = match color {
                        policy::AmbientColor::Black => RgbU8 {
                            red: 0,
                            green: 0,
                            blue: 0,
                        },
                        policy::AmbientColor::Blue => RgbU8 {
                            red: 0,
                            green: 0,
                            blue: 10,
                        },
                        policy::AmbientColor::Red => RgbU8 {
                            red: 10,
                            green: 0,
                            blue: 0,
                        },
                        policy::AmbientColor::Green => RgbU8 {
                            red: 0,
                            green: 10,
                            blue: 0,
                        },
                        policy::AmbientColor::Magenta => RgbU8 {
                            red: 5,
                            green: 0,
                            blue: 5,
                        },
                        policy::AmbientColor::Cyan => RgbU8 {
                            red: 0,
                            green: 5,
                            blue: 5,
                        },
                        policy::AmbientColor::Yellow => RgbU8 {
                            red: 5,
                            green: 5,
                            blue: 0,
                        },
                        policy::AmbientColor::White => RgbU8 {
                            red: 3,
                            green: 3,
                            blue: 3,
                        },
                    };
                }
            },
            Err(_) => {
                if counter > 0 {
                    counter -= 1;
                }
                log::debug!(
                    "neopixel: {state:?} phase:{phase} c:{counter} output_color:{new_color:?}"
                );
                match phase {
                    // Idle
                    0 => {
                        new_color = ambient_color;
                    }
                    1 => {
                        if state.mentioned {
                            new_color = RgbU8 {
                                red: MENTION_CURVE[counter],
                                green: 0,
                                blue: MENTION_CURVE[counter],
                            };
                        }
                    }
                    2 => {
                        if state.unseen_count > 0 {
                            let cc = counter & 0x03;
                            new_color = RgbU8 {
                                red: 0,
                                green: MESSAGES_CURVE[cc],
                                blue: 0,
                            };
                        }
                    }
                    _ => unreachable!(),
                }
                if counter == 0 {
                    phase = (phase + 1) % 3;
                    counter = match phase {
                        0 => 10,
                        1 => 8,
                        2 => {
                            if state.unseen_count > 5 {
                                20
                            } else {
                                state.unseen_count * 4
                            }
                        }
                        _ => unreachable!(),
                    }
                }
            }
        };
        if new_color != output_color {
            output_color = new_color;
            neopixel
                .set_color(output_color.red, output_color.green, output_color.blue)
                .inspect_err(|e| log::error!("neopixel: {e}"))
                .ok();
        }
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
