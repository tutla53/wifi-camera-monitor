//! This example uses the RP Pico W board Wifi chip (cyw43).
//! Connects to specified Wifi network and creates a TCP endpoint on port 1234.

#![no_std]
#![no_main]
#![allow(async_fn_in_trait)]

use {
    core::str::from_utf8,
    cyw43::JoinOptions,
    cyw43_pio::PioSpi,
    embassy_executor::Spawner,
    embassy_net::{
        tcp::TcpSocket,
        Config, 
        StackResources,
    },
    embassy_rp::{
        bind_interrupts,
        clocks::RoscRng,
        gpio::{
            Level, 
            Output,
        },
        peripherals::{
            DMA_CH0, 
            PIO0, 
            USB,
        },
        pio::{
            InterruptHandler as PioInterruptHandler, 
            Pio,
        },
        usb::{
            InterruptHandler as UsbInterruptHandler, 
            Driver,
        },
    },
    embassy_time::{Duration, Timer},
    embedded_io_async::Write,
    rand::RngCore,
    static_cell::StaticCell,
    defmt::*,
    {defmt_rtt as _, panic_probe as _},
};

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => PioInterruptHandler<PIO0>;
    USBCTRL_IRQ => UsbInterruptHandler<USB>;
});

#[embassy_executor::task]
async fn logger_task(driver: Driver<'static, USB>) {
    embassy_usb_logger::run!(1024, log::LevelFilter::Info, driver);
}

const WIFI_NETWORK: &str = env!("WIFI_NETWORK");
const WIFI_PASSWORD: &str = env!("WIFI_PASSWORD");

#[embassy_executor::task]
async fn cyw43_task(runner: cyw43::Runner<'static, Output<'static>, PioSpi<'static, PIO0, 0, DMA_CH0>>) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, cyw43::NetDriver<'static>>) -> ! {
    runner.run().await
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    
    let p = embassy_rp::init(Default::default());
    let driver = Driver::new(p.USB, Irqs);
    spawner.spawn(logger_task(driver)).unwrap();
    
    log::info!("Hello World!");

    let mut rng = RoscRng;
    let fw = include_bytes!("../cyw43-firmware/43439A0.bin");
    let clm = include_bytes!("../cyw43-firmware/43439A0_clm.bin");

    let pwr = Output::new(p.PIN_23, Level::Low);
    let cs = Output::new(p.PIN_25, Level::High);
    let mut pio = Pio::new(p.PIO0, Irqs);
    let spi = PioSpi::new(&mut pio.common, pio.sm0, pio.irq0, cs, p.PIN_24, p.PIN_29, p.DMA_CH0);

    static STATE: StaticCell<cyw43::State> = StaticCell::new();
    let state = STATE.init(cyw43::State::new());
    let (net_device, mut control, runner) = cyw43::new(state, pwr, spi, fw).await;
    unwrap!(spawner.spawn(cyw43_task(runner)));

    control.init(clm).await;
    control
        .set_power_management(cyw43::PowerManagementMode::PowerSave)
        .await;

    // Using DHCP config for the ipv4 address
    let config = Config::dhcpv4(Default::default());

    // Generate random seed
    let seed = rng.next_u64();

    // Init network stack
    static RESOURCES: StaticCell<StackResources<3>> = StaticCell::new();
    let (stack, runner) = embassy_net::new(net_device, config, RESOURCES.init(StackResources::new()), seed);

    unwrap!(spawner.spawn(net_task(runner)));

    loop {
        match control
            .join(WIFI_NETWORK, JoinOptions::new(WIFI_PASSWORD.as_bytes()))
            .await
        {
            Ok(_) => break,
            Err(err) => {
                log::info!("join failed with status={}", err.status);
            }
        }
    }

    // Wait for DHCP, not necessary when using static IP
    info!("Waiting for DHCP...");
    while !stack.is_config_up() {
        Timer::after_millis(100).await;
    }
    log::info!("DHCP is now up!");

    // And now we can use it!

    let mut rx_buffer = [0; 4096];
    let mut tx_buffer = [0; 4096];
    let mut buf = [0; 4096];

    loop {
        let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
        socket.set_timeout(Some(Duration::from_secs(600)));

        control.gpio_set(0, false).await;

        match stack.config_v4(){
            Some(value) => log::info!("Server Address: {:?}", value.address.address()),
            None => log::warn!("Unable to Get the Adrress")
        } 

        log::info!("Listening on TCP:1234...");

        if let Err(e) = socket.accept(1234).await {
            log::warn!("accept error: {:?}", e);
            continue;
        }

        log::info!("Received connection from {:?}", socket.remote_endpoint());
        control.gpio_set(0, true).await;

        loop {
            let n = match socket.read(&mut buf).await {
                Ok(0) => {
                    log::warn!("read EOF");
                    break;
                }
                Ok(n) => n,
                Err(e) => {
                    log::warn!("read error: {:?}", e);
                    break;
                }
            };

            log::info!("rxd {}", from_utf8(&buf[..n]).unwrap());

            match socket.write_all(&buf[..n]).await {
                Ok(()) => {}
                Err(e) => {
                    log::warn!("write error: {:?}", e);
                    break;
                }
            };
        }
    }
}