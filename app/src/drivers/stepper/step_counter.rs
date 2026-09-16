use embassy_stm32::{
    Peri, timer::{
        GeneralInstance4Channel,
        low_level::{CountingMode, SlaveMode, Timer, TriggerSource},
    },
};

#[derive(PartialEq, Eq)]
pub enum Dir {
    Cw,
    Ccw,
}

impl Dir {
    fn mul(&self) -> i32 {
        match self {
            Dir::Cw => 1,
            Dir::Ccw => -1,
        }
    }
}

pub struct StepCounter<'d, T: GeneralInstance4Channel> {
    inner: Timer<'d, T>,
    dir: Dir,
    pos: i32,
}

impl<'d, T: GeneralInstance4Channel> StepCounter<'d, T> {
    pub fn new(tim: Peri<'d, T>, trigger: TriggerSource) -> Self {
        let inner = Timer::new(tim);

        // Counter (slave timer) configuration
        inner.set_counting_mode(CountingMode::EdgeAlignedUp);
        inner.set_slave_mode(SlaveMode::EXT_CLOCK_MODE);
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
        }
    }
    // read the cnt register and update the position, resetting the cnt register
    fn update_pos(&mut self) {
        let cnt = self.inner.regs_gp16().cnt().read().cnt();
        self.inner.reset();
        self.pos += cnt as i32 * self.dir.mul()
    }
    pub fn get_pos(&mut self) -> i32 {
        self.update_pos();
        self.pos
    }
    // since the counter internally does not track direction, 
    pub fn set_dir(&mut self, dir: Dir) {
        if self.dir == dir {
            return
        }
        self.update_pos();
        self.dir = dir;
    }
}
