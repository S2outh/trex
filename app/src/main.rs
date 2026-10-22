#![no_std]
#![no_main]

use panic_halt as _;

use mcu_crypto_asm as _;

use defmt::{info, warn};
use embassy_executor::Spawner;
use embassy_net::Stack;
use embassy_net::dns::DnsQueryType;
use embassy_net::tcp::{TcpListener, TcpSocket};
use embassy_net::wire::{IpAddr, IpCidr, Ipv4Addr, SocketAddr};
use embassy_net::{StackStorage, StaticPool};
use embassy_stm32::eth::{self, Ethernet, GenericPhy, PacketQueue, Sma};
use embassy_stm32::flash::{self, Flash};
use embassy_stm32::gpio::{Output, Speed};
use embassy_stm32::peripherals;
use embassy_stm32::rng::Rng;
use embassy_stm32::timer::low_level::TriggerSource;
use embassy_stm32::usart::{self, Uart};
use embassy_stm32::wdg::IndependentWatchdog;
use embassy_stm32::{Config, gpio::Level};
use embassy_stm32::{bind_interrupts, dma, rcc, rng};
use embassy_time::Timer;
use static_cell::{ConstStaticCell, StaticCell};

use embedded_nats::UserPwdAuthenticator;

use crate::control_loop::ControlLoop;
use crate::control_loop::axis::controller::PidParams;
use crate::control_loop::axis::{Axis, AxisConfig};
use crate::drivers::gnss::Gnss;
use crate::drivers::stepper::step_counter::StepCounter;
use crate::drivers::stepper::step_interface::StepInterface;
use crate::drivers::stepper::{Stepper, step_interface::PulsePin};

use crate::firmware_manager::{FirmwareManager, FirmwareManagerStorage};
use crate::logger::TcpLogger;
use crate::tm_loop::{TMChannel, TMLoop};

mod control_loop;
mod drivers;
mod firmware_manager;
mod logger;
mod tm_loop;

// General setup stuff
const WATCHDOG_TIMEOUT_US: u32 = 5_000_000;
const WATCHDOG_PETTING_INTERVAL_US: u32 = WATCHDOG_TIMEOUT_US / 2;

const IP_CIDR: IpCidr = IpCidr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 0, 10)), 24);
const GATEWAY: Ipv4Addr = Ipv4Addr::new(192, 168, 0, 1);
const DNS_SERVER: IpAddr = IpAddr::V4(Ipv4Addr::new(192, 168, 0, 1));

const FIRMWARE_UPDATE_PORT: u16 = 3000;

const LOGGER_PORT: u16 = 3001;

// Storage setup
static FIRMWARE_STORAGE: StaticCell<FirmwareManagerStorage> = StaticCell::new();

// Ethernet
// queues for raw packets before and after processing
static PACKET_QUEUE: ConstStaticCell<PacketQueue<4, 4>> = ConstStaticCell::new(PacketQueue::new());
// The ethernet DMA needs 8-byte aligned buffers, sized in multiples of 8.
static POOL: StaticPool<1520, 16, 8> = StaticPool::new();
// Storage holding the stack information at runtime
static STACK: StaticCell<StackStorage> = StaticCell::new();

// Ethernet device storage
type EthDevice =
    Ethernet<'static, peripherals::ETH, GenericPhy<Sma<'static, peripherals::ETH_SMA>>>;
static DEVICE: StaticCell<EthDevice> = StaticCell::new();

// buffers for tcp data before and after processing
static NATS_TCP_RX_BUF: ConstStaticCell<[u8; 256]> = ConstStaticCell::new([0; _]);
static NATS_TCP_TX_BUF: ConstStaticCell<[u8; 256]> = ConstStaticCell::new([0; _]);

static UPD_TCP_RX_BUF: ConstStaticCell<[u8; 256]> = ConstStaticCell::new([0; _]);
static UPD_TCP_TX_BUF: ConstStaticCell<[u8; 256]> = ConstStaticCell::new([0; _]);

static LOG_TCP_RX_BUF: ConstStaticCell<[u8; 256]> = ConstStaticCell::new([0; _]);
static LOG_TCP_TX_BUF: ConstStaticCell<[u8; 256]> = ConstStaticCell::new([0; _]);

// NATS
const NATS_MAX_TOPIC_SIZE: usize = 64;
const NATS_MAX_MESSAGE_SIZE: usize = 256;
type NatsCollections = embedded_nats::Heapless<NATS_MAX_TOPIC_SIZE, NATS_MAX_MESSAGE_SIZE>;
static NATS_STORAGE: embedded_nats::Storage<NatsCollections> = embedded_nats::Storage::new();
const NATS_ADDR: &str = "nats.lan";
const NATS_PORT: u16 = 4222;
const NATS_USER: &str = "nats";
const NATS_PWD: &str = "south";
const NATS_NUM_SUBS: usize = 2;

static TM_CHANNEL: TMChannel = TMChannel::new();

// Devices
// GPS
static GNSS_UART_RX_BUF: ConstStaticCell<[u8; 256]> = ConstStaticCell::new([0; _]);

// Steppers
const AZ_STEPPS_PER_REV: u32 = 12_000;
const AZ_MAX_SPEED: f64 = 1.5; // rad/s
const AZ_ACCELERATION: f64 = 1.2; // rad/s^2

const EL_STEPPS_PER_REV: u32 = 24_000;
const EL_MAX_SPEED: f64 = 2.0; // rad/s
const EL_ACCELERATION: f64 = 1.5; // rad/s^2

bind_interrupts!(struct Irqs {
    ETH => eth::InterruptHandler<peripherals::ETH>;
    FLASH => flash::InterruptHandler;
    RNG => rng::InterruptHandler<peripherals::RNG>;
    USART3 => usart::InterruptHandler<peripherals::USART3>;
    DMA1_STREAM0 => dma::InterruptHandler<peripherals::DMA1_CH0>;
    DMA1_STREAM1 => dma::InterruptHandler<peripherals::DMA1_CH1>;
});

fn get_rcc_config() -> rcc::Config {
    let mut rcc_config = rcc::Config::default();
    rcc_config.hsi = Some(rcc::HSIPrescaler::Div1); // 64 MHz
    rcc_config.hsi48 = Some(Default::default()); // needed for RNG

    rcc_config.pll1 = Some(rcc::Pll {
        source: rcc::PllSource::Hsi,
        prediv: rcc::PllPreDiv::Div4,  // 16 MHz
        mul: rcc::PllMul::Mul30,       // 480 MHz
        divp: Some(rcc::PllDiv::Div2), // 240 MHz
        divq: Some(rcc::PllDiv::Div8), // 60 MHz
        divr: Some(rcc::PllDiv::Div8), // 60 MHz
    });
    rcc_config.sys = rcc::Sysclk::Pll1P; // cpu runns with 240 MHz

    rcc_config.voltage_scale = rcc::VoltageScale::Scale2; // voltage scale for max 300 MHz Pll out

    rcc_config.ahb_pre = rcc::AHBPrescaler::Div2; // AHB runns at 120 MHz (src: sysclk)
    rcc_config.apb1_pre = rcc::APBPrescaler::Div2; // APB 1-4 all run with 60 MHz (src: ahb)
    rcc_config.apb2_pre = rcc::APBPrescaler::Div2;
    rcc_config.apb3_pre = rcc::APBPrescaler::Div2;
    rcc_config.apb4_pre = rcc::APBPrescaler::Div2;

    rcc_config
}

/// Watchdog petting task
#[embassy_executor::task]
async fn petter(mut watchdog: IndependentWatchdog<'static, peripherals::IWDG1>) {
    loop {
        watchdog.pet();
        Timer::after_micros(WATCHDOG_PETTING_INTERVAL_US.into()).await;
    }
}

macro_rules! runner_task {
    ($name: ident, $type: ty) => {
        #[embassy_executor::task]
        pub async fn $name(mut runner: $type) -> ! {
            runner.run().await
        }
    };
}

runner_task!(ctrl_task, ControlLoop<'static>);
runner_task!(gnss_task, Gnss<'static>);
runner_task!(tm_task, TMLoop<'static>);
runner_task!(net_task, embassy_net::Runner<'static>);
runner_task!(logger_task, TcpLogger<'static>);
runner_task!(firmware_manager_task, FirmwareManager<'static>);
runner_task!(
    nats_task,
    embedded_nats::Runner<'static, NatsCollections, UserPwdAuthenticator, NATS_NUM_SUBS>
);

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

    info!("Launching");

    // create independent watchdog,
    let mut watchdog = IndependentWatchdog::new(p.IWDG1, WATCHDOG_TIMEOUT_US);
    watchdog.unleash();

    // Launch watchdog task
    spawner.spawn(petter(watchdog).unwrap());

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
    rng.fill_bytes(&mut seed).await.unwrap();
    let seed = u64::from_le_bytes(seed);

    let mut mac_addr = [0; 6];
    rng.fill_bytes(&mut mac_addr).await.unwrap();
    // make sure mac addr is unicast
    mac_addr[0] &= !1;

    // Ethernet device setup
    let device = Ethernet::new(
        PACKET_QUEUE.take(),
        eth_int,
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
        Irqs,
    )
    .unwrap();

    // Initialize network stack
    info!("Initializing network task");
    let (stack, runner) = embassy_net::Stack::new(STACK.init(StackStorage::new()), &POOL, seed);

    let iface = stack.add_iface_borrowed(DEVICE.init(device)).unwrap();

    iface.add_ip_addr(IP_CIDR).unwrap();
    stack
        .routes()
        .add_default_ipv4_route(GATEWAY, iface.handle())
        .unwrap();
    stack.set_dns_servers(&[DNS_SERVER]);

    // Launch network task
    spawner.spawn(net_task(runner).unwrap());

    iface.wait_link_up().await;

    info!("Network initialized");

    // Initialize logger socket
    let socket = TcpSocket::new(stack, LOG_TCP_RX_BUF.take(), LOG_TCP_TX_BUF.take()).unwrap();

    let mut listener = TcpListener::new(stack).unwrap();
    listener.listen(LOGGER_PORT).unwrap();

    // Initialize logger
    let runner = TcpLogger::new(listener, socket);

    // launch logger task
    spawner.spawn(logger_task(runner).unwrap());

    // Initialize updater socket
    let socket = TcpSocket::new(stack, UPD_TCP_RX_BUF.take(), UPD_TCP_TX_BUF.take()).unwrap();

    let mut listener = TcpListener::new(stack).unwrap();
    listener.listen(FIRMWARE_UPDATE_PORT).unwrap();

    // Initialize firmware manager
    let config = flash::Config::default();
    let flash = Flash::new(p.FLASH, Irqs, config);
    let storage = FIRMWARE_STORAGE.init(FirmwareManagerStorage::new(flash));
    let runner = FirmwareManager::new(storage, listener, socket).await;

    // launch firmware manager task
    spawner.spawn(firmware_manager_task(runner).unwrap());

    // Initizlize Nats socket
    let socket = TcpSocket::new(stack, NATS_TCP_RX_BUF.take(), NATS_TCP_TX_BUF.take()).unwrap();

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
    let (client, runner) =
        embedded_nats::new_with_user_pwd(NATS_USER, NATS_PWD, socket_addr, socket, &NATS_STORAGE)
            .unwrap();

    // launch nats task
    spawner.spawn(nats_task(runner).unwrap());

    // launch tm loop
    let tm_loop = TMLoop::new(client.clone(), &TM_CHANNEL);
    spawner.spawn(tm_task(tm_loop).unwrap());

    // launch gnss loop
    // PPS on PD10
    let mut config = usart::Config::default();
    config.swap_rx_tx = true;
    config.baudrate = 9600;
    let uart = Uart::new(p.USART3, p.PD8, p.PD9, p.DMA1_CH0, p.DMA1_CH1, Irqs, config).unwrap();
    let (_writer, reader) = uart.split();
    let gnss = Gnss::new(reader.into_ring_buffered(GNSS_UART_RX_BUF.take()));
    spawner.spawn(gnss_task(gnss).unwrap());

    // Azimut stepper setup
    let step = PulsePin::new(p.PB0);
    let dir = Output::new(p.PE9, Level::Low, Speed::Medium);
    let enable = Output::new(p.PE10, Level::Low, Speed::Medium);

    let step_interface = StepInterface::new(p.TIM3, step);
    // In the stm32 interconnection matrix TIM3 is ITR2 to TIM24
    let step_counter = StepCounter::new(p.TIM24, TriggerSource::Itr2);
    let azimut_stepper = Stepper::new(step_interface, step_counter, dir, enable, AZ_STEPPS_PER_REV);

    // Elevation stepper setup
    let step = PulsePin::new(p.PA0);
    let dir = Output::new(p.PE7, Level::Low, Speed::Medium);
    let enable = Output::new(p.PE8, Level::Low, Speed::Medium);

    let step_interface = StepInterface::new(p.TIM2, step);
    // In the stm32 interconnection matrix TIM2 is ITR1 to TIM23
    let step_counter = StepCounter::new(p.TIM23, TriggerSource::Itr1);
    let elevation_stepper =
        Stepper::new(step_interface, step_counter, dir, enable, EL_STEPPS_PER_REV);

    // LEDs on PE0..=PE4
    // let mut blue_led = Output::new(p.PE2, Level::Low, Speed::Low);
    let mut _red_led = Output::new(p.PE0, Level::High, Speed::Low);

    // setup control loop
    let azimut = Axis::new(
        azimut_stepper,
        AxisConfig {
            max_speed: AZ_MAX_SPEED,
            acceleration: AZ_ACCELERATION,
            lower_bound: -core::f64::consts::PI,
            upper_bound: core::f64::consts::PI,
            v_pid_params: PidParams {
                kp: 4.,
                ki: Some((0.2, 5.)),
                kd: Some(3.0),
                deadband: 0.06,
            },
            a_pid_params: PidParams {
                kp: 0.7,
                ki: Some((0.2, 5.)),
                kd: None,
                deadband: 0.02,
            },
        },
    );
    let elevation = Axis::new(
        elevation_stepper,
        AxisConfig {
            max_speed: EL_MAX_SPEED,
            acceleration: EL_ACCELERATION,
            lower_bound: -core::f64::consts::PI / 2.,
            upper_bound: core::f64::consts::PI / 2.,
            v_pid_params: PidParams {
                kp: 4.,
                ki: Some((0.2, 5.)),
                kd: Some(3.0),
                deadband: 0.06,
            },
            a_pid_params: PidParams {
                kp: 0.7,
                ki: Some((0.2, 5.)),
                kd: None,
                deadband: 0.02,
            },
        },
    );

    // launch control loop
    let control_loop = ControlLoop::new(client, &TM_CHANNEL, azimut, elevation);
    spawner.spawn(ctrl_task(control_loop).unwrap());

    core::future::pending::<()>().await;
}
