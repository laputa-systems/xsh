//! Internal byte/text search primitives shared by lowered IR, normal runtime
//! methods, and host parsers.
//!
//! These helpers are deliberately `Value`-free: they operate on `&[u8]` and
//! `&str` only. Error messages and public method semantics stay with the
//! runtime owners that call into here. The point is to route every hot
//! byte-search and line-count site through the same `memchr`-backed
//! implementation instead of hand-rolled `windows()` / `lines()` loops.

use bstr::ByteSlice;

/// Trim leading and trailing whitespace from `bytes`. Pure-ASCII input uses the
/// same `is_ascii_whitespace` scan as the lowered `Str` trim fast path, so byte
/// trimming stays consistent with the existing scanner behavior; other input
/// falls back to bstr's Unicode-aware trim, matching `str::trim`'s
/// `White_Space` semantics on valid UTF-8.
pub fn trim_bytes(bytes: &[u8]) -> &[u8] {
    if bytes.is_ascii() {
        let start = bytes
            .iter()
            .position(|byte| !byte.is_ascii_whitespace())
            .unwrap_or(bytes.len());
        let end = bytes
            .iter()
            .rposition(|byte| !byte.is_ascii_whitespace())
            .map_or(start, |index| index + 1);
        &bytes[start..end]
    } else {
        bytes.trim()
    }
}

/// Count the lines in `bytes` with the same semantics as
/// [`count_lines`] for `&str`: one line per `\n`-terminated segment, no
/// trailing empty line for a final `\n`, and zero for empty input.
pub fn count_lines_bytes(bytes: &[u8]) -> usize {
    if bytes.is_empty() {
        return 0;
    }
    let newlines = memchr::memchr_iter(b'\n', bytes).count();
    if bytes[bytes.len() - 1] == b'\n' {
        newlines
    } else {
        newlines + 1
    }
}

/// Find the first occurrence of `needle` in `haystack`, returning its byte
/// offset. An empty needle matches at offset 0, mirroring `str::find("")`.
pub fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    match needle {
        [] => Some(0),
        [byte] => memchr::memchr(*byte, haystack),
        _ => memchr::memmem::find(haystack, needle),
    }
}

/// Whether `haystack` contains `needle`.
pub fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    find_bytes(haystack, needle).is_some()
}

/// Count the lines in `text` using the same semantics as `str::lines().count()`
/// but with a `memchr`-accelerated newline scan. `str::lines()` yields one item
/// per `\n`-terminated segment, does not produce a trailing empty line for a
/// final `\n`, and yields nothing for an empty string.
pub fn count_lines(text: &str) -> usize {
    count_lines_bytes(text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_lines_matches_std() {
        for case in [
            "",
            "\n",
            "\n\n",
            "a",
            "a\n",
            "a\nb",
            "a\nb\n",
            "a\r\nb\r\n",
            "a\r\nb",
            "trailing whitespace \n  \n",
        ] {
            assert_eq!(
                count_lines(case),
                case.lines().count(),
                "count_lines mismatch for {case:?}"
            );
        }
    }

    #[test]
    fn count_lines_bytes_matches_str_count() {
        for case in ["", "\n", "a\nb", "a\nb\n", "a\r\nb\r\n"] {
            assert_eq!(count_lines_bytes(case.as_bytes()), case.lines().count());
        }
    }

    #[test]
    fn trim_bytes_matches_str_trim_on_utf8() {
        for case in ["", "  ", " a ", "\t\nx\r\n", "no-trim", "  héllo  "] {
            assert_eq!(
                trim_bytes(case.as_bytes()),
                case.trim().as_bytes(),
                "trim_bytes mismatch for {case:?}"
            );
        }
    }

    #[test]
    fn find_bytes_matches_str_find() {
        let haystack = "the quick brown fox";
        for needle in ["", "the", "quick", "fox", "zzz", "x"] {
            assert_eq!(
                find_bytes(haystack.as_bytes(), needle.as_bytes()),
                haystack.find(needle),
                "find_bytes mismatch for {needle:?}"
            );
        }
    }
}
