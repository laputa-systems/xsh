pub(crate) fn count() -> i64 {
    std::thread::available_parallelism()
        .map(|count| count.get() as i64)
        .unwrap_or(1)
}
