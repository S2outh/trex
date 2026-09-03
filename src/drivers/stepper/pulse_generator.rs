use core::marker::PhantomData;

use embassy_stm32::{
    Peri, gpio::Flex, time::Hertz, timer::{
        GeneralInstance4Channel, TimerChannel, TimerPin, low_level::{CountingMode, OutputCompareMode, RoundTo, Timer},
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

pub struct PulseGenerator<'d, T: GeneralInstance4Channel, C> {
    inner: Timer<'d, T>,
    output: Flex<'d>,
    phantom: PhantomData<C>
}

impl<'d, T: GeneralInstance4Channel, C: TimerChannel> PulseGenerator<'d, T, C> {
    pub fn new(tim: Peri<'d, T>, output: PulsePin<'d, T, C>, init_freq: Hertz) -> Self {
        let inner = Timer::new(tim);
        let output = output.pin;

        // Initialize timer
        inner.set_counting_mode(CountingMode::EdgeAlignedUp);
        inner.set_frequency(init_freq, RoundTo::Slower);
        inner.enable_outputs();

        // Initialize timer output
        inner.set_output_compare_mode(C::CHANNEL, OutputCompareMode::Toggle);

        // Enable preloading into shadow registers, only apply on update event
        inner.set_output_compare_preload(C::CHANNEL, true);
        inner.set_autoreload_preload(true);

        // Apply and start
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
