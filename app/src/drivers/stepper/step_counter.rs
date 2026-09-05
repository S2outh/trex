use embassy_stm32::{
    Peri, timer::{
        Channel, GeneralInstance4Channel, low_level::{CountingMode, SlaveMode, Timer, TriggerSource},
    },
};

pub(super) struct StepCounter<'d, T: GeneralInstance4Channel> {
    inner: Timer<'d, T>,
}

impl<'d, T: GeneralInstance4Channel> StepCounter<'d, T> {
    pub fn new(tim: Peri<'d, T>, trigger: TriggerSource) -> Self {
        let inner = Timer::new(tim);

        // Counter (slave timer) configuration
        inner.set_counting_mode(CountingMode::EdgeAlignedUp);
        inner.set_slave_mode(SlaveMode::EXT_CLOCK_MODE);
        inner.set_trigger_source(trigger);

        // Apply and start
        inner.generate_update_event();
        inner.start();

        Self {
            inner,
        }
    }
    pub fn get(&mut self) -> u32 {
        self.inner.get_capture_value(Channel::Ch1).into()
    }
}
