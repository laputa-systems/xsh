use std::ffi::CString;

pub(crate) struct PosixRegex { inner: libc::regex_t }
impl PosixRegex {
    pub(crate) fn compile(pattern: &str, extended: bool, ignore_case: bool) -> Result<Self, String> {
        let mut at = 0;
        let source = pattern.as_bytes();
        while at < source.len() {
            if source[at] == b'\\' {
                at += 1;
                if source.get(at).is_some_and(|b| matches!(b, b'b' | b'B' | b'<' | b'>')) {
                    return Err("GNU word-boundary regex escapes are unsupported by the POSIX byte matcher".into());
                }
            }
            at += 1;
        }
        let pattern = CString::new(pattern).map_err(|_| "NUL in regular expression".to_owned())?;
        let mut inner = unsafe { std::mem::zeroed::<libc::regex_t>() };
        let flags = if extended { libc::REG_EXTENDED } else { 0 } | if ignore_case { libc::REG_ICASE } else { 0 };
        let code = unsafe { libc::regcomp(&mut inner, pattern.as_ptr(), flags) };
        if code != 0 { return Err(regex_error(code, &inner)); }
        Ok(Self { inner })
    }
    pub(crate) fn captures(&self, text: &[u8], start: usize) -> Result<Option<Vec<Option<(usize, usize)>>>, String> {
        let tail = text.get(start..).ok_or_else(|| "match offset outside input".to_owned())?;
        let input = CString::new(tail).map_err(|_| "POSIX regular expressions do not support NUL input on this platform".to_owned())?;
        let mut matches = [libc::regmatch_t { rm_so: -1, rm_eo: -1 }; 10];
        let flags = if start == 0 { 0 } else { libc::REG_NOTBOL };
        let code = unsafe { libc::regexec(&self.inner, input.as_ptr(), matches.len(), matches.as_mut_ptr(), flags) };
        if code == libc::REG_NOMATCH { return Ok(None); }
        if code != 0 { return Err(regex_error(code, &self.inner)); }
        Ok(Some(matches.iter().map(|m| if m.rm_so < 0 { None } else { Some((start + m.rm_so as usize, start + m.rm_eo as usize)) }).collect()))
    }
}
impl Drop for PosixRegex { fn drop(&mut self) { unsafe { libc::regfree(&mut self.inner) }; } }
fn regex_error(code: i32, regex: &libc::regex_t) -> String {
    let size = unsafe { libc::regerror(code, regex, std::ptr::null_mut(), 0) };
    let mut bytes = vec![0; size];
    unsafe { libc::regerror(code, regex, bytes.as_mut_ptr().cast(), bytes.len()) };
    String::from_utf8_lossy(&bytes[..size.saturating_sub(1)]).into_owned()
}
