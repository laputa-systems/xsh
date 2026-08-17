//! Optional allocation traffic tracking for frontend/IR diagnostics.
//!
//! Counters only move when a dedicated diagnostics binary installs
//! [`CountingAllocator`] as the global allocator. Library callers still run
//! and report zero peak/traffic when tracking is inactive.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

static TRACKING_INSTALLED: AtomicBool = AtomicBool::new(false);
static WORKER_COLLECTION: OnceLock<Mutex<WorkerCollection>> = OnceLock::new();

const WORKER_ALLOCATION_SCOPE_COUNT: usize = 4;

/// A named segment within an explicitly instrumented indexed worker.
#[repr(usize)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkerAllocationScope {
    Setup,
    ParMapResults,
    ParMapItem,
    FusedReduceItem,
}

impl WorkerAllocationScope {
    pub const ALL: [Self; WORKER_ALLOCATION_SCOPE_COUNT] = [
        Self::Setup,
        Self::ParMapResults,
        Self::ParMapItem,
        Self::FusedReduceItem,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Setup => "worker_setup",
            Self::ParMapResults => "par_map_results",
            Self::ParMapItem => "par_map_item",
            Self::FusedReduceItem => "fused_reduce_item",
        }
    }

    const fn index(self) -> usize {
        self as usize
    }
}

/// Allocation traffic whose ownership cannot be inferred from deallocation.
///
/// Worker values may cross thread boundaries, so scoped counters deliberately
/// record allocation events and bytes only; the enclosing worker stage owns the
/// thread-local live and peak accounting.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ScopedAllocTraffic {
    pub alloc_count: usize,
    pub alloc_bytes: usize,
}

impl ScopedAllocTraffic {
    const ZERO: Self = Self {
        alloc_count: 0,
        alloc_bytes: 0,
    };

    fn record(&mut self, size: usize) {
        self.alloc_count = self.alloc_count.saturating_add(1);
        self.alloc_bytes = self.alloc_bytes.saturating_add(size);
    }
}

thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    static LIVE_BYTES: Cell<usize> = const { Cell::new(0) };
    static PEAK_BYTES: Cell<usize> = const { Cell::new(0) };
    static ALLOC_COUNT: Cell<usize> = const { Cell::new(0) };
    static ALLOC_BYTES: Cell<usize> = const { Cell::new(0) };
    static WORKER_ALLOCATION_SCOPE: Cell<Option<WorkerAllocationScope>> = const { Cell::new(None) };
    static WORKER_SCOPE_TRAFFIC: Cell<[ScopedAllocTraffic; WORKER_ALLOCATION_SCOPE_COUNT]> = const {
        Cell::new([ScopedAllocTraffic::ZERO; WORKER_ALLOCATION_SCOPE_COUNT])
    };
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
        record_worker_scope_alloc(size);
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
            record_worker_scope_alloc(delta);
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

fn record_worker_scope_alloc(size: usize) {
    WORKER_ALLOCATION_SCOPE.with(|scope| {
        let Some(scope) = scope.get() else {
            return;
        };
        WORKER_SCOPE_TRAFFIC.with(|traffic| {
            let mut counters = traffic.get();
            counters[scope.index()].record(size);
            traffic.set(counters);
        });
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
    stages: Vec<WorkerStageTraffic>,
}

/// Allocation counters from one explicitly instrumented worker thread.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WorkerStageTraffic {
    pub traffic: AllocTraffic,
    pub scopes: [ScopedAllocTraffic; WORKER_ALLOCATION_SCOPE_COUNT],
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

/// Restores the preceding worker allocation scope when dropped.
pub struct WorkerAllocationScopeGuard {
    active: bool,
    previous: Option<WorkerAllocationScope>,
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
pub fn end_worker_collection() -> Vec<WorkerStageTraffic> {
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
        WORKER_ALLOCATION_SCOPE.with(|scope| scope.set(None));
        WORKER_SCOPE_TRAFFIC.with(|traffic| {
            traffic.set([ScopedAllocTraffic::ZERO; WORKER_ALLOCATION_SCOPE_COUNT])
        });
    }
    WorkerStage { active }
}

impl Drop for WorkerStage {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let traffic = WorkerStageTraffic {
            traffic: end_stage(),
            scopes: WORKER_SCOPE_TRAFFIC.with(Cell::get),
        };
        WORKER_ALLOCATION_SCOPE.with(|scope| scope.set(None));
        let mut collection = worker_collection().lock().expect("worker allocation lock poisoned");
        if collection.active {
            collection.stages.push(traffic);
        }
    }
}

impl WorkerStage {
    /// Attribute allocations made while this guard is alive to one execution
    /// segment. Disabled product allocators return an inert guard.
    #[inline]
    pub fn scope(&self, scope: WorkerAllocationScope) -> WorkerAllocationScopeGuard {
        if !self.active {
            return WorkerAllocationScopeGuard {
                active: false,
                previous: None,
            };
        }
        let previous = WORKER_ALLOCATION_SCOPE.with(|current| current.replace(Some(scope)));
        WorkerAllocationScopeGuard {
            active: true,
            previous,
        }
    }
}

impl Drop for WorkerAllocationScopeGuard {
    fn drop(&mut self) {
        if self.active {
            WORKER_ALLOCATION_SCOPE.with(|scope| scope.set(self.previous));
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
