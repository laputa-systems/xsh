use std::io;
use std::num::NonZeroUsize;

use rustix::process::Resource;

use crate::transfer::{
    EngineOptions, MAX_DESCRIPTORS, MAX_JOBS, MIN_DESCRIPTORS, RECORD_MEMORY_BYTES,
    RESERVED_DESCRIPTORS, WORKER_DESCRIPTORS, WORKER_MEMORY_BYTES,
};

fn descriptor_budget() -> usize {
    let limit = rustix::process::getrlimit(Resource::Nofile)
        .current
        .unwrap_or(u64::MAX);
    usize::try_from(limit.saturating_sub(RESERVED_DESCRIPTORS as u64))
        .unwrap_or(usize::MAX)
        .min(MAX_DESCRIPTORS)
}

fn address_space_available() -> io::Result<Option<u64>> {
    let Some(limit) = rustix::process::getrlimit(Resource::As).current else {
        return Ok(None);
    };
    let status = std::fs::read_to_string("/proc/self/status")?;
    let used = status
        .lines()
        .find_map(|line| line.strip_prefix("VmSize:"))
        .and_then(|line| line.split_whitespace().next())
        .ok_or_else(|| io::Error::other("address-space usage is unavailable"))?
        .parse::<u64>()
        .map_err(|_| io::Error::other("address-space usage is malformed"))?
        .checked_mul(1024)
        .ok_or_else(|| io::Error::other("address-space usage overflow"))?;
    Ok(Some(
        limit
            .saturating_sub(used)
            .saturating_sub(RECORD_MEMORY_BYTES as u64),
    ))
}

fn memory_jobs() -> io::Result<usize> {
    match address_space_available()? {
        Some(available) => Ok(usize::try_from(available / WORKER_MEMORY_BYTES as u64)
            .unwrap_or(usize::MAX)
            .min(MAX_JOBS)),
        None => Ok(MAX_JOBS),
    }
}

fn default_jobs() -> usize {
    std::thread::available_parallelism()
        .map(NonZeroUsize::get)
        .unwrap_or(1)
        .min(4)
}

pub(crate) fn worker_count(options: &EngineOptions) -> io::Result<usize> {
    let budget = descriptor_budget();
    if budget < MIN_DESCRIPTORS {
        return Err(io::Error::other(
            "fewer than 16 descriptors remain after the reservation",
        ));
    }
    let descriptor_jobs = (budget.saturating_sub(10) / WORKER_DESCRIPTORS).clamp(1, MAX_JOBS);
    let allowed = descriptor_jobs.min(memory_jobs()?);
    if allowed == 0 {
        return Err(io::Error::new(
            if options.jobs.is_some() {
                io::ErrorKind::InvalidInput
            } else {
                io::ErrorKind::OutOfMemory
            },
            "the address-space budget cannot admit a transfer worker",
        ));
    }
    match options.jobs {
        Some(jobs) if jobs.get() > allowed => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "jobs exceeds the descriptor or address-space budget",
        )),
        Some(jobs) => Ok(jobs.get()),
        None => Ok(default_jobs().min(allowed)),
    }
}

