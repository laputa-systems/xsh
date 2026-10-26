/// `assert!` that's only checked in debug builds and is otherwise an
/// optimization hint in release builds.
macro_rules! assert_unchecked {
    ($condition:expr $(, $message:expr)* $(,)?) => {
        if cfg!(debug_assertions) {
            assert!($condition $(, $message)*);
        } else {
            $crate::util::assert_unchecked($condition);
        }
    }
}
