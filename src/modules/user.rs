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
    let passwd_path = passwd_file();
    let shadow_path = shadow_file();
    let mut users = parse_passwd(&read_optional(&passwd_path, span, "user-add")?);
    if users.iter().any(|entry| entry.name == name) {
        return Err(RuntimeError::new("user-add", "user already exists").with_span(span));
    }
    let uid = uid.unwrap_or_else(|| next_id(users.iter().map(|entry| entry.uid)));
    let gid = gid.unwrap_or(uid);
    let home_path = home
        .map(|value| value.display())
        .unwrap_or_else(|| format!("/home/{name}"));
    let shell_path = shell
        .map(|value| value.display())
        .unwrap_or_else(|| "/bin/sh".to_string());
    users.push(PasswdEntry {
        name: name.to_string(),
        uid,
        gid,
        gecos: gecos.to_string(),
        home: home_path.clone(),
        shell: shell_path.clone(),
    });
    write_atomic(&passwd_path, &render_passwd(&users), span, "user-add")?;
    let shadow_text = read_optional(&shadow_path, span, "user-add")?;
    if !shadow_text.is_empty() {
        let mut lines = shadow_text.lines().map(str::to_string).collect::<Vec<_>>();
        if !lines
            .iter()
            .any(|line| line.split(':').next() == Some(name))
        {
            lines.push(format!("{name}:!:0:0:99999:7:::"));
            write_atomic(&shadow_path, &(lines.join("\n") + "\n"), span, "user-add")?;
        }
    }
    Ok(Value::Record(crate::runtime::value::RecordMap::from([
        (Arc::from("name"), Value::Str(name.into())),
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
    let mut users = parse_passwd(&users_text);
    let Some(entry) = users.iter().find(|entry| entry.name == name).cloned() else {
        return Err(RuntimeError::new("user-remove", "user was not found").with_span(span));
    };
    users.retain(|entry| entry.name != name);
    write_atomic(&passwd_path, &render_passwd(&users), span, "user-remove")?;
    let shadow_text = read_optional(&shadow_path, span, "user-remove")?;
    if !shadow_text.is_empty() {
        let lines = shadow_text
            .lines()
            .filter(|line| line.split(':').next() != Some(name))
            .collect::<Vec<_>>();
        write_atomic(
            &shadow_path,
            &(lines.join("\n") + "\n"),
            span,
            "user-remove",
        )?;
    }
    if remove_home {
        let _ = fs::remove_dir_all(entry.home);
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

#[derive(Clone)]
struct PasswdEntry {
    name: String,
    uid: i64,
    gid: i64,
    gecos: String,
    home: String,
    shell: String,
}

fn parse_passwd(text: &str) -> Vec<PasswdEntry> {
    text.lines()
        .filter_map(|line| {
            let fields = line.split(':').collect::<Vec<_>>();
            (fields.len() >= 7).then(|| PasswdEntry {
                name: fields[0].to_string(),
                uid: fields[2].parse().unwrap_or(0),
                gid: fields[3].parse().unwrap_or(0),
                gecos: fields[4].to_string(),
                home: fields[5].to_string(),
                shell: fields[6].to_string(),
            })
        })
        .collect()
}

fn render_passwd(entries: &[PasswdEntry]) -> String {
    entries
        .iter()
        .map(|entry| {
            format!(
                "{}:x:{}:{}:{}:{}:{}",
                entry.name, entry.uid, entry.gid, entry.gecos, entry.home, entry.shell
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

fn validate_name(name: &str, kind: &str, span: Span) -> Result<(), RuntimeError> {
    if name.is_empty()
        || name.contains(':')
        || name.contains('\n')
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
        Err(error) => Err(RuntimeError::new(kind, error.to_string()).with_span(span)),
    }
}

fn write_atomic(path: &Path, text: &str, span: Span, kind: &str) -> Result<(), RuntimeError> {
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    fs::write(&tmp, text)
        .map_err(|error| RuntimeError::new(kind, error.to_string()).with_span(span))?;
    fs::rename(&tmp, path)
        .map_err(|error| RuntimeError::new(kind, error.to_string()).with_span(span))
}
