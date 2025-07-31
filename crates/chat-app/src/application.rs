extern crate alloc;

pub mod serial;

use core::fmt;

use alloc::{
    boxed::Box,
    string::{String, ToString},
};

use aranya_crypto::{DeviceId, Id};
use aranya_policy_vm::Text;
use aranya_runtime::{vm_action, CommandId, VmEffect};
use embassy_futures::select::{select3, Either3};
use embassy_time::Instant;
use esp_println::println;

use crate::{
    application::serial::{SerialCommand, SerialResponse},
    aranya::{
        daemon::{ACTION_IN_CHANNEL, EFFECT_OUT_CHANNEL},
        policy::MessageReceived,
    },
    hardware::neopixel::{NeopixelState, NEOPIXEL_SIGNAL},
};

type Channel<T> = embassy_sync::channel::Channel<
    embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex,
    T,
    2,
>;

pub static SERIAL_IN_CHANNEL: Channel<SerialCommand> = Channel::new();
pub static SERIAL_OUT_CHANNEL: Channel<SerialResponse> = Channel::new();
pub static BUTTON_CHANNEL: Channel<()> = Channel::new();

#[derive(Debug, thiserror::Error)]
pub struct NotMessageReceived();

impl fmt::Display for NotMessageReceived {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Not MessageReceived")
    }
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    /// This timestamp does not store an authoritative time value, just
    /// a relative one for query purposes.
    ts: Instant,
    id: CommandId,
    author: DeviceId,
    msg: String,
}

impl TryFrom<VmEffect> for ChatMessage {
    type Error = NotMessageReceived;

    fn try_from(value: VmEffect) -> Result<Self, Self::Error> {
        let mr: MessageReceived = value.fields.try_into().map_err(|_| NotMessageReceived())?;
        Ok(ChatMessage {
            ts: Instant::now(),
            id: value.command,
            author: mr.author.into(),
            msg: mr.msg.to_string(),
        })
    }
}

pub struct Application {
    device_id: DeviceId,
    chat_buffer: heapless::spsc::Queue<Box<ChatMessage>, 100>,
    unseen_count: usize,
    mentioned: bool,
}

impl Application {
    pub fn new(device_id: DeviceId) -> Application {
        Application {
            device_id,
            chat_buffer: heapless::spsc::Queue::new(),
            unseen_count: 0,
            mentioned: false,
        }
    }

    pub async fn run(&mut self) {
        let mut effect_subscriber = EFFECT_OUT_CHANNEL
            .subscriber()
            .expect("application could not get subscriber slot");

        loop {
            let selected = select3(
                effect_subscriber.next_message_pure(),
                SERIAL_IN_CHANNEL.receive(),
                BUTTON_CHANNEL.receive(),
            )
            .await;
            match selected {
                Either3::First(effect) => {
                    if effect.recalled {
                        continue;
                    }
                    match effect.name.as_str() {
                        "MessageReceived" => {
                            let command_id = effect.command;
                            let chatmsg: ChatMessage = effect
                                .try_into()
                                .expect("Got some effect other than MessageReceived somehow");
                            if self.chat_buffer.is_full() {
                                self.chat_buffer.dequeue();
                            }
                            if chatmsg.author != self.device_id {
                                self.unseen_count += 1;
                                self.update_neopixel();
                            }
                            if !self.chat_buffer.iter().any(|msg| msg.id == command_id) {
                                self.chat_buffer.enqueue(Box::new(chatmsg)).ok();
                            }
                        }
                        _ => (),
                    };
                }
                Either3::Second(ser_cmd) => {
                    println!("application received command: {ser_cmd:?}");
                    match ser_cmd {
                        SerialCommand::SendMessage(msg) => {
                            let msg: Text = msg.try_into().expect("invalid string");
                            ACTION_IN_CHANNEL
                                .send(vm_action!(send_message(self.device_id, msg)).into())
                                .await;
                            SERIAL_OUT_CHANNEL.send(SerialResponse::Sent).await;
                        }
                        SerialCommand::GetMessages(instant) => {
                            let msgs = self
                                .chat_buffer
                                .iter()
                                .filter_map(|i| {
                                    if i.ts > instant {
                                        Some(i.as_ref().clone())
                                    } else {
                                        None
                                    }
                                })
                                .collect();
                            SERIAL_OUT_CHANNEL
                                .send(SerialResponse::MessageData(msgs))
                                .await;
                            self.unseen_count = 0;
                            self.mentioned = false;
                        }
                    }
                }
                Either3::Third(_) => {
                    self.unseen_count = 0;
                    self.mentioned = false;
                    self.update_neopixel();
                }
            }
            println!("application processing done");
        }
    }

    fn update_neopixel(&self) {
        NEOPIXEL_SIGNAL.signal(NeopixelState {
            unseen_count: self.unseen_count,
            mentioned: self.mentioned,
        });
    }
}

#[embassy_executor::task]
pub async fn app_task(device_id: DeviceId) {
    let mut application = Application::new(device_id);
    application.run().await;
}
