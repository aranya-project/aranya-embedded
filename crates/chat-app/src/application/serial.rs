extern crate alloc;

use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use bytes::{BufMut, BytesMut};
use core::fmt::Write;
use embassy_futures::{
    join::join,
    select::{select, Either},
};
use embassy_time::Instant;
use embassy_usb::{
    class::cdc_acm,
    driver::EndpointError,
    msos::{self, windows_version},
    types::InterfaceNumber,
    Builder,
};
use esp_hal::{gpio::GpioPin, otg_fs, peripherals::USB0};
use esp_println::println;

use crate::application::ChatMessage;
use crate::application::{SERIAL_IN_CHANNEL, SERIAL_OUT_CHANNEL};

const MAX_SERIAL_PACKET_SIZE: u16 = 64;
const WEB_SOURCE: &'static str = include_str!("../../web/client.html");
const DEVICE_INTERFACE_GUIDS: &[&str] = &["{63788892-2A36-4357-AFD0-008A6570D80A}"];

#[derive(Debug)]
pub enum SerialCommand {
    SendMessage(String),
    GetMessages(Instant),
}

#[derive(Debug)]
pub enum SerialResponse {
    // Response from a 'getmsgs' query
    MessageData(Vec<ChatMessage>),
    // A message has been successfully sent
    Sent,
}

#[embassy_executor::task]
pub async fn usb_serial_task(usb0: USB0, usb_dp: GpioPin<20>, usb_dm: GpioPin<19>) {
    let usb = otg_fs::Usb::new(usb0, usb_dp, usb_dm);
    let mut ep_out_buffer = [0u8; 1024];
    let config = otg_fs::asynch::Config::default();
    let driver = otg_fs::asynch::Driver::new(usb, &mut ep_out_buffer, config);

    let mut config = embassy_usb::Config::new(0x303A, 0x3001);
    config.manufacturer = Some("SpiderOak");
    config.product = Some("Demo Board V2");
    config.serial_number = Some("2");
    config.device_class = 0xEF;
    config.device_sub_class = 0x02;
    config.device_protocol = 0x01;
    config.composite_with_iads = true;

    let mut config_descriptor = [0u8; 256];
    let mut bos_descriptor = [0u8; 256];
    let mut msos_descriptor = [0u8; 256];
    let mut control_buf = [0u8; 64];

    let mut state = cdc_acm::State::new();
    let mut builder = Builder::new(
        driver,
        config,
        &mut config_descriptor,
        &mut bos_descriptor,
        &mut msos_descriptor,
        &mut control_buf,
    );

    builder.msos_descriptor(windows_version::WIN8_1, 2);

    let mut class = cdc_acm::CdcAcmClass::new(&mut builder, &mut state, MAX_SERIAL_PACKET_SIZE);

    builder.msos_writer().configuration(0);
    builder.msos_writer().function(InterfaceNumber(0));
    builder
        .msos_writer()
        .function_feature(msos::CompatibleIdFeatureDescriptor::new("WINUSB", ""));
    builder
        .msos_writer()
        .function_feature(msos::RegistryPropertyFeatureDescriptor::new(
            "DeviceInterfaceGUIDs",
            msos::PropertyData::RegMultiSz(DEVICE_INTERFACE_GUIDS),
        ));

    let mut usb = builder.build();
    let usb_fut = usb.run();

    let app_fut = async {
        loop {
            class.wait_connection().await;
            let mut sce = SerialCommandEngine::new(&mut class);
            sce.io_loop().await.expect("USB failure");
        }
    };

    join(usb_fut, app_fut).await;
}

#[derive(Debug)]
enum SerialCommandState {
    Idle,
    Command,
    Data,
}

const SOH: u8 = 0x01;
const STX: u8 = 0x02;
const ETX: u8 = 0x03;
const EOT: u8 = 0x04;
const CR: u8 = 0x0D;
const SUB: u8 = 0x01A;
const ESC: u8 = 0x1B;
const SP: u8 = 0x20;

fn valid_text_char(c: u8) -> bool {
    c == 0x07 || (c >= 0x09 && c <= 0x0D) || (c >= 0x20 && c < 0x7F)
}

pub struct SerialCommandEngine<'d, 'a> {
    class: &'d mut cdc_acm::CdcAcmClass<'a, otg_fs::asynch::Driver<'a>>,
}

impl<'d, 'a> SerialCommandEngine<'d, 'a> {
    fn new(
        class: &'d mut cdc_acm::CdcAcmClass<'a, otg_fs::asynch::Driver<'a>>,
    ) -> SerialCommandEngine<'d, 'a> {
        SerialCommandEngine { class }
    }

    async fn io_loop(&mut self) -> Result<(), EndpointError> {
        let mut buf = [0u8; 64];
        let mut scs = SerialCommandState::Idle;
        let mut command: heapless::String<8> = heapless::String::new();
        let mut data: heapless::String<80> = heapless::String::new();

        loop {
            let selected = select(
                self.class.read_packet(&mut buf),
                SERIAL_OUT_CHANNEL.receive(),
            )
            .await;
            match selected {
                Either::First(n) => {
                    let n = n?;
                    for c in &buf[0..n] {
                        /* if *c > 0x1F {
                            self.class.write_packet(&[*c]).await?;
                        } else {
                            self.class
                                .write_packet(&alloc::format!("<{:02X}>", c).as_bytes())
                                .await?;
                        } */
                        if *c == SOH {
                            command.clear();
                            data.clear();
                            scs = SerialCommandState::Command;
                            continue;
                        }
                        if *c == ESC {
                            scs = SerialCommandState::Data;
                            continue;
                        }
                        match scs {
                            SerialCommandState::Idle => match *c {
                                CR => {
                                    self.class
                                        .write_packet(
                                            "Serial ready; press ^Z to download client\r\n"
                                                .as_bytes(),
                                        )
                                        .await?;
                                }
                                SUB => {
                                    self.class
                                        .write_packet("----- 8< CUT HERE 8< -----\r\n".as_bytes())
                                        .await?;
                                    for line in WEB_SOURCE.split("\n") {
                                        self.send_buffer(line.as_bytes()).await?;
                                        self.class.write_packet("\r\n".as_bytes()).await?;
                                    }
                                    self.class
                                        .write_packet("----- 8< CUT HERE 8< -----\r\n".as_bytes())
                                        .await?;
                                }
                                _ => (),
                            },
                            SerialCommandState::Command => match *c {
                                STX => {
                                    scs = SerialCommandState::Data;
                                }
                                c if c >= 0x20 && c < 0x7F && command.len() <= 8 => {
                                    command.push(c.into()).ok();
                                }
                                _ => {
                                    self.class.write_packet(&[ESC]).await?;
                                }
                            },
                            SerialCommandState::Data => match *c {
                                EOT => {
                                    self.handle_serial_command(&command, &data).await;
                                    scs = SerialCommandState::Idle;
                                }
                                c if valid_text_char(c) && data.len() <= 64 => {
                                    data.push(c.into()).ok();
                                }
                                _ => {
                                    self.class.write_packet(&[ESC]).await?;
                                }
                            },
                        }
                    }
                }
                Either::Second(response) => match response {
                    SerialResponse::MessageData(d) => {
                        let mut msgbuf = BytesMut::with_capacity(256);
                        for cm in d {
                            write!(
                                msgbuf,
                                "{} {} {}{}",
                                cm.author,
                                cm.ts.as_ticks(),
                                cm.msg,
                                ETX as char
                            )
                            .expect("message should fit");
                        }

                        self.send_response("msgdata", &msgbuf).await?;
                    }
                    SerialResponse::Sent => self.send_response("sent", &[]).await?,
                },
            }
        }
    }

    async fn handle_serial_command(&self, command: &str, data: &str) {
        let sc = match command {
            "sendmsg" => SerialCommand::SendMessage(data.to_string()),
            "getmsgs" => SerialCommand::GetMessages(Instant::from_ticks(
                u64::from_str_radix(data, 10).expect("bad instant"),
            )),
            _ => {
                println!("Unknown serial command `{command}`");
                return;
            }
        };
        println!("command: {sc:?}");
        SERIAL_IN_CHANNEL.send(sc).await;
    }

    async fn send_response(&mut self, name: &str, data: &[u8]) -> Result<(), EndpointError> {
        let mut buf = BytesMut::with_capacity(256);
        buf.put_u8(SOH);
        buf.put_slice(name.as_bytes());
        buf.put_u8(STX);
        buf.put_slice(data);
        buf.put_u8(EOT);
        self.send_buffer(&buf).await?;
        Ok(())
    }

    async fn send_buffer(&mut self, buf: &[u8]) -> Result<(), EndpointError> {
        for c in buf.chunks(MAX_SERIAL_PACKET_SIZE as usize) {
            self.class.write_packet(c).await?;
        }
        Ok(())
    }
}
