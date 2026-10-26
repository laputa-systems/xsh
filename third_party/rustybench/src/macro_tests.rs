//! Focused macro/runner coverage kept in the library test binary.

use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};

use crate::{
    __private::{BENCH_ENTRIES, EntryMeta, GROUP_ENTRIES},
    Rustybench,
};

static CHILD1_ITERS: AtomicUsize = AtomicUsize::new(0);
static CHILD2_ITERS: AtomicUsize = AtomicUsize::new(0);
static CHILD3_ITERS: AtomicUsize = AtomicUsize::new(0);

#[crate::bench_group(crate = crate, sample_count = 10, sample_size = 50)]
mod attr_options {
    use super::*;

    #[crate::bench_group(crate = crate, sample_size = 1)]
    mod child1 {
        use super::*;

        #[crate::bench(crate = crate)]
        fn bench() {
            CHILD1_ITERS.fetch_add(1, SeqCst);
        }
    }

    #[crate::bench_group(crate = crate, sample_count = 42)]
    mod child2 {
        use super::*;

        #[crate::bench(crate = crate)]
        fn bench() {
            CHILD2_ITERS.fetch_add(1, SeqCst);
        }
    }

    mod child3 {
        use super::*;

        #[crate::bench(crate = crate, sample_count = 1)]
        fn bench() {
            CHILD3_ITERS.fetch_add(1, SeqCst);
        }
    }
}

// These entries exist only to exercise registration metadata. Keeping them
// ignored prevents the iteration-count test from benchmarking its fixtures.
#[crate::bench(crate = crate, ignore)]
fn raw_ident() {}

#[crate::bench(crate = crate, name = "raw name ident", ignore)]
fn raw_name_ident() {}

#[crate::bench_group(crate = crate, ignore)]
mod metadata_group {
    #[crate::bench(crate = crate, ignore)]
    fn inner() {}

    #[crate::bench_group(crate = crate, ignore)]
    mod nested_group {}
}

fn find_bench(raw_name: &str) -> &'static crate::__private::BenchEntry {
    BENCH_ENTRIES
        .iter()
        .find(|entry| entry.meta.raw_name == raw_name)
        .unwrap_or_else(|| panic!("{raw_name} not found"))
}

fn find_group(raw_name: &str) -> &'static crate::__private::GroupEntry {
    GROUP_ENTRIES
        .iter()
        .find(|entry| entry.meta.raw_name == raw_name)
        .unwrap_or_else(|| panic!("{raw_name} not found"))
}

fn ignored(meta: &EntryMeta) -> bool {
    meta.bench_options
        .as_ref()
        .and_then(|options| options.ignore)
        .unwrap_or_default()
}

#[test]
fn attribute_options_inherit_and_override() {
    Rustybench::default().run_benches();

    assert_eq!(CHILD1_ITERS.load(SeqCst), 10);
    assert_eq!(CHILD2_ITERS.load(SeqCst), 2100);
    assert_eq!(CHILD3_ITERS.load(SeqCst), 50);
}

#[test]
fn registration_metadata_preserves_names_and_paths() {
    let raw = find_bench("raw_ident");
    assert_eq!(raw.meta.display_name, "raw_ident");
    assert!(raw.meta.module_path.ends_with("::macro_tests"));
    assert!(ignored(&raw.meta));

    let renamed = find_bench("raw_name_ident");
    assert_eq!(renamed.meta.display_name, "raw name ident");

    let inner = find_bench("inner");
    assert!(
        inner
            .meta
            .module_path
            .ends_with("::macro_tests::metadata_group")
    );
    assert!(ignored(&inner.meta));

    let group = find_group("nested_group");
    assert!(group.meta.module_path.ends_with("::metadata_group"));
    assert!(ignored(&group.meta));
}
