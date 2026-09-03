#![no_std]
#![no_main]

use defmt::*;
use embassy_executor::Spawner;
use embassy_stm32::Config;
use embassy_stm32::rcc;
use {defmt_rtt as _, panic_probe as _};

mod drivers;

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

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let mut config = Config::default();
    config.rcc = get_rcc_config();
    let _p = embassy_stm32::init(config);

    info!("Launching");

    core::future::pending::<()>().await;
}
