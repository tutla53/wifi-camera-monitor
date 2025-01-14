/*
    OLED Display Task
*/

use {
    embassy_rp::i2c::{
        I2c,
        Config,
    },
    crate::resources::gpio_list::{
        Irqs,
        DisplayResources,
    },
    ssd1306::{
        I2CDisplayInterface,
        Ssd1306,
        mode::DisplayConfig,
        prelude::DisplayRotation,
        size::DisplaySize128x64,
    },
    embassy_sync::{
        signal::Signal,
        blocking_mutex::raw::CriticalSectionRawMutex,
    },
    embassy_net::Ipv4Cidr,
    core::fmt::Write,
};

const ROW_IP: u8 = 0;
const ROW_IP_ADRR: u8 = 1;

const ROW_STATUS: u8 = 3;
const ROW_STATUS_MSG: u8 = 4;

static DISPLAY_CONTROL: Signal<CriticalSectionRawMutex, Command> = Signal::new();

pub enum Command {
    Addr(Ipv4Cidr),
    Status(usize)
}

pub fn send_command(command: Command) {
    DISPLAY_CONTROL.signal(command);
}

async fn wait_command() -> Command {
    DISPLAY_CONTROL.wait().await
}

const ENCODE_CODE: [&str; 18] = [
    "Success                                        ", 
    "Operation Failed                               ",
    "Time Out                                       ",
    "No Matching Network Found                      ",
    "Aborted                                        ",
    "Packet not acknowledged                        ",
    "AUTH or ASSOC packet was unsolicited           ",
    "Attempt to ASSOC to an auto auth configuration ",
    "Incomplete Scan                                ",
    "Scan aborted by another scan                   ",
    "Scan aborted due to assoc in progress          ",
    "802.11h quiet period started                   ",
    "User disabled scanning (WLC_SET_SCANSUPPRESS)  ",
    "No allowable channels to scat                  ",
    "Scan aborted due to CCX fast roam              ",
    "Abort channel select                           ",
    "Listening on    Port 1234                     ",
    "TCP is Connected                               "

];

#[embassy_executor::task]
pub async fn display(r: DisplayResources) {
    let i2c0 = I2c::new_async(r.I2C_CH, r.SCL_PIN, r.SDA_PIN, Irqs, Config::default());
    let interface = I2CDisplayInterface::new(i2c0);
    let mut display = Ssd1306::new(interface, DisplaySize128x64, DisplayRotation::Rotate0).into_terminal_mode();
    
    match display.init(){
        Ok(()) => {log::warn!("Display has been Initialized")}
        Err(e) => {
            log::warn!("Write Error: {:?}", e);
        }
    };

    let mut buffer = itoa::Buffer::new();

    display.clear().unwrap();
    display.set_position(0, ROW_IP).unwrap();
    let _ = display.write_str("IP Address:");
    display.set_position(0, ROW_STATUS).unwrap();
    let _ = display.write_str("Status:");

    loop{
        let command = wait_command().await;
        
        match command {
            Command::Addr(ipv4) => {
                let pico_addr = ipv4.address().octets();
                display.set_position(0, ROW_IP_ADRR).unwrap();

                let s: &str = buffer.format(pico_addr[0]);
                let _ = display.write_str(s);
                let _ = display.write_str(".");
                let s: &str = buffer.format(pico_addr[1]);
                let _ = display.write_str(s);
                let _ = display.write_str(".");
                let s: &str = buffer.format(pico_addr[2]);
                let _ = display.write_str(s);
                let _ = display.write_str(".");
                let s: &str = buffer.format(pico_addr[3]);
                let _ = display.write_str(s);
            },
            Command::Status(value) => {
                display.set_position(0, ROW_STATUS_MSG).unwrap();
                let _ = display.write_str(ENCODE_CODE[value]);
            },
        }
    }

}