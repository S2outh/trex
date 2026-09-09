#![no_std]
#![no_main]

use core::cell::RefCell;

use defmt_rtt as _;

use cortex_m_rt::{entry, exception};
use embassy_boot_stm32::{BootLoader, BootLoaderConfig};
use embassy_stm32::flash::{BANK1_REGION, Flash};
use embassy_stm32::wdg::IndependentWatchdog;
use embassy_sync::blocking_mutex::Mutex;

const FLASH_COPY_BUFFER_SIZE: usize = 2048;
// This timeout is only relevant until the main firmware boots.
const WATCHDOG_TIMEOUT_US: u32 = 12_000_000;

#[entry]
fn main() -> ! {
    let p = embassy_stm32::init(Default::default());
    let layout = Flash::new_blocking(p.FLASH).into_blocking_regions();
    let flash = Mutex::new(RefCell::new(layout.bank1_region));

    defmt::info!("[BOOT] Loading firmware...");

    let config = BootLoaderConfig::from_linkerfile_blocking(&flash, &flash, &flash);
    let active_offset = config.active.offset();
    let bl = BootLoader::prepare::<_, _, _, FLASH_COPY_BUFFER_SIZE>(config);

    defmt::info!("[BOOT] Sucessfull");

    // The watchdog will only be petted if the active partition works correctly
    let mut watchdog = IndependentWatchdog::new(p.IWDG1, WATCHDOG_TIMEOUT_US);
    watchdog.unleash();

    unsafe { bl.load(BANK1_REGION.base() + active_offset) }
}

#[unsafe(no_mangle)]
#[cfg_attr(target_os = "none", unsafe(link_section = ".HardFault.user"))]
unsafe extern "C" fn HardFault() {
    cortex_m::peripheral::SCB::sys_reset();
}

#[exception]
unsafe fn DefaultHandler(_: i16) -> ! {
    cortex_m::peripheral::SCB::sys_reset();
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    cortex_m::asm::udf();
}
