use embassy_stm32::{
    Peri,
    timer::{
        GeneralInstance4Channel,
        low_level::{CountingMode, SlaveMode, Timer, TriggerSource},
    },
};

use super::Dir;

// actual public facing step counter interface
pub struct StepCounter<'d, T: GeneralInstance4Channel> {
    inner: Timer<'d, T>,
    dir: Dir,
    pos: i32,
    last_cnt: i32,
}

impl<'d, T: GeneralInstance4Channel> StepCounter<'d, T> {
    pub fn new(tim: Peri<'d, T>, trigger: TriggerSource) -> Self {
        let inner = Timer::new(tim);

        // Counter (slave timer) configuration
        inner.set_counting_mode(CountingMode::EdgeAlignedUp);
        inner.set_slave_mode(SlaveMode::ExtClockMode);
        inner.set_trigger_source(trigger);

        // Disable prescaler, and set timer rollover value to MAX for free running
        inner.regs_gp16().psc().write_value(0);
        inner.regs_gp16().arr().write(|r| r.set_arr(u16::MAX));

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
        let cnt = self.inner.regs_gp16().cnt().read().cnt() as i32;
        let mut rel_cnt = cnt - self.last_cnt;

        // overflow happened
        // (this dropps stepps if thhe counter rolls over twice between
        // two update_pos() calls, which won't happen in practice)
        if rel_cnt < 0 {
            rel_cnt += u16::MAX as i32 + 1;
        }

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
