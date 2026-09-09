#![no_std]
#![no_main]

use core::net::{Ipv4Addr, SocketAddr};

use defmt::*;
use embassy_executor::Spawner;
use embassy_nats::UserPwdAuthenticator;
use embassy_net::dns::DnsQueryType;
use embassy_net::tcp::TcpSocket;
use embassy_net::{Ipv4Cidr, Stack, StackResources, StaticConfigV4};
use embassy_stm32::eth::{self, Ethernet, GenericPhy, PacketQueue, Sma};
use embassy_stm32::flash::{self, Flash};
use embassy_stm32::gpio::{Output, Speed};
use embassy_stm32::peripherals::{ETH, ETH_SMA, IWDG1, RNG};
use embassy_stm32::rng::Rng;
use embassy_stm32::timer::low_level::TriggerSource;
use embassy_stm32::wdg::IndependentWatchdog;
use embassy_stm32::{Config, gpio::Level};
use embassy_stm32::{bind_interrupts, rcc, rng};
use embassy_time::{Duration, Timer};
use embedded_alloc::LlffHeap as Heap;
use heapless::Vec;
use static_cell::StaticCell;

use crate::drivers::stepper::step_counter::StepCounter;
use crate::drivers::stepper::step_interface::StepInterface;
use crate::drivers::stepper::{Stepper, step_interface::PulsePin};

use crate::firmware_manager::{FirmwareManager, FirmwareManagerStorage};
use crate::logger::TcpEncoder;

use panic_reset as _;

mod drivers;
mod firmware_manager;
mod logger;

// General setup stuff
const WATCHDOG_TIMEOUT_US: u32 = 5_000_000;
const WATCHDOG_PETTING_INTERVAL_US: u32 = WATCHDOG_TIMEOUT_US / 2;

const IP_CONFIG: StaticConfigV4 = StaticConfigV4 {
    address: Ipv4Cidr::new(Ipv4Addr::new(192, 168, 0, 10), 24),
    gateway: None,
    dns_servers: Vec::from_array([Ipv4Addr::new(192, 168, 0, 1)]),
};

const LOGGER_PORT: u16 = 3001;

const FIRMWARE_UPDATE_PORT: u16 = 3000;

// Storage setup
static FIRMWARE_STORAGE: StaticCell<FirmwareManagerStorage> = StaticCell::new();

// Heap setup
const HEAP_KB: usize = 64;

extern crate alloc;

#[global_allocator]
static HEAP: Heap = Heap::empty();

// Ethernet
// queues for raw packets before and after processing
static PACKET_QUEUE: StaticCell<PacketQueue<4, 4>> = StaticCell::new();
// resources to hold the sockets used by the net driver.
// One for DHCP, one for DNS, one for the NTP UDP socket and one for the NATS TCP socket
static RESOURCES: StaticCell<StackResources<4>> = StaticCell::new();

// buffer sizes for tcp data before and after processing
const NATS_TCP_RX_BUF_SIZE: usize = 1024;
static NATS_TCP_RX_BUF: StaticCell<[u8; NATS_TCP_RX_BUF_SIZE]> = StaticCell::new();

const NATS_TCP_TX_BUF_SIZE: usize = 1024;
static NATS_TCP_TX_BUF: StaticCell<[u8; NATS_TCP_TX_BUF_SIZE]> = StaticCell::new();

const UPD_TCP_RX_BUF_SIZE: usize = 1024;
static UPD_TCP_RX_BUF: StaticCell<[u8; UPD_TCP_RX_BUF_SIZE]> = StaticCell::new();

const UPD_TCP_TX_BUF_SIZE: usize = 1024;
static UPD_TCP_TX_BUF: StaticCell<[u8; UPD_TCP_TX_BUF_SIZE]> = StaticCell::new();

const LOG_TCP_RX_BUF_SIZE: usize = 1024;
static LOG_TCP_RX_BUF: StaticCell<[u8; LOG_TCP_RX_BUF_SIZE]> = StaticCell::new();

const LOG_TCP_TX_BUF_SIZE: usize = 1024;
static LOG_TCP_TX_BUF: StaticCell<[u8; LOG_TCP_TX_BUF_SIZE]> = StaticCell::new();

// NATS
static NATS_STORAGE: embassy_nats::Storage = embassy_nats::Storage::new();
const NATS_ADDR: &str = "nats.lan";
const NATS_PORT: u16 = 4222;
const NATS_USER: &str = "nats";
const NATS_PWD: &str = "south";

static CH: embassy_nats::MsgChannel = embassy_nats::MsgChannel::new();

// Devices
const STEPPS_PER_REV: u32 = 12_000;

type EthDevice = Ethernet<'static, ETH, GenericPhy<Sma<'static, ETH_SMA>>>;

bind_interrupts!(struct Irqs {
    ETH => eth::InterruptHandler;
    FLASH => flash::InterruptHandler;
    RNG => rng::InterruptHandler<RNG>;
});

fn get_rcc_config() -> rcc::Config {
    let mut rcc_config = rcc::Config::default();
    rcc_config.hsi = Some(rcc::HSIPrescaler::DIV1); // 64 MHz
    rcc_config.hsi48 = Some(Default::default()); // needed for RNG

    rcc_config.pll1 = Some(rcc::Pll {
        source: rcc::PllSource::HSI,
        prediv: rcc::PllPreDiv::DIV4,  // 16 MHz
        mul: rcc::PllMul::MUL30,       // 480 MHz
        divp: Some(rcc::PllDiv::DIV2), // 240 MHz
        divq: Some(rcc::PllDiv::DIV8), // 60 MHz
        divr: Some(rcc::PllDiv::DIV8), // 60 MHz
    });
    rcc_config.sys = rcc::Sysclk::PLL1_P; // cpu runns with 240 MHz

    rcc_config.voltage_scale = rcc::VoltageScale::Scale2; // voltage scale for max 300 MHz Pll out

    rcc_config.ahb_pre = rcc::AHBPrescaler::DIV2; // AHB runns at 120 MHz (src: sysclk)
    rcc_config.apb1_pre = rcc::APBPrescaler::DIV2; // APB 1-4 all run with 60 MHz (src: ahb)
    rcc_config.apb2_pre = rcc::APBPrescaler::DIV2;
    rcc_config.apb3_pre = rcc::APBPrescaler::DIV2;
    rcc_config.apb4_pre = rcc::APBPrescaler::DIV2;

    rcc_config
}

/// Watchdog petting task
#[embassy_executor::task]
async fn petter(mut watchdog: IndependentWatchdog<'static, IWDG1>) {
    loop {
        watchdog.pet();
        Timer::after_micros(WATCHDOG_PETTING_INTERVAL_US.into()).await;
    }
}

#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, EthDevice>) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn logger_task(mut runner: TcpEncoder<'static>) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn firmware_manager_task(mut runner: FirmwareManager<'static>) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn nats_task(mut runner: embassy_nats::Runner<'static, UserPwdAuthenticator>) -> ! {
    runner.run().await
}

async fn resolve_addr(
    stack: &Stack<'_>,
    addr: &str,
    port: u16,
) -> Result<SocketAddr, embassy_net::dns::Error> {
    let ips = stack.dns_query(addr, DnsQueryType::A).await?;
    let Some(ip) = ips.first() else {
        return Err(embassy_net::dns::Error::Failed);
    };
    Ok(SocketAddr::new((*ip).into(), port))
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let mut config = Config::default();
    config.rcc = get_rcc_config();
    let p = embassy_stm32::init(config);

    // init global allocator
    unsafe {
        embedded_alloc::init!(HEAP, HEAP_KB * 1024);
    }

    info!("Launching");

    // create independent watchdog,
    let mut watchdog = IndependentWatchdog::new(p.IWDG1, WATCHDOG_TIMEOUT_US);
    watchdog.unleash();

    // Initialize ethernet
    let eth_int = p.ETH;
    let ref_clk = p.PA1;
    let mdio = p.PA2;
    let mdc = p.PC1;
    let crs = p.PA7;
    let rx_d0 = p.PC4;
    let rx_d1 = p.PC5;
    let tx_d0 = p.PB12;
    let tx_d1 = p.PB13;
    let tx_en = p.PB11;
    let sma = p.ETH_SMA;

    info!("Creating Ethernet device...");

    // Generate random seed and mac.
    let mut rng = Rng::new(p.RNG, Irqs);

    let mut seed = [0; 8];
    rng.fill_bytes(&mut seed);
    let seed = u64::from_le_bytes(seed);

    let mut mac_addr = [0; 6];
    rng.fill_bytes(&mut mac_addr);
    // make sure mac addr is unicast
    mac_addr[0] &= !1;

    // Ethernet device setup
    let device = Ethernet::new(
        PACKET_QUEUE.init(PacketQueue::<4, 4>::new()),
        eth_int,
        Irqs,
        ref_clk,
        crs,
        rx_d0,
        rx_d1,
        tx_d0,
        tx_d1,
        tx_en,
        mac_addr,
        sma,
        mdio,
        mdc,
    );

    let net_cfg = embassy_net::Config::ipv4_static(IP_CONFIG);

    // Initialize network stack
    info!("Initializing network task");
    let (stack, runner) =
        embassy_net::new(device, net_cfg, RESOURCES.init(StackResources::new()), seed);

    // Launch watchdog task
    spawner.spawn(petter(watchdog).unwrap());

    // Launch network task
    spawner.spawn(net_task(runner).unwrap());

    stack.wait_config_up().await;

    info!("Network initialized");

    // Initialize logger socket
    let socket = TcpSocket::new(
        stack,
        LOG_TCP_RX_BUF.init([0; _]),
        LOG_TCP_TX_BUF.init([0; _]),
    );

    // Initialize logger
    let runner = TcpEncoder::new(socket, LOGGER_PORT).await;

    // launch logger task
    spawner.spawn(logger_task(runner).unwrap());

    // Initialize updater socket
    let socket = TcpSocket::new(
        stack,
        UPD_TCP_RX_BUF.init([0; _]),
        UPD_TCP_TX_BUF.init([0; _]),
    );

    // Initialize firmware manager
    let flash = Flash::new(p.FLASH, Irqs);
    let storage = FIRMWARE_STORAGE.init(FirmwareManagerStorage::new(flash).await);
    let runner = FirmwareManager::new(storage, socket, FIRMWARE_UPDATE_PORT).await;

    // launch firmware manager task
    spawner.spawn(firmware_manager_task(runner).unwrap());

    // Initizlize Nats socket
    let socket = TcpSocket::new(
        stack,
        NATS_TCP_RX_BUF.init([0; _]),
        NATS_TCP_TX_BUF.init([0; _]),
    );

    // resolve nats addr
    let socket_addr = loop {
        match resolve_addr(&stack, NATS_ADDR, NATS_PORT).await {
            Ok(addr) => break addr,
            Err(e) => {
                warn!("could not resolve nats addr: {:?}, retrying...", e);
                Timer::after_secs(2).await;
            }
        }
    };

    // nats connection
    let (mut client, runner) =
        embassy_nats::new_with_user_pwd(NATS_USER, NATS_PWD, socket_addr, socket, &NATS_STORAGE);

    // launch nats task
    spawner.spawn(nats_task(runner).unwrap());

    let step = PulsePin::new(p.PA0);
    let dir = Output::new(p.PE7, Level::Low, Speed::Medium);
    let enable = Output::new(p.PE8, Level::Low, Speed::Medium);

    let step_interface = StepInterface::new(p.TIM2, step);
    // In the stm32 interconnection matrix TIM2 is ITR1 to TIM23
    let step_counter = StepCounter::new(p.TIM23, TriggerSource::ITR1);
    let mut stepper = Stepper::new(step_interface, step_counter, dir, enable, STEPPS_PER_REV);

    // LEDs on PE0..=PE4
    let mut led = Output::new(p.PE2, Level::Low, Speed::Low);

    #[derive(serde::Deserialize)]
    struct TestTarget {
        v: f64,
    }

    client
        .subscribe(alloc::string::String::from("trex.testing.target"), &CH)
        .await;
    loop {
        led.toggle();
        let nats_msg = client.receive().await;
        match minicbor_serde::from_slice::<TestTarget>(&nats_msg.data) {
            Ok(cmd) => {
                defmt::info!("stepps before move: {}", stepper.get_steps());
                stepper.set_speed(cmd.v);
                Timer::after(Duration::from_secs(1)).await;
                stepper.stop();
                defmt::info!("stepps after move: {}", stepper.get_steps());
            }
            Err(e) => defmt::warn!("could not decode cmd: {}", defmt::Debug2Format(&e)),
        }
    }

    //core::future::pending::<()>().await;
}
