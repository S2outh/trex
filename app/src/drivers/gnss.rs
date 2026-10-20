use embassy_stm32::usart::{self, RingBufferedUartRx};
use nmea::ParseResult;

pub struct Gnss<'a> {
    reader: RingBufferedUartRx<'a>,
}

const DELIM: [u8; 2] = *b"\r\n";

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("Usart error {0}")]
    Usart(#[from] usart::Error),
    #[error("NMEA error")]
    Nmea,
}

impl<'a> Gnss<'a> {
    pub fn new(reader: RingBufferedUartRx<'a>) -> Self {
        Self { reader }
    }
    async fn read_line(&mut self, mut buf: &mut [u8]) -> Result<usize, Error> {
        let mut delim_pos = 0;
        let mut pos = 0;
        loop {
            let byte = buf.first_mut().unwrap();
            self.reader.read(core::array::from_mut(byte)).await?;
            pos += 1;
            if *byte == DELIM[delim_pos] {
                delim_pos += 1;
                if delim_pos >= DELIM.len() {
                    return Ok(pos)
                }
            }
            buf = &mut buf[1..];
        }
    }
    async fn next_msg(&mut self) -> Result<ParseResult, Error> {
        let mut buf = [0; nmea::SENTENCE_MAX_LEN + DELIM.len()];
        let len = self.read_line(&mut buf).await?;
        Ok(nmea::parse_bytes(&buf[..len]).map_err(|_| Error::Nmea)?)
    }
    pub async fn run(&mut self) -> ! {
        loop {
            if let Ok(msg) = self.next_msg().await {
                defmt::info!("{}", msg);
            }
        }
    }
}
