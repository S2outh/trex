use embassy_stm32::{
    Peri,
    timer::{
        GeneralInstance32bit4Channel,
        low_level::{CountingMode, SlaveMode, Timer, TriggerSource},
    },
};

pub struct StepCounter<'d, T: GeneralInstance32bit4Channel> {
    inner: Timer<'d, T>,
}

impl<'d, T: GeneralInstance32bit4Channel> StepCounter<'d, T> {
    pub fn new(tim: Peri<'d, T>, trigger: TriggerSource) -> Self {
        let inner = Timer::new(tim);

        // Counter (slave timer) configuration
        inner.set_counting_mode(CountingMode::EdgeAlignedUp);
        inner.set_slave_mode(SlaveMode::EXT_CLOCK_MODE);
        inner.set_trigger_source(trigger);

        // Disable prescaler, and enable free running
        inner.regs_gp32().psc().write_value(0);
        inner.regs_gp32().arr().write_value(u32::MAX);

        // Apply and start
        inner.generate_update_event();
        inner.start();

        Self { inner }
    }
    pub fn get(&mut self) -> u32 {
        self.inner.regs_gp32().cnt().read()
    }
}
