use core::{marker::PhantomData, sync::atomic::Ordering};

use embassy_stm32::{
    Peri,
    peripherals,
    interrupt::typelevel::{Interrupt, Binding, Handler},
    pac::timer::TimCore,
    timer::{
        GeneralInstance4Channel,
        low_level::{CountingMode, SlaveMode, Timer, TriggerSource},
    },
};
use portable_atomic::AtomicU16;

use super::Dir;

// The interrupt handler for overflow counting (and handeling)
pub struct InterruptHandler<T: CounterInstance> {
    _phantom: PhantomData<T>,
}

impl<T: CounterInstance> Handler<T::UpdateInterrupt> for InterruptHandler<T> {
    unsafe fn on_interrupt() {
        // SAFETY: T::regs() is this timers block, TimCore is the register subset
        // common to every type of timer, so this timer is guaranteed to have it
        let regs = unsafe { TimCore::from_ptr(T::regs()) };

        // if this timers update interrupt flag is active:
        if regs.sr().read().uif() {
            // clear the interrupt flag
            regs.sr().modify(|r| {
                r.set_uif(false);
            });
            T::state().overflows.add(1, Ordering::Relaxed);
        }
    }
}

// The global (static) state of each timer peripheral acting as counter
struct CounterState {
    overflows: AtomicU16,
}

impl CounterState {
    const fn new() -> Self {
        let overflows = AtomicU16::new(0);
        Self { overflows }
    }
}

mod sealed {
    pub trait Sealed {}
}

// Trait for each timer instance that can act as counter
pub trait CounterInstance: GeneralInstance4Channel + sealed::Sealed {
    #[allow(private_interfaces)]
    fn state() -> &'static CounterState;
}

macro_rules! impl_counter_instance {
    ($inst: ident) => {
        impl sealed::Sealed for peripherals::$inst {}
        #[allow(private_interfaces)]
        impl CounterInstance for peripherals::$inst {
            fn state() -> &'static CounterState {
                static STATE: CounterState = CounterState::new();
                &STATE
            }
        }
    };
}

// Only implementing for the h723 timers, not device agnostic
impl_counter_instance!(TIM1);
impl_counter_instance!(TIM2);
impl_counter_instance!(TIM3);
impl_counter_instance!(TIM4);
impl_counter_instance!(TIM5);

impl_counter_instance!(TIM8);

impl_counter_instance!(TIM12);
impl_counter_instance!(TIM13);
impl_counter_instance!(TIM14);
impl_counter_instance!(TIM15);
impl_counter_instance!(TIM16);
impl_counter_instance!(TIM17);

impl_counter_instance!(TIM23);
impl_counter_instance!(TIM24);

// actual public facing step counter interface
pub struct StepCounter<'d, T: CounterInstance> {
    inner: Timer<'d, T>,
    dir: Dir,
    pos: i32,
    last_cnt: i32,
}

impl<'d, T: CounterInstance> StepCounter<'d, T> {
    pub fn new(
        tim: Peri<'d, T>,
        trigger: TriggerSource,
        _irq: impl Binding<T::UpdateInterrupt, InterruptHandler<T>>,
    ) -> Self {
        let inner = Timer::new(tim);

        // Counter (slave timer) configuration
        inner.set_counting_mode(CountingMode::EdgeAlignedUp);
        inner.set_slave_mode(SlaveMode::EXT_CLOCK_MODE);
        inner.set_trigger_source(trigger);

        // Disable prescaler, and set timer rollover value to MAX for free running
        inner.regs_gp16().psc().write_value(0);
        inner.regs_gp16().arr().write(|r| r.set_arr(u16::MAX));

        // Enable interrupt
        inner.enable_update_interrupt(true); // in timer
        T::UpdateInterrupt::unpend(); // remove pending bit
        unsafe { T::UpdateInterrupt::enable(); } // in nvic

        // Apply and start
        inner.generate_update_event();
        inner.start();

        Self {
            inner,
            dir: Dir::Cw,
            pos: 0,
            last_cnt: 0,
        }
    }
    // read the cnt register and overflow counter and update the position
    fn update_pos(&mut self) {
        // wrapping these in a CS in order to mitigate potential issues
        // if overflow happens between the swap and the read
        // TODO when overflow happens and dir is switched between two updates this still breaks
        let (overflows, cnt) = critical_section::with(|_| {
            (
                T::state().overflows.swap(0, Ordering::SeqCst) as i32,
                self.inner.regs_gp16().cnt().read().cnt() as i32,
            )
        });
        let rel_cnt = cnt + overflows * (u16::MAX as i32 + 1) - self.last_cnt;
        self.last_cnt = cnt;
        self.pos += rel_cnt * self.dir.int();
    }
    pub fn get_pos(&mut self) -> i32 {
        self.update_pos();
        // actual pos is halved, as master counter is toggle
        self.pos / 2
    }
    // since the counter internally does not track direction,
    pub fn set_dir(&mut self, dir: Dir) {
        if self.dir == dir {
            return;
        }
        self.update_pos();
        self.dir = dir;
    }
}
