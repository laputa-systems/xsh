use crate::error::{Error, Result};
use std::io::{self, Read};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

const GIB: u64 = 1 << 30;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub total_output: u64,
    pub entry_output: u64,
    pub entries: u64,
    pub index_bytes: u64,
    pub workspace_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            total_output: 256 * GIB,
            entry_output: 64 * GIB,
            entries: 4_194_304,
            index_bytes: 256 << 20,
            workspace_bytes: GIB,
        }
    }
}

type CancelFn = dyn Fn() -> bool + Send + Sync;

pub struct Budget {
    limits: Limits,
    total_output: AtomicU64,
    index_bytes: AtomicU64,
    entries: AtomicU64,
    cancel: Box<CancelFn>,
}

impl Budget {
    pub fn new(limits: Limits, cancel: impl Fn() -> bool + Send + Sync + 'static) -> Arc<Self> {
        Arc::new(Self {
            limits,
            total_output: AtomicU64::new(0),
            index_bytes: AtomicU64::new(0),
            entries: AtomicU64::new(0),
            cancel: Box::new(cancel),
        })
    }

    pub fn uncancelable(limits: Limits) -> Arc<Self> {
        Self::new(limits, || false)
    }

    pub const fn limits(&self) -> &Limits {
        &self.limits
    }

    pub fn total_output(&self) -> u64 {
        self.total_output.load(Ordering::Relaxed)
    }

    pub fn checkpoint(&self) -> Result<()> {
        if (self.cancel)() {
            Err(Error::canceled())
        } else {
            Ok(())
        }
    }

    pub fn output_allowance(&self) -> u64 {
        self.limits
            .total_output
            .saturating_sub(self.total_output.load(Ordering::Relaxed))
    }

    pub fn charge_output(&self, bytes: u64) -> Result<()> {
        let total = self
            .total_output
            .fetch_add(bytes, Ordering::Relaxed)
            .saturating_add(bytes);
        if total > self.limits.total_output {
            return Err(Error::limit(format!(
                "decoded output exceeds {} bytes",
                self.limits.total_output
            )));
        }
        Ok(())
    }

    pub fn charge_index(&self, bytes: u64) -> Result<()> {
        let total = self
            .index_bytes
            .fetch_add(bytes, Ordering::Relaxed)
            .saturating_add(bytes);
        if total > self.limits.index_bytes {
            return Err(Error::limit(format!(
                "entry metadata exceeds {} bytes",
                self.limits.index_bytes
            )));
        }
        Ok(())
    }

    pub fn release_index(&self, bytes: u64) {
        self.index_bytes.fetch_sub(bytes, Ordering::Relaxed);
    }

    pub fn count_entry(&self) -> Result<()> {
        let total = self.entries.fetch_add(1, Ordering::Relaxed) + 1;
        if total > self.limits.entries {
            return Err(Error::limit(format!(
                "archive has more than {} entries",
                self.limits.entries
            )));
        }
        Ok(())
    }

    pub fn declare_entry_size(&self, size: u64) -> Result<()> {
        if size > self.limits.entry_output {
            return Err(Error::limit(format!(
                "entry declares {size} bytes, over the {} byte entry limit",
                self.limits.entry_output
            )));
        }
        Ok(())
    }
}

pub struct Metered<R> {
    inner: R,
    budget: Arc<Budget>,
}

impl<R> Metered<R> {
    pub fn new(inner: R, budget: Arc<Budget>) -> Self {
        Self { inner, budget }
    }
}

impl<R: Read> Read for Metered<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.budget.checkpoint()?;
        let allowance = self.budget.output_allowance().saturating_add(1);
        let want = usize::try_from(allowance).map_or(buffer.len(), |allowed| allowed.min(buffer.len()));
        let count = self.inner.read(&mut buffer[..want])?;
        self.budget.charge_output(count as u64)?;
        Ok(count)
    }
}
