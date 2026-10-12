//! FIFO writer used by native tests that need to observe when a reader has
//! consumed each short write. The XSH FIFO API does not expose unread-byte
//! counts, so this host-boundary helper waits for the kernel queue to drain.

use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

const HELPER_LIMIT: Duration = Duration::from_secs(14);
const POLL_INTERVAL: Duration = Duration::from_millis(1);
const OPEN_RETRY_INTERVAL: Duration = Duration::from_millis(10);

pub(crate) fn write_acked_chunks(mut args: impl Iterator<Item = OsString>) -> Result<(), String> {
    let fifo = required_path(&mut args, "FIFO path")?;
    let input = required_path(&mut args, "input path")?;
    let width = required_number(&mut args, "chunk width")?;
    let pause_ms = required_duration_ms(&mut args, "pause_ms")?;
    if args.next().is_some() {
        return Err("unexpected argument".to_string());
    }
    if width == 0 {
        return Err("chunk width must be positive".to_string());
    }

    let deadline = Instant::now() + HELPER_LIMIT;
    let data = std::fs::read(&input).map_err(|error| format!("read {}: {error}", input.display()))?;
    let mut writer = open_writer(&fifo, deadline)?;
    for chunk in data.chunks(width) {
        writer
            .write_all(chunk)
            .map_err(|error| format!("write {}: {error}", fifo.display()))?;
        wait_until_drained(&writer, deadline)?;
        sleep_until_next_chunk(pause_ms, deadline)?;
    }
    Ok(())
}

fn open_writer(path: &PathBuf, deadline: Instant) -> Result<File, String> {
    loop {
        if Instant::now() >= deadline {
            return Err(format!("timed out opening FIFO {} for writing", path.display()));
        }
        match OpenOptions::new()
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(path)
        {
            Ok(file) => {
                // SAFETY: `file` owns the FIFO descriptor passed to fcntl.
                let flags = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETFL) };
                if flags < 0 {
                    return Err(format!("read FIFO writer flags: {}", io::Error::last_os_error()));
                }
                // SAFETY: `file` still owns this descriptor and the flags came from F_GETFL.
                if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETFL, flags & !libc::O_NONBLOCK) }
                    < 0
                {
                    return Err(format!("set FIFO writer flags: {}", io::Error::last_os_error()));
                }
                return Ok(file);
            }
            Err(error) if error.raw_os_error() == Some(libc::ENXIO) => {
                thread::sleep(OPEN_RETRY_INTERVAL);
            }
            Err(error) => return Err(format!("open FIFO {}: {error}", path.display())),
        }
    }
}

fn wait_until_drained(writer: &File, deadline: Instant) -> Result<(), String> {
    loop {
        if Instant::now() >= deadline {
            return Err("timed out waiting for the FIFO reader to consume a chunk".to_string());
        }
        let mut unread = 0;
        // SAFETY: `unread` is a live writable integer for ioctl's FIONREAD result.
        if unsafe { libc::ioctl(writer.as_raw_fd(), libc::FIONREAD, &mut unread) } < 0 {
            return Err(format!("query unread FIFO bytes: {}", io::Error::last_os_error()));
        }
        if unread == 0 {
            return Ok(());
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn sleep_until_next_chunk(pause: Duration, deadline: Instant) -> Result<(), String> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if pause >= remaining {
        thread::sleep(remaining);
        return Err("timed out waiting between FIFO chunks".to_string());
    }
    thread::sleep(pause);
    Ok(())
}

fn required_path(args: &mut impl Iterator<Item = OsString>, name: &str) -> Result<PathBuf, String> {
    args.next()
        .map(PathBuf::from)
        .ok_or_else(|| format!("{name} is required"))
}

fn required_number(args: &mut impl Iterator<Item = OsString>, name: &str) -> Result<usize, String> {
    args.next()
        .and_then(|arg| arg.into_string().ok())
        .ok_or_else(|| format!("{name} is required"))?
        .parse::<usize>()
        .map_err(|error| format!("invalid {name}: {error}"))
}

fn required_duration_ms(
    args: &mut impl Iterator<Item = OsString>,
    name: &str,
) -> Result<Duration, String> {
    args.next()
        .and_then(|arg| arg.into_string().ok())
        .ok_or_else(|| format!("{name} is required"))?
        .parse::<u64>()
        .map(Duration::from_millis)
        .map_err(|error| format!("invalid {name}: {error}"))
}
