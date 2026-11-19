#[cfg(not(target_env = "musl"))]
mod engine {
    use libc::{c_char, c_int, c_void, size_t};
    #[repr(C)]
    pub struct Regex { nsub: size_t, opaque: *mut c_void, padding: [*mut c_void; 4], nsub2: size_t, padding2: c_char }
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct Match { pub rm_so: isize, pub rm_eo: isize }
    pub const EXTENDED: c_int = 1;
    pub const ICASE: c_int = 2;
    pub const NOTBOL: c_int = 1;
    pub const NOMATCH: c_int = 1;
    unsafe extern "C" {
        #[link_name = "xsh_musl_regcomp"] pub fn regcomp(regex: *mut Regex, pattern: *const c_char, flags: c_int) -> c_int;
        #[link_name = "xsh_musl_regexec"] pub fn regexec(regex: *const Regex, text: *const c_char, n: size_t, matches: *mut Match, flags: c_int) -> c_int;
        #[link_name = "xsh_musl_regfree"] pub fn regfree(regex: *mut Regex);
        #[link_name = "xsh_musl_regerror"] pub fn regerror(code: c_int, regex: *const Regex, buffer: *mut c_char, size: size_t) -> size_t;
    }
}
#[cfg(target_env = "musl")]
mod engine {
    pub use libc::{regex_t as Regex, regmatch_t as Match, regcomp, regexec, regfree, regerror,
        REG_EXTENDED as EXTENDED, REG_ICASE as ICASE, REG_NOTBOL as NOTBOL, REG_NOMATCH as NOMATCH};
}
use std::ffi::CString;

pub(crate) struct PosixRegex { inner: engine::Regex }
impl PosixRegex {
    pub(crate) fn compile(pattern: &str, extended: bool, ignore_case: bool) -> Result<Self, String> {
        Self::compile_with_dialect(pattern, extended, ignore_case, false)
    }
    pub(crate) fn compile_host(pattern: &str, extended: bool, ignore_case: bool) -> Result<Self, String> {
        Self::compile_with_dialect(pattern, extended, ignore_case, true)
    }
    fn compile_with_dialect(pattern: &str, extended: bool, ignore_case: bool, host_dialect: bool) -> Result<Self, String> {
        let mut at = 0;
        let source = pattern.as_bytes();
        if !host_dialect {
            while at < source.len() {
                if source[at] == b'\\' {
                    at += 1;
                    if source.get(at).is_some_and(|b| matches!(b, b'b' | b'B' | b'<' | b'>')) {
                        return Err("GNU word-boundary regex escapes are unsupported by the POSIX byte matcher".into());
                    }
                }
                at += 1;
            }
        }
        let pattern = CString::new(pattern).map_err(|_| "NUL in regular expression".to_owned())?;
        let mut inner = unsafe { std::mem::zeroed::<engine::Regex>() };
        let flags = if extended { engine::EXTENDED } else { 0 } | if ignore_case { engine::ICASE } else { 0 };
        let code = unsafe { engine::regcomp(&mut inner, pattern.as_ptr(), flags) };
        if code != 0 { return Err(regex_error(code, &inner)); }
        Ok(Self { inner })
    }
    pub(crate) fn captures(&self, text: &[u8], start: usize) -> Result<Option<Vec<Option<(usize, usize)>>>, String> {
        let tail = text.get(start..).ok_or_else(|| "match offset outside input".to_owned())?;
        let input = CString::new(tail).map_err(|_| "POSIX regular expressions do not support NUL input on this platform".to_owned())?;
        let mut matches = [engine::Match { rm_so: -1, rm_eo: -1 }; 10];
        let flags = if start == 0 { 0 } else { engine::NOTBOL };
        let code = unsafe { engine::regexec(&self.inner, input.as_ptr(), matches.len(), matches.as_mut_ptr(), flags) };
        if code == engine::NOMATCH { return Ok(None); }
        if code != 0 { return Err(regex_error(code, &self.inner)); }
        Ok(Some(matches.iter().map(|m| if m.rm_so < 0 { None } else { Some((start + m.rm_so as usize, start + m.rm_eo as usize)) }).collect()))
    }
}
impl Drop for PosixRegex { fn drop(&mut self) { unsafe { engine::regfree(&mut self.inner) }; } }
fn regex_error(code: i32, regex: &engine::Regex) -> String {
    let size = unsafe { engine::regerror(code, regex, std::ptr::null_mut(), 0) };
    let mut bytes = vec![0; size];
    unsafe { engine::regerror(code, regex, bytes.as_mut_ptr().cast(), bytes.len()) };
    String::from_utf8_lossy(&bytes[..size.saturating_sub(1)]).into_owned()
}

#[cfg(test)]
mod tests {
    use super::PosixRegex;

    #[test]
    fn musl_bre_extensions_and_byte_classes() {
        for (pattern, text, expected) in [
            (r"a\|b", &b"b"[..], (0, 1)),
            (r"a\+", &b"aaa"[..], (0, 3)),
            (r"a\?b", &b"b"[..], (0, 1)),
            (r"a**", &b"aaa"[..], (0, 3)),
        ] {
            let re = PosixRegex::compile_host(pattern, false, false).unwrap();
            assert_eq!(re.captures(text, 0).unwrap().unwrap()[0], Some(expected), "{pattern}");
        }
        let re = PosixRegex::compile_host("[[:alpha:]]", false, false).unwrap();
        assert!(re.captures(&[0xe9], 0).unwrap().is_none());
    }
}
