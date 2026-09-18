use core::{marker::PhantomData, sync::atomic::Ordering};

use embassy_stm32::{
    Peri,
    interrupt::typelevel::{Binding, Handler},
    pac::timer::TimCore,
    peripherals,
    timer::{
        GeneralInstance4Channel,
        low_level::{CountingMode, SlaveMode, Timer, TriggerSource},
    },
};
use portable_atomic::AtomicU16;

#[derive(PartialEq, Eq)]
pub enum Dir {
    Cw,
    Ccw,
}

impl Dir {
    fn int(&self) -> i32 {
        match self {
            Dir::Cw => 1,
            Dir::Ccw => -1,
        }
    }
}

pub struct InterruptHandler<T: CounterInstance> {
    _phantom: PhantomData<T>,
}

impl<T: CounterInstance + GeneralInstance4Channel> Handler<T::UpdateInterrupt>
    for InterruptHandler<T>
{
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

struct CounterState {
    overflows: AtomicU16,
}

impl CounterState {
    const fn new() -> Self {
        let overflows = AtomicU16::new(0);
        Self { overflows }
    }
}

pub trait CounterInstance {
    #[allow(private_interfaces)]
    fn state() -> &'static CounterState;
}

macro_rules! impl_counter_state {
    ($type: ty) => {
        impl CounterInstance for $type {
            #[allow(private_interfaces)]
            fn state() -> &'static CounterState {
                static STATE: CounterState = CounterState::new();
                &STATE
            }
        }
    };
}

impl_counter_state!(peripherals::TIM23);
impl_counter_state!(peripherals::TIM24);

pub struct StepCounter<'d, T: GeneralInstance4Channel> {
    inner: Timer<'d, T>,
    dir: Dir,
    pos: i32,
    last_cnt: i32,
}

impl<'d, T: GeneralInstance4Channel + CounterInstance> StepCounter<'d, T> {
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

        // set interrupt
        inner.enable_update_interrupt(true);

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
    // read the cnt register and update the position, resetting the cnt register
    fn update_pos(&mut self) {
        // wrapping these in a CS in order to mitigate potential issues
        // if overflow happens between the swap and the read
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
        self.pos
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
