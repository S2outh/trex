use std::path::Path;

use anyhow::{Context, Result, anyhow};
use defmt_decoder::{
    DecodeError, Frame, Locations, Table,
    log::{
        DefmtLoggerType,
        format::{Formatter, FormatterConfig, HostFormatter},
    },
};
use tokio::{io::AsyncReadExt, net::TcpStream};

use crate::NetConf;

pub async fn run(data: &[u8], net_conf: &NetConf) -> Result<()> {
    // defmt logger setup
    let table = Table::parse(&data)?.ok_or_else(|| anyhow!(".defmt data not found"))?;
    let locs = table.get_locations(&data)?;

    // check if the locations info contains all the indicies
    let locs = if table.indices().all(|idx| locs.contains_key(&(idx as u64))) {
        Some(locs)
    } else {
        None
    };

    // logger config
    let logger_type = DefmtLoggerType::Stdout;
    let formatter_config = FormatterConfig::default();
    let host_formatter_config = FormatterConfig::default();

    let formatter = Formatter::new(formatter_config);
    let host_formatter = HostFormatter::new(host_formatter_config);

    defmt_decoder::log::init_logger(formatter, host_formatter, logger_type, |_| true);

    let mut stream_decoder = table.new_stream_decoder();

    let current_dir = std::env::current_dir()?;

    // open tcp stream
    let mut tcp = TcpStream::connect((net_conf.host.clone(), net_conf.logger_port))
        .await
        .context("could not connect to target")?;

    const READ_BUFFER_SIZE: usize = 1024;
    let mut buf = [0; READ_BUFFER_SIZE];

    loop {
        // read from tcpstream and push it to the decoder
        let n = tcp.read(&mut buf).await?;

        // if 0 bytes where read, we reached EOF
        if n == 0 {
            break Ok(());
        }

        stream_decoder.received(&buf[..n]);

        // decode the received data
        loop {
            match stream_decoder.decode() {
                Ok(frame) => forward_to_logger(&frame, location_info(&locs, &frame, &current_dir)),
                Err(DecodeError::UnexpectedEof) => break,
                Err(DecodeError::Malformed) => match table.encoding().can_recover() {
                    // if recovery is impossible, abort
                    false => return Err(DecodeError::Malformed.into()),
                    // if recovery is possible, skip the current frame and continue with new data
                    true => continue,
                },
            }
        }
    }
}

type LocationInfo = (Option<String>, Option<u32>, Option<String>);

fn forward_to_logger(frame: &Frame, location_info: LocationInfo) {
    let (file, line, mod_path) = location_info;
    defmt_decoder::log::log_defmt(frame, file.as_deref(), line, mod_path.as_deref());
}

fn location_info(locs: &Option<Locations>, frame: &Frame, current_dir: &Path) -> LocationInfo {
    let (mut file, mut line, mut mod_path) = (None, None, None);

    let loc = locs.as_ref().map(|locs| locs.get(&frame.index()));

    if let Some(Some(loc)) = loc {
        // try to get the relative path, else the full one
        let path = loc.file.strip_prefix(current_dir).unwrap_or(&loc.file);

        file = Some(path.display().to_string());
        line = Some(loc.line as u32);
        mod_path = Some(loc.module.clone());
    }

    (file, line, mod_path)
}
