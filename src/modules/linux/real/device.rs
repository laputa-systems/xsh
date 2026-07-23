use super::common::{cstring_path, error_value, io_error, ok_unit, path_value};
use super::{LO_NAME_SIZE, LOOP_CLR_FD, LOOP_CTL_GET_FREE, LOOP_GET_STATUS64, LOOP_SET_FD};
use super::{LOOP_SET_STATUS64, LoopInfo64, SWAP_FLAG_PREFER, SWAP_FLAG_PRIO_SHIFT};
use super::{SWAP_HEADER_OFFSET, SWAP_HEADER_SIZE, SWAP_MAGIC, SWAP_UUID_OFFSET, UeventStream};
use crate::modules::linux::BLKGETSIZE64;
use crate::runtime::value::{LiveStream, PathValue, RuntimeError, StreamValue, Value};
use crate::source::Span;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub(crate) fn write_device(
    device: &Path,
    source: &Path,
    span: Span,
) -> Result<Value, RuntimeError> {
    let mut input = match File::open(source) {
        Ok(file) => file,
        Err(error) => return Ok(io_error("linux-write-device", error, span)),
    };
    let mut output = match OpenOptions::new().write(true).open(device) {
        Ok(file) => file,
        Err(error) => return Ok(io_error("linux-write-device", error, span)),
    };
    match io::copy(&mut input, &mut output) {
        Ok(_) => Ok(ok_unit()),
        Err(error) => Ok(io_error("linux-write-device", error, span)),
    }
}

pub(crate) fn read_device(
    device: &Path,
    dest: &Path,
    bytes: i64,
    span: Span,
) -> Result<Value, RuntimeError> {
    if bytes < 0 {
        return Ok(error_value(
            "linux-read-device",
            "bytes must be non-negative",
            span,
        ));
    }
    let mut input = match File::open(device) {
        Ok(file) => file,
        Err(error) => return Ok(io_error("linux-read-device", error, span)),
    };
    let mut output = match OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(dest)
    {
        Ok(file) => file,
        Err(error) => return Ok(io_error("linux-read-device", error, span)),
    };
    let mut limited = Read::by_ref(&mut input).take(bytes as u64);
    match io::copy(&mut limited, &mut output) {
        Ok(_) => Ok(ok_unit()),
        Err(error) => Ok(io_error("linux-read-device", error, span)),
    }
}

pub(crate) fn uevent_stream(span: Span) -> Result<Value, RuntimeError> {
    match UeventStream::open(span) {
        Ok(stream) => Ok(Value::ok(Value::stream(StreamValue::from_live(
            "linux.uevent_stream",
            stream,
        )))),
        Err(error) => Ok(Value::err(Value::Error(Box::new(error)))),
    }
}

pub(crate) fn loop_attach(
    file: &Path,
    device: Option<&Path>,
    span: Span,
) -> Result<Value, RuntimeError> {
    let device = match device {
        Some(device) => device.to_path_buf(),
        None => match free_loop_device() {
            Ok(device) => device,
            Err(error) => return Ok(io_error("linux-loop", error, span)),
        },
    };
    let backing = match File::open(file) {
        Ok(file) => file,
        Err(error) => return Ok(io_error("linux-loop", error, span)),
    };
    let loop_file = match OpenOptions::new().read(true).write(true).open(&device) {
        Ok(file) => file,
        Err(error) => return Ok(io_error("linux-loop", error, span)),
    };
    let rc = unsafe { libc::ioctl(loop_file.as_raw_fd(), LOOP_SET_FD as _, backing.as_raw_fd()) };
    if rc != 0 {
        return Ok(io_error("linux-loop", io::Error::last_os_error(), span));
    }
    let mut info = LoopInfo64::default();
    copy_file_name(&mut info.lo_file_name, file.as_os_str().as_bytes());
    let rc = unsafe { libc::ioctl(loop_file.as_raw_fd(), LOOP_SET_STATUS64 as _, &info) };
    if rc != 0 {
        let error = io::Error::last_os_error();
        unsafe {
            libc::ioctl(loop_file.as_raw_fd(), LOOP_CLR_FD as _);
        }
        return Ok(io_error("linux-loop", error, span));
    }
    Ok(Value::ok(Value::Path(path_value(&device, span)?)))
}

pub(crate) fn loop_detach(device: &Path, span: Span) -> Result<Value, RuntimeError> {
    let file = match OpenOptions::new().read(true).write(true).open(device) {
        Ok(file) => file,
        Err(error) => return Ok(io_error("linux-loop", error, span)),
    };
    let rc = unsafe { libc::ioctl(file.as_raw_fd(), LOOP_CLR_FD as _) };
    if rc == 0 {
        Ok(ok_unit())
    } else {
        Ok(io_error("linux-loop", io::Error::last_os_error(), span))
    }
}

pub(crate) fn loop_list(span: Span) -> Result<Value, RuntimeError> {
    let entries = match fs::read_dir("/dev") {
        Ok(entries) => entries,
        Err(error) => return Ok(io_error("linux-loop", error, span)),
    };
    let mut paths = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with("loop") || !name[4..].chars().all(|ch| ch.is_ascii_digit()) {
            continue;
        }
        paths.push(entry.path());
    }
    paths.sort_unstable();
    Ok(Value::ok(Value::stream(StreamValue::from_live(
        "linux.loop_list",
        LoopDeviceStream {
            paths: paths.into_iter(),
        },
    ))))
}

struct LoopDeviceStream {
    paths: std::vec::IntoIter<PathBuf>,
}

impl LiveStream for LoopDeviceStream {
    fn next(&mut self, span: Span) -> Result<Option<Value>, RuntimeError> {
        loop {
            let Some(path) = self.paths.next() else {
                return Ok(None);
            };
            match loop_device_record(&path, span)? {
                Some(record) => return Ok(Some(record)),
                None => continue,
            }
        }
    }
}

pub(crate) fn mkswap(device: &Path, span: Span) -> Result<Value, RuntimeError> {
    match write_swap_header(device, span) {
        Ok(()) => Ok(ok_unit()),
        Err(error) => Ok(Value::err(Value::Error(Box::new(error)))),
    }
}

pub(crate) fn swapon(device: &Path, priority: i64, span: Span) -> Result<Value, RuntimeError> {
    let device = cstring_path(device, "linux-swapon", span)?;
    let flags = if priority >= 0 {
        if priority > 32_767 {
            return Ok(error_value("linux-swapon", "priority is too large", span));
        }
        SWAP_FLAG_PREFER | ((priority as libc::c_int) << SWAP_FLAG_PRIO_SHIFT)
    } else {
        0
    };
    let rc = unsafe { libc::swapon(device.as_ptr(), flags) };
    if rc == 0 {
        Ok(ok_unit())
    } else {
        Ok(io_error("linux-swapon", io::Error::last_os_error(), span))
    }
}

pub(crate) fn swapoff(device: &Path, span: Span) -> Result<Value, RuntimeError> {
    let device = cstring_path(device, "linux-swapoff", span)?;
    let rc = unsafe { libc::swapoff(device.as_ptr()) };
    if rc == 0 {
        Ok(ok_unit())
    } else {
        Ok(io_error("linux-swapoff", io::Error::last_os_error(), span))
    }
}

fn free_loop_device() -> io::Result<PathBuf> {
    let control = File::open("/dev/loop-control")?;
    let index = unsafe { libc::ioctl(control.as_raw_fd(), LOOP_CTL_GET_FREE as _) };
    if index >= 0 {
        Ok(PathBuf::from(format!("/dev/loop{index}")))
    } else {
        Err(io::Error::last_os_error())
    }
}

fn loop_device_record(path: &Path, span: Span) -> Result<Option<Value>, RuntimeError> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if is_inaccessible_loop_device(&error) => return Ok(None),
        Err(error) => {
            return Err(RuntimeError::new("linux-loop", error.to_string()).with_span(span));
        }
    };
    let mut info = LoopInfo64::default();
    let rc = unsafe { libc::ioctl(file.as_raw_fd(), LOOP_GET_STATUS64 as _, &mut info) };
    if rc != 0 {
        let error = io::Error::last_os_error();
        return match error.raw_os_error() {
            Some(libc::ENXIO | libc::ENODEV) => Ok(None),
            _ if is_inaccessible_loop_device(&error) => Ok(None),
            _ => Err(RuntimeError::new("linux-loop", error.to_string()).with_span(span)),
        };
    }
    Ok(Some(Value::Record(crate::runtime::value::RecordMap::from(
        [
            (Arc::from("device"), Value::Path(path_value(path, span)?)),
            (
                Arc::from("file"),
                Value::Path(PathValue::new(c_string_from_bytes(&info.lo_file_name))?),
            ),
            (Arc::from("offset"), Value::Int(info.lo_offset as i64)),
            (Arc::from("size"), Value::Int(info.lo_sizelimit as i64)),
        ],
    ))))
}

fn is_inaccessible_loop_device(error: &io::Error) -> bool {
    matches!(error.raw_os_error(), Some(libc::EACCES | libc::EPERM))
}

fn copy_file_name(dest: &mut [u8; LO_NAME_SIZE], bytes: &[u8]) {
    let len = bytes.len().min(dest.len().saturating_sub(1));
    dest[..len].copy_from_slice(&bytes[..len]);
}

fn c_string_from_bytes(bytes: &[u8]) -> Vec<u8> {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    bytes[..end].to_vec()
}

fn write_swap_header(device: &Path, span: Span) -> Result<(), RuntimeError> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(device)
        .map_err(|error| RuntimeError::new("linux-mkswap", error.to_string()).with_span(span))?;
    let page_size = page_size(span)?;
    let size = device_size(&file, span)?;
    if size < (page_size as u64) * 10 {
        return Err(
            RuntimeError::new("linux-mkswap", "swap area needs to be at least 10 pages")
                .with_span(span),
        );
    }
    let last_page = size
        .checked_div(page_size as u64)
        .and_then(|pages| pages.checked_sub(1))
        .ok_or_else(|| {
            RuntimeError::new("linux-mkswap", "swap area is too small").with_span(span)
        })?;
    let mut header = vec![0_u8; SWAP_HEADER_SIZE];
    header[0..4].copy_from_slice(&1_u32.to_ne_bytes());
    header[4..8].copy_from_slice(&(last_page as u32).to_ne_bytes());
    fill_random(&mut header[SWAP_UUID_OFFSET..SWAP_UUID_OFFSET + 16], span)?;
    let magic_offset = page_size - SWAP_MAGIC.len();
    file.seek(SeekFrom::Start(0))
        .and_then(|_| file.write_all(&vec![0_u8; SWAP_HEADER_OFFSET as usize]))
        .and_then(|_| file.seek(SeekFrom::Start(SWAP_HEADER_OFFSET)))
        .and_then(|_| file.write_all(&header))
        .and_then(|_| file.seek(SeekFrom::Start(magic_offset as u64)))
        .and_then(|_| file.write_all(SWAP_MAGIC))
        .and_then(|_| file.flush())
        .and_then(|_| file.sync_all())
        .map_err(|error| RuntimeError::new("linux-mkswap", error.to_string()).with_span(span))
}

fn device_size(file: &File, span: Span) -> Result<u64, RuntimeError> {
    let mut size = 0_u64;
    let rc = unsafe { libc::ioctl(file.as_raw_fd(), BLKGETSIZE64 as _, &mut size) };
    if rc == 0 && size > 0 {
        return Ok(size);
    }

    file.metadata()
        .map(|metadata| metadata.len())
        .map_err(|error| RuntimeError::new("linux-mkswap", error.to_string()).with_span(span))
}

fn fill_random(buffer: &mut [u8], span: Span) -> Result<(), RuntimeError> {
    File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(buffer))
        .map_err(|error| RuntimeError::new("linux-mkswap", error.to_string()).with_span(span))
}

fn page_size(_span: Span) -> Result<usize, RuntimeError> {
    Ok(rustix::param::page_size())
}
