#![allow(clippy::single_call_fn)]

use crate::runtime::value::{PathValue, RuntimeError, Value};
use crate::source::Span;
use std::ffi::{CStr, CString};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub(crate) fn current(span: Span) -> Result<Value, RuntimeError> {
    by_uid(rustix::process::getuid().as_raw(), span)
}

pub(crate) fn lookup(name: &str, span: Span) -> Result<Value, RuntimeError> {
    let name = CString::new(name).map_err(|_| {
        RuntimeError::new("user-name", "user name cannot contain NUL").with_span(span)
    })?;
    let mut passwd: libc::passwd = unsafe { std::mem::zeroed() };
    let mut result = std::ptr::null_mut();
    let mut buffer = vec![0u8; passwd_buffer_size()];
    let rc = unsafe {
        libc::getpwnam_r(
            name.as_ptr(),
            &mut passwd,
            buffer.as_mut_ptr().cast::<libc::c_char>(),
            buffer.len(),
            &mut result,
        )
    };
    passwd_result(rc, result, passwd, span)
}

/// Resolve account memberships before entering a restricted filesystem or
/// changing process credentials. The requested primary group replaces the
/// account default; the default is not granted unless NSS lists membership.
pub(crate) fn groups(name: &str, primary_gid: Option<i64>, span: Span) -> Result<Value, RuntimeError> {
    let requested = primary_gid.map(|gid| {
        u32::try_from(gid).ok().filter(|gid| *gid != u32::MAX)
            .ok_or_else(|| RuntimeError::new("user-groups", "primary_gid must be between 0 and 4294967294").with_span(span))
    }).transpose()?;
    let account = lookup(name, span)?;
    let Value::Record(fields) = account else {
        unreachable!("user lookup returns an account record");
    };
    let Some(Value::Int(primary)) = fields.get("gid") else {
        unreachable!("account record includes a numeric primary gid");
    };
    let primary = requested.unwrap_or(*primary as u32);
    let name = CString::new(name).map_err(|_| {
        RuntimeError::new("user-name", "user name cannot contain NUL").with_span(span)
    })?;
    let mut groups = vec![0 as libc::gid_t; 16];
    loop {
        let mut count = groups.len() as libc::c_int;
        // Darwin uses signed 32-bit group IDs here; gid_t has the same width,
        // so the pointer cast preserves all ID bits on every supported host.
        let result = unsafe {
            libc::getgrouplist(name.as_ptr(), primary as _, groups.as_mut_ptr().cast(), &mut count)
        };
        if result >= 0 {
            let count = usize::try_from(count).ok().filter(|count| *count <= groups.len())
                .ok_or_else(|| RuntimeError::new("user-groups", "invalid group count from host").with_span(span))?;
            groups.truncate(count);
            groups.push(primary as libc::gid_t);
            groups.sort_unstable();
            groups.dedup();
            return Ok(Value::List(groups.into_iter().map(|id| Value::Int(i64::from(id))).collect()));
        }
        let required = usize::try_from(count).ok().filter(|count| *count > groups.len())
            .unwrap_or_else(|| groups.len().saturating_mul(2));
        if required > 65_536 {
            return Err(RuntimeError::new("user-groups", "host group list exceeds the supported size").with_span(span));
        }
        groups.try_reserve_exact(required - groups.len())
            .map_err(|error| RuntimeError::new("user-groups", error.to_string()).with_span(span))?;
        groups.resize(required, 0);
    }
}

pub(crate) fn by_uid(uid: u32, span: Span) -> Result<Value, RuntimeError> {
    let mut passwd: libc::passwd = unsafe { std::mem::zeroed() };
    let mut result = std::ptr::null_mut();
    let mut buffer = vec![0u8; passwd_buffer_size()];
    let rc = unsafe {
        libc::getpwuid_r(
            uid as libc::uid_t,
            &mut passwd,
            buffer.as_mut_ptr().cast::<libc::c_char>(),
            buffer.len(),
            &mut result,
        )
    };
    passwd_result(rc, result, passwd, span)
}

pub(crate) fn add(
    name: &str,
    uid: Option<i64>,
    gid: Option<i64>,
    home: Option<PathValue>,
    shell: Option<PathValue>,
    gecos: &str,
    span: Span,
) -> Result<Value, RuntimeError> {
    validate_name(name, "user-name", span)?;
    let name = AccountField::new(name, "user-add", span)?;
    let gecos = AccountField::new(gecos, "user-add", span)?;
    let home_path = match home {
        Some(value) => String::from_utf8(value.bytes).map_err(|_| RuntimeError::new("user-add", "home is not UTF-8").with_span(span))?,
        None => format!("/home/{}", name.0),
    };
    let shell_path = match shell {
        Some(value) => String::from_utf8(value.bytes).map_err(|_| RuntimeError::new("user-add", "shell is not UTF-8").with_span(span))?,
        None => "/bin/sh".to_string(),
    };
    let home = AccountField::new(&home_path, "user-add", span)?;
    let shell = AccountField::new(&shell_path, "user-add", span)?;
    let passwd_path = passwd_file();
    let shadow_path = shadow_file();
    let mut text = read_optional(&passwd_path, span, "user-add")?;
    let users = parse_passwd(&text, span, "user-add")?;
    if users.iter().any(|entry| entry.name == name.0) {
        return Err(RuntimeError::new("user-add", "user already exists").with_span(span));
    }
    let uid = uid.unwrap_or_else(|| next_id(users.iter().map(|entry| entry.uid)));
    let gid = gid.unwrap_or(uid);
    account_id(uid, "user-add", span)?;
    account_id(gid, "user-add", span)?;
    let shadow_text = read_optional(&shadow_path, span, "user-add")?;
    if !text.is_empty() && !text.ends_with('\n') { text.push('\n'); }
    text.push_str(&format!("{}:x:{}:{}:{}:{}:{}\n", name.0, uid, gid, gecos.0, home.0, shell.0));
    let mut shadow = shadow_text.clone();
    if !shadow.is_empty() && !shadow.lines().any(|line| line.split(':').next() == Some(name.0)) {
        if !shadow.ends_with('\n') { shadow.push('\n'); }
        shadow.push_str(&format!("{}:!:0:0:99999:7:::\n", name.0));
    }
    super::fs::write_atomic(passwd_path, text.as_bytes(), span).map_err(|error| account_write_error(error, "user-add"))?;
    if shadow != shadow_text { super::fs::write_atomic(shadow_path, shadow.as_bytes(), span).map_err(|error| account_write_error(error, "user-add"))?; }
    Ok(Value::Record(crate::runtime::value::RecordMap::from([
        (Arc::from("name"), Value::Str(name.0.into())),
        (Arc::from("uid"), Value::Int(uid)),
        (Arc::from("gid"), Value::Int(gid)),
        (
            Arc::from("home"),
            Value::Path(PathValue::new(home_path.into_bytes())?),
        ),
        (Arc::from("shell"), Value::Str(shell_path.into())),
    ])))
}

pub(crate) fn remove(name: &str, remove_home: bool, span: Span) -> Result<Value, RuntimeError> {
    validate_name(name, "user-name", span)?;
    let passwd_path = passwd_file();
    let shadow_path = shadow_file();
    let users_text = read_optional(&passwd_path, span, "user-remove")?;
    let users = parse_passwd(&users_text, span, "user-remove")?;
    let Some(entry) = users.iter().find(|entry| entry.name == name) else {
        return Err(RuntimeError::new("user-remove", "user was not found").with_span(span));
    };
    let shadow_text = read_optional(&shadow_path, span, "user-remove")?;
    let filtered = users_text.split_inclusive('\n').filter(|line| line.split(':').next() != Some(name)).collect::<String>();
    let shadow = shadow_text.split_inclusive('\n').filter(|line| line.split(':').next() != Some(name)).collect::<String>();
    super::fs::write_atomic(passwd_path, filtered.as_bytes(), span).map_err(|error| account_write_error(error, "user-remove"))?;
    if !shadow_text.is_empty() { super::fs::write_atomic(shadow_path, shadow.as_bytes(), span).map_err(|error| account_write_error(error, "user-remove"))?; }
    if remove_home {
        let _ = fs::remove_dir_all(&entry.home);
    }
    Ok(Value::Unit)
}

pub(crate) fn uid_from_i64(uid: i64, span: Span) -> Result<u32, RuntimeError> {
    if !(0..=u32::MAX as i64).contains(&uid) {
        return Err(RuntimeError::new("uid-range", "uid is out of range").with_span(span));
    }
    Ok(uid as u32)
}

pub(crate) fn name_for_uid(uid: u32) -> Option<String> {
    let mut passwd: libc::passwd = unsafe { std::mem::zeroed() };
    let mut result = std::ptr::null_mut();
    let mut buffer = vec![0u8; passwd_buffer_size()];
    let rc = unsafe {
        libc::getpwuid_r(
            uid as libc::uid_t,
            &mut passwd,
            buffer.as_mut_ptr().cast::<libc::c_char>(),
            buffer.len(),
            &mut result,
        )
    };
    if rc == 0 && !result.is_null() {
        Some(cstr_to_string(passwd.pw_name))
    } else {
        None
    }
}

fn passwd_result(
    rc: libc::c_int,
    result: *mut libc::passwd,
    passwd: libc::passwd,
    span: Span,
) -> Result<Value, RuntimeError> {
    if rc != 0 {
        return Err(RuntimeError::new(
            "user-lookup",
            std::io::Error::from_raw_os_error(rc).to_string(),
        )
        .with_span(span));
    }
    if result.is_null() {
        return Err(RuntimeError::new("user-not-found", "user was not found").with_span(span));
    }
    user_record(&passwd, span)
}

fn user_record(passwd: &libc::passwd, span: Span) -> Result<Value, RuntimeError> {
    let home =
        PathValue::new(cstr_to_bytes(passwd.pw_dir)).map_err(|error| error.with_span(span))?;
    Ok(Value::Record(crate::runtime::value::RecordMap::from([
        (
            Arc::from("name"),
            Value::Str(cstr_to_string(passwd.pw_name).into()),
        ),
        (Arc::from("uid"), Value::Int(passwd.pw_uid as i64)),
        (Arc::from("gid"), Value::Int(passwd.pw_gid as i64)),
        (Arc::from("home"), Value::Path(home)),
        (
            Arc::from("shell"),
            Value::Str(cstr_to_string(passwd.pw_shell).into()),
        ),
    ])))
}

fn passwd_buffer_size() -> usize {
    let size = unsafe { libc::sysconf(libc::_SC_GETPW_R_SIZE_MAX) };
    if size > 0 { size as usize } else { 16_384 }
}

fn cstr_to_string(ptr: *const libc::c_char) -> String {
    if ptr.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned()
    }
}

fn cstr_to_bytes(ptr: *const libc::c_char) -> Vec<u8> {
    if ptr.is_null() {
        Vec::new()
    } else {
        unsafe { CStr::from_ptr(ptr) }.to_bytes().to_vec()
    }
}

pub(crate) struct AccountField<'a>(pub(crate) &'a str);

impl<'a> AccountField<'a> {
    pub(crate) fn new(value: &'a str, kind: &str, span: Span) -> Result<Self, RuntimeError> {
        if value.contains([':', '\n', '\r', '\0']) {
            return Err(RuntimeError::new(kind, "invalid account field").with_span(span));
        }
        Ok(Self(value))
    }
}

pub(crate) fn account_id(id: i64, kind: &str, span: Span) -> Result<(), RuntimeError> {
    if !(0..u32::MAX as i64).contains(&id) {
        return Err(RuntimeError::new(kind, "account id is out of range").with_span(span));
    }
    Ok(())
}

struct PasswdEntry<'a> {
    name: &'a str,
    uid: i64,
    home: &'a str,
}

fn parse_passwd<'a>(text: &'a str, span: Span, kind: &str) -> Result<Vec<PasswdEntry<'a>>, RuntimeError> {
    text.split_terminator('\n').map(|line| {
        let fields = line.split(':').collect::<Vec<_>>();
        if fields.len() != 7 || fields.iter().any(|field| AccountField::new(field, kind, span).is_err()) {
            return Err(RuntimeError::new(kind, "invalid passwd record").with_span(span));
        }
        let uid = fields[2].parse::<u32>().map_err(|_| RuntimeError::new(kind, "invalid passwd uid").with_span(span))?;
        let gid = fields[3].parse::<u32>().map_err(|_| RuntimeError::new(kind, "invalid passwd gid").with_span(span))?;
        account_id(i64::from(uid), kind, span)?;
        account_id(i64::from(gid), kind, span)?;
        Ok(PasswdEntry { name: fields[0], uid: i64::from(uid), home: fields[5] })
    }).collect()
}

fn validate_name(name: &str, kind: &str, span: Span) -> Result<(), RuntimeError> {
    if name.is_empty()
        || name.contains(':')
        || name.contains('\n')
        || name.contains('\r')
        || name.contains('\0')
        || name.starts_with('-')
    {
        return Err(RuntimeError::new(kind, "invalid name").with_span(span));
    }
    Ok(())
}

fn next_id(ids: impl Iterator<Item = i64>) -> i64 {
    ids.max().unwrap_or(999).saturating_add(1).max(1000)
}

fn passwd_file() -> PathBuf {
    std::env::var_os("XSH_PASSWD_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/etc/passwd"))
}

fn shadow_file() -> PathBuf {
    std::env::var_os("XSH_SHADOW_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/etc/shadow"))
}

fn read_optional(path: &Path, span: Span, kind: &str) -> Result<String, RuntimeError> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(RuntimeError::host(kind, &error).with_span(span)),
    }
}

pub(crate) fn account_write_error(error: RuntimeError, kind: &str) -> RuntimeError {
    let mut result = RuntimeError::new(kind, error.message.clone());
    result.payload = error.payload.clone();
    result.facets = error.facets.clone();
    result.span = error.span;
    result
}
