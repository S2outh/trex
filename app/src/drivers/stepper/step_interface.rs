use core::marker::PhantomData;

use embassy_stm32::{
    Peri, gpio::Flex, time::Hertz, timer::{
        GeneralInstance4Channel, TimerChannel, TimerPin, low_level::{CountingMode, MasterMode, OutputCompareMode, RoundTo, Timer},
    },
};

pub struct PulsePin<'d, T, C> {
    pin: Flex<'d>,
    phantom: PhantomData<(T, C)>,
}

impl<'d, T: GeneralInstance4Channel, C: TimerChannel> PulsePin<'d, T, C> {
    pub fn new(pin: Peri<'d, impl TimerPin<T, C>>) -> Self {
        Self {
            pin: Flex::new(pin),
            phantom: PhantomData,
        }
    }
}

pub(super) struct StepInterface<'d, T: GeneralInstance4Channel, C> {
    inner: Timer<'d, T>,
    output: Flex<'d>,
    phantom: PhantomData<C>
}

impl<'d, T: GeneralInstance4Channel, C: TimerChannel> StepInterface<'d, T, C> {
    pub fn new(tim: Peri<'d, T>, output: PulsePin<'d, T, C>) -> Self {
        let inner = Timer::new(tim);
        let output = output.pin;

        // Initialize timer
        inner.set_counting_mode(CountingMode::EdgeAlignedUp);
        inner.enable_outputs();

        // Initialize timer output
        inner.set_output_compare_mode(C::CHANNEL, OutputCompareMode::Toggle);

        // Set master mode for counter
        inner.set_master_mode(MasterMode::UPDATE);

        // Enable preloading into shadow registers, only apply on update event
        inner.set_output_compare_preload(C::CHANNEL, true);
        inner.set_autoreload_preload(true);

        // Apply
        inner.generate_update_event();

        Self {
            inner,
            output,
            phantom: PhantomData,
        }
    }
    pub fn start(&mut self) {
        self.inner.start();
    }
    pub fn stop(&mut self) {
        self.inner.stop();
    }
    pub fn set_frequency(&mut self, freq: Hertz) {
        self.inner.set_frequency(freq, RoundTo::Slower);
    }
}
