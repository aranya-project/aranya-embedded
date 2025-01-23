use embedded_sdmmc::{TimeSource, Timestamp};
use esp_hal::prelude::*;
use esp_hal::{
    timer::timg::{Instance, Timer},
    Blocking,
};

/*
The timer works off microseconds. We can change the prescaler to make it milli or normal seconds but focus was on other matters.
*/

// Instance is a trait within timg or the Timer Group module that holds a particular instance of a timer. It implements the main timing methods such as `.start()` or `now()`. Timer is a General purpose timer struct. In the ESP32 there are 2 timer groups, Timer0 and Timer1. TimerX indicates that we accept either of them. Within each Timer driver you can chose for its functionality to be Blocking or Async. In short timer selects that we want a Timer driver of either timer group that's blocking. Within TimerX we hold the actual peripheral timer of the timer group that we're referring to, such as TIMG0 or TIMG1. In short the signature is Driver<TimerGroup<TimerPeripheral>,DrivingProtocol>. For additional clarification there are two hardware timer groups, Timer0 and Timer1. Each group has two general-purpose hardware timers TIMG0 or TIMG1 for a total of 4 timers.
pub struct Esp32TimeSource<I>
where
    I: Instance,
{
    timer: Timer<I, Blocking>,
    start_time: Timestamp,
}

impl<I> Esp32TimeSource<I>
where
    // Instance is a trait defined in the ESP32 HAL (Hardware Abstraction Layer) that represents the common functionality for timer instances
    I: Instance,
{
    pub fn new(timer: Timer<I, Blocking>, start_time: Timestamp) -> Self {
        let esp32_timer = Self { timer, start_time };
        esp32_timer.timer.reset();
        esp32_timer.timer.start();
        esp32_timer
    }

    pub fn synchronize(&mut self, current_time: Timestamp) {
        self.start_time = current_time;
        self.timer.reset();
        self.timer.start();
    }
}
/*
Max of u64 is 18446744073709551615 so to estimate the max of the timer in years we divide that by 1000000, 60, 60, 24, 365 for microseconds to seconds to minutes to hours to days to years. It will last 584942.417355 years assuming no synchronization.
*/
impl<I> TimeSource for Esp32TimeSource<I>
where
    I: Instance,
{
    fn get_timestamp(&self) -> Timestamp {
        let elapsed_seconds = self.timer.now().ticks() / 1_000_000;
        let mut timestamp = self.start_time;

        const SECONDS_PER_MINUTE: u64 = 60;
        const SECONDS_PER_HOUR: u64 = 60 * 60;
        const SECONDS_PER_DAY: u64 = 24 * 60 * 60;
        const SECONDS_PER_MONTH: u64 = 30 * 24 * 60 * 60; // Simplified, assuming 30-day months
        const SECONDS_PER_YEAR: u64 = 365 * 24 * 60 * 60; // Simplified, not accounting for leap years

        // Calculate years
        let years = elapsed_seconds / SECONDS_PER_YEAR;
        timestamp.year_since_1970 = timestamp.year_since_1970.saturating_add(years as u8);
        let mut remaining_seconds = elapsed_seconds % SECONDS_PER_YEAR;

        // Calculate months
        let months = remaining_seconds / SECONDS_PER_MONTH;
        timestamp.zero_indexed_month = (timestamp.zero_indexed_month as u64 + months) as u8 % 12;
        remaining_seconds %= SECONDS_PER_MONTH;

        // Calculate days
        let days = remaining_seconds / SECONDS_PER_DAY;
        timestamp.zero_indexed_day = (timestamp.zero_indexed_day as u64 + days) as u8 % 30;
        remaining_seconds %= SECONDS_PER_DAY;

        // Calculate hours
        let hours = remaining_seconds / SECONDS_PER_HOUR;
        timestamp.hours = (timestamp.hours as u64 + hours) as u8 % 24;
        remaining_seconds %= SECONDS_PER_HOUR;

        // Calculate minutes
        let minutes = remaining_seconds / SECONDS_PER_MINUTE;
        timestamp.minutes = (timestamp.minutes as u64 + minutes) as u8 % 60;

        // Calculate seconds
        let seconds = remaining_seconds % SECONDS_PER_MINUTE;
        timestamp.seconds = (timestamp.seconds as u64 + seconds) as u8 % 60;

        timestamp
    }
}

// todo Add proper timing to files storage
