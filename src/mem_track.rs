//! Optional allocation traffic tracking for frontend/IR diagnostics.
//!
//! Counters only move when the dedicated frontend-stats binary installs
//! [`CountingAllocator`] as the global allocator. Library callers still run
//! and report zero peak/traffic when tracking is inactive.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

static TRACKING_INSTALLED: AtomicBool = AtomicBool::new(false);
static WORKER_COLLECTION: OnceLock<Mutex<WorkerCollection>> = OnceLock::new();

thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    static LIVE_BYTES: Cell<usize> = const { Cell::new(0) };
    static PEAK_BYTES: Cell<usize> = const { Cell::new(0) };
    static ALLOC_COUNT: Cell<usize> = const { Cell::new(0) };
    static ALLOC_BYTES: Cell<usize> = const { Cell::new(0) };
}

/// Marker global allocator used only by the frontend-stats binary.
pub struct CountingAllocator;

impl CountingAllocator {
    pub const fn new() -> Self {
        Self
    }

    pub fn install_marker() {
        TRACKING_INSTALLED.store(true, Ordering::Relaxed);
    }
}

impl Default for CountingAllocator {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            record_alloc(layout.size());
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            record_alloc(layout.size());
        }
        ptr
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_ptr = unsafe { System.realloc(ptr, layout, new_size) };
        if !new_ptr.is_null() {
            record_realloc(layout.size(), new_size);
        }
        new_ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        record_dealloc(layout.size());
        unsafe { System.dealloc(ptr, layout) }
    }
}

fn record_alloc(size: usize) {
    ENABLED.with(|enabled| {
        if !enabled.get() {
            return;
        }
        ALLOC_COUNT.with(|count| count.set(count.get().saturating_add(1)));
        ALLOC_BYTES.with(|bytes| bytes.set(bytes.get().saturating_add(size)));
        LIVE_BYTES.with(|live| {
            let next = live.get().saturating_add(size);
            live.set(next);
            PEAK_BYTES.with(|peak| {
                if next > peak.get() {
                    peak.set(next);
                }
            });
        });
    });
}

fn record_dealloc(size: usize) {
    ENABLED.with(|enabled| {
        if !enabled.get() {
            return;
        }
        LIVE_BYTES.with(|live| live.set(live.get().saturating_sub(size)));
    });
}

fn record_realloc(old_size: usize, new_size: usize) {
    ENABLED.with(|enabled| {
        if !enabled.get() {
            return;
        }
        if new_size >= old_size {
            let delta = new_size - old_size;
            ALLOC_COUNT.with(|count| count.set(count.get().saturating_add(1)));
            ALLOC_BYTES.with(|bytes| bytes.set(bytes.get().saturating_add(delta)));
            LIVE_BYTES.with(|live| {
                let next = live.get().saturating_add(delta);
                live.set(next);
                PEAK_BYTES.with(|peak| {
                    if next > peak.get() {
                        peak.set(next);
                    }
                });
            });
        } else {
            let delta = old_size - new_size;
            LIVE_BYTES.with(|live| live.set(live.get().saturating_sub(delta)));
        }
    });
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AllocTraffic {
    pub live_bytes: usize,
    pub peak_bytes: usize,
    pub alloc_count: usize,
    pub alloc_bytes: usize,
    pub tracking_active: bool,
}

#[derive(Default)]
struct WorkerCollection {
    active: bool,
    stages: Vec<AllocTraffic>,
}

/// A per-worker allocation scope used by the dedicated runtime-stats binary.
///
/// Product allocators never install [`CountingAllocator`], so creating this
/// scope is a no-op in normal execution. The collected counters remain
/// thread-local: allocation ownership can cross worker boundaries, so callers
/// must treat worker peaks as allocation-pressure evidence rather than process
/// RSS or an exact concurrent-live total.
pub struct WorkerStage {
    active: bool,
}

pub fn tracking_installed() -> bool {
    TRACKING_INSTALLED.load(Ordering::Relaxed)
}

/// Start collecting allocation traffic from explicitly instrumented runtime
/// workers. The caller must end the collection after joining every worker.
pub fn begin_worker_collection() {
    if !tracking_installed() {
        return;
    }
    let mut collection = worker_collection().lock().expect("worker allocation lock poisoned");
    collection.active = true;
    collection.stages.clear();
}

/// Finish a worker collection and return one thread-local traffic sample per
/// participating worker.
pub fn end_worker_collection() -> Vec<AllocTraffic> {
    if !tracking_installed() {
        return Vec::new();
    }
    let mut collection = worker_collection().lock().expect("worker allocation lock poisoned");
    collection.active = false;
    std::mem::take(&mut collection.stages)
}

/// Begin tracking one explicitly instrumented worker.
pub fn begin_worker_stage() -> WorkerStage {
    if !tracking_installed() {
        return WorkerStage { active: false };
    }
    let active = worker_collection()
        .lock()
        .expect("worker allocation lock poisoned")
        .active;
    if active {
        begin_stage();
    }
    WorkerStage { active }
}

impl Drop for WorkerStage {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let traffic = end_stage();
        let mut collection = worker_collection().lock().expect("worker allocation lock poisoned");
        if collection.active {
            collection.stages.push(traffic);
        }
    }
}

fn worker_collection() -> &'static Mutex<WorkerCollection> {
    WORKER_COLLECTION.get_or_init(|| Mutex::new(WorkerCollection::default()))
}

pub fn begin_stage() {
    ENABLED.with(|enabled| enabled.set(true));
    let live = LIVE_BYTES.with(Cell::get);
    PEAK_BYTES.with(|peak| peak.set(live));
    ALLOC_COUNT.with(|count| count.set(0));
    ALLOC_BYTES.with(|bytes| bytes.set(0));
}

pub fn end_stage() -> AllocTraffic {
    let traffic = snapshot();
    ENABLED.with(|enabled| enabled.set(false));
    traffic
}

pub fn snapshot() -> AllocTraffic {
    AllocTraffic {
        live_bytes: LIVE_BYTES.with(Cell::get),
        peak_bytes: PEAK_BYTES.with(Cell::get),
        alloc_count: ALLOC_COUNT.with(Cell::get),
        alloc_bytes: ALLOC_BYTES.with(Cell::get),
        tracking_active: tracking_installed(),
    }
}
