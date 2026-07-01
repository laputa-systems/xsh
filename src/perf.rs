#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AllocationSnapshot {
    /// Number of allocations (alloc + realloc-to-larger).
    pub allocation_calls: u64,
    /// Total bytes passed to `alloc` (layout size, pre-bin-rounding).
    pub allocation_bytes: u64,
    /// Number of deallocations.
    pub deallocation_calls: u64,
    /// Total bytes passed to `dealloc`.
    pub deallocation_bytes: u64,
    /// Number of reallocations.
    pub reallocation_calls: u64,
    /// New size bytes passed to `realloc`.
    pub reallocation_bytes: u64,

    // Allocation count by requested-size bucket (alloc only, not realloc):
    /// ≤ 16 bytes  (raw string data for short keys, small scalars)
    pub alloc_calls_le16: u64,
    /// 17 – 64 bytes  (String/Vec headers, small records)
    pub alloc_calls_le64: u64,
    /// 65 – 256 bytes  (BTreeMap nodes, medium strings, path buffers)
    pub alloc_calls_le256: u64,
    /// 257 – 4096 bytes  (large nodes, Vec data)
    pub alloc_calls_le4096: u64,
    /// > 4096 bytes  (large buffers)
    pub alloc_calls_gt4096: u64,

    // Mimalloc process-level metrics (always populated when XSH_PERF_ALLOC=1):
    /// Peak resident set size in bytes (from mi_process_info).
    pub peak_rss_bytes: u64,
}

// In-process heap profiler. With `--features dhat-heap` the dhat allocator becomes
// the global allocator and records a per-call-stack backtrace for every allocation;
// the `dhat::Profiler` guard created in the binary entrypoints writes the
// dh_view.html-compatible JSON on exit. This works on musl (no libc malloc
// interception, unlike Valgrind DHAT) precisely because we own the allocator.
//
// `perf-metrics` and `dhat-heap` each install a `#[global_allocator]`, so they are
// mutually exclusive. They only ever coincide under `--all-features` (e.g. `cargo
// clippy --all-features` in `make lint`); there `perf-metrics` takes precedence and
// dhat goes inert so the crate still builds with a single global allocator. That
// combination is never a real profiling build.
#[cfg(all(feature = "dhat-heap", not(feature = "perf-metrics")))]
#[global_allocator]
static DHAT_ALLOC: dhat::Alloc = dhat::Alloc;

#[cfg(feature = "perf-metrics")]
mod mi_ffi {
    unsafe extern "C" {
        /// Flush per-thread allocation stats into the global heap.
        /// Must be called before mi_stats_reset / mi_stats_get_json to get
        /// accurate mid-execution counts.
        pub(super) fn mi_collect(force: bool);

        /// Reset all mimalloc internal stat counters to zero.
        pub(super) fn mi_stats_reset();

        /// Return process-level memory and timing info. All out-params are
        /// optional (null-safe). peak_rss is precise on macOS/Windows; on
        /// Linux it uses current_commit as a proxy.
        pub(super) fn mi_process_info(
            elapsed_msecs: *mut usize,
            user_msecs: *mut usize,
            system_msecs: *mut usize,
            current_rss: *mut usize,
            peak_rss: *mut usize,
            current_commit: *mut usize,
            peak_commit: *mut usize,
            page_faults: *mut usize,
        );
    }
}

#[cfg(feature = "perf-metrics")]
fn mi_peak_rss() -> u64 {
    let mut peak_rss = 0usize;
    unsafe {
        mi_ffi::mi_process_info(
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut peak_rss,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
    }
    peak_rss as u64
}

// Build with `--features perf-metrics` to enable. This installs CountingAllocator
// as the global allocator; it intercepts every alloc/dealloc/realloc call and
// increments atomic counters, then delegates to mimalloc so production allocator
// behaviour is preserved.

#[cfg(feature = "perf-metrics")]
mod imp {
    use super::{AllocationSnapshot, mi_ffi, mi_peak_rss};
    use core::ffi::c_void;
    use libmimalloc_sys as mi;
    use std::alloc::{GlobalAlloc, Layout};
    use std::sync::atomic::{AtomicU64, Ordering};

    pub struct CountingAllocator;

    static ALLOC_CALLS: AtomicU64 = AtomicU64::new(0);
    static ALLOC_BYTES: AtomicU64 = AtomicU64::new(0);
    static DEALLOC_CALLS: AtomicU64 = AtomicU64::new(0);
    static DEALLOC_BYTES: AtomicU64 = AtomicU64::new(0);
    static REALLOC_CALLS: AtomicU64 = AtomicU64::new(0);
    static REALLOC_BYTES: AtomicU64 = AtomicU64::new(0);
    // Size-class histogram for alloc only:
    static ALLOC_LE16: AtomicU64 = AtomicU64::new(0);
    static ALLOC_LE64: AtomicU64 = AtomicU64::new(0);
    static ALLOC_LE256: AtomicU64 = AtomicU64::new(0);
    static ALLOC_LE4096: AtomicU64 = AtomicU64::new(0);
    static ALLOC_GT4096: AtomicU64 = AtomicU64::new(0);

    #[global_allocator]
    static GLOBAL_ALLOCATOR: CountingAllocator = CountingAllocator;

    #[inline]
    fn size_bucket(size: usize) -> &'static AtomicU64 {
        match size {
            0..=16 => &ALLOC_LE16,
            17..=64 => &ALLOC_LE64,
            65..=256 => &ALLOC_LE256,
            257..=4096 => &ALLOC_LE4096,
            _ => &ALLOC_GT4096,
        }
    }

    unsafe impl GlobalAlloc for CountingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let ptr = unsafe { mi::mi_malloc_aligned(layout.size(), layout.align()) } as *mut u8;
            if !ptr.is_null() {
                ALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
                ALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
                size_bucket(layout.size()).fetch_add(1, Ordering::Relaxed);
            }
            ptr
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            DEALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
            DEALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
            unsafe { mi::mi_free(ptr.cast::<c_void>()) };
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            let next =
                unsafe { mi::mi_realloc_aligned(ptr.cast::<c_void>(), new_size, layout.align()) }
                    as *mut u8;
            if !next.is_null() {
                REALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
                REALLOC_BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
            }
            next
        }
    }

    pub fn allocation_metrics_requested() -> bool {
        std::env::var_os("XSH_PERF_ALLOC").is_some()
    }

    pub fn reset_allocations() {
        ALLOC_CALLS.store(0, Ordering::Relaxed);
        ALLOC_BYTES.store(0, Ordering::Relaxed);
        DEALLOC_CALLS.store(0, Ordering::Relaxed);
        DEALLOC_BYTES.store(0, Ordering::Relaxed);
        REALLOC_CALLS.store(0, Ordering::Relaxed);
        REALLOC_BYTES.store(0, Ordering::Relaxed);
        ALLOC_LE16.store(0, Ordering::Relaxed);
        ALLOC_LE64.store(0, Ordering::Relaxed);
        ALLOC_LE256.store(0, Ordering::Relaxed);
        ALLOC_LE4096.store(0, Ordering::Relaxed);
        ALLOC_GT4096.store(0, Ordering::Relaxed);
        // Also reset mimalloc's internal counters so mi_process_info reflects
        // only post-reset activity.
        unsafe { mi_ffi::mi_collect(false) };
        unsafe { mi_ffi::mi_stats_reset() };
    }

    pub fn allocation_snapshot() -> Option<AllocationSnapshot> {
        // Flush per-thread stats before reading process info.
        unsafe { mi_ffi::mi_collect(false) };
        Some(AllocationSnapshot {
            allocation_calls: ALLOC_CALLS.load(Ordering::Relaxed),
            allocation_bytes: ALLOC_BYTES.load(Ordering::Relaxed),
            deallocation_calls: DEALLOC_CALLS.load(Ordering::Relaxed),
            deallocation_bytes: DEALLOC_BYTES.load(Ordering::Relaxed),
            reallocation_calls: REALLOC_CALLS.load(Ordering::Relaxed),
            reallocation_bytes: REALLOC_BYTES.load(Ordering::Relaxed),
            alloc_calls_le16: ALLOC_LE16.load(Ordering::Relaxed),
            alloc_calls_le64: ALLOC_LE64.load(Ordering::Relaxed),
            alloc_calls_le256: ALLOC_LE256.load(Ordering::Relaxed),
            alloc_calls_le4096: ALLOC_LE4096.load(Ordering::Relaxed),
            alloc_calls_gt4096: ALLOC_GT4096.load(Ordering::Relaxed),
            peak_rss_bytes: mi_peak_rss(),
        })
    }
}

#[cfg(not(feature = "perf-metrics"))]
mod imp {
    use super::AllocationSnapshot;

    pub fn allocation_metrics_requested() -> bool {
        false
    }

    pub fn reset_allocations() {}

    pub fn allocation_snapshot() -> Option<AllocationSnapshot> {
        None
    }
}

pub use imp::{allocation_metrics_requested, allocation_snapshot, reset_allocations};
