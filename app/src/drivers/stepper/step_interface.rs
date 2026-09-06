use core::{marker::PhantomData};

use embassy_stm32::{
    Peri, gpio::{AfType, Flex, OutputType, Speed}, time::Hertz, timer::{
        GeneralInstance4Channel, TimerChannel, TimerPin, low_level::{CountingMode, MasterMode, OutputCompareMode, RoundTo, Timer},
    },
};

pub struct PulsePin<'d, T, C> {
    pin: Flex<'d>,
    phantom: PhantomData<(T, C)>,
}

impl<'d, T: GeneralInstance4Channel, C: TimerChannel> PulsePin<'d, T, C> {
    pub fn new(pin: Peri<'d, impl TimerPin<T, C>>) -> Self {
        let af = pin.af_num();
        let mut pin = Flex::new(pin);
        pin.set_low();
        pin.set_as_af_unchecked(af, AfType::output(OutputType::PushPull, Speed::VeryHigh));
        Self { pin, phantom: PhantomData }
    }
}

pub(super) struct StepInterface<'d, T: GeneralInstance4Channel, C> {
    inner: Timer<'d, T>,
    _output: Flex<'d>,
    phantom: PhantomData<C>
}

impl<'d, T: GeneralInstance4Channel, C: TimerChannel> StepInterface<'d, T, C> {
    pub fn new(tim: Peri<'d, T>, output: PulsePin<'d, T, C>) -> Self {
        let inner = Timer::new(tim);
        let _output = output.pin;

        // Initialize timer
        inner.set_counting_mode(CountingMode::EdgeAlignedUp);
        inner.enable_outputs();

        // Set master mode for counter
        inner.set_master_mode(MasterMode::UPDATE);

        // Initialize timer output
        inner.set_output_compare_mode(C::CHANNEL, OutputCompareMode::Toggle);

        // Enable preloading into shadow registers, only apply on update event
        inner.set_autoreload_preload(true);

        // Enable channel
        inner.enable_channel(C::CHANNEL, true);
        
        // Apply
        inner.generate_update_event();

        Self {
            inner,
            _output,
            phantom: PhantomData,
        }
    }
    pub fn start(&mut self) {
        self.inner.start();
    }
    pub fn stop(&mut self) {
        self.inner.stop();
    }
    pub fn set_frequency(&mut self, mut freq: Hertz) {
        freq.0 *= 2;
        self.inner.set_frequency(freq, RoundTo::Slower);
        self.inner.generate_update_event();
    }
}
