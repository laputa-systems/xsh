#![allow(clippy::single_call_fn)]

use crate::runtime::value::{RuntimeError, Value};
use crate::source::Span;
use std::ffi::{CStr, CString};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub(crate) fn current(span: Span) -> Result<Value, RuntimeError> {
    by_gid(rustix::process::getgid().as_raw(), span)
}

pub(crate) fn lookup(name: &str, span: Span) -> Result<Value, RuntimeError> {
    let name = CString::new(name).map_err(|_| {
        RuntimeError::new("group-name", "group name cannot contain NUL").with_span(span)
    })?;
    let mut group: libc::group = unsafe { std::mem::zeroed() };
    let mut result = std::ptr::null_mut();
    let mut buffer = vec![0u8; group_buffer_size()];
    let rc = unsafe {
        libc::getgrnam_r(
            name.as_ptr(),
            &mut group,
            buffer.as_mut_ptr().cast::<libc::c_char>(),
            buffer.len(),
            &mut result,
        )
    };
    group_result(rc, result, group, span)
}

pub(crate) fn by_gid(gid: u32, span: Span) -> Result<Value, RuntimeError> {
    let mut group: libc::group = unsafe { std::mem::zeroed() };
    let mut result = std::ptr::null_mut();
    let mut buffer = vec![0u8; group_buffer_size()];
    let rc = unsafe {
        libc::getgrgid_r(
            gid as libc::gid_t,
            &mut group,
            buffer.as_mut_ptr().cast::<libc::c_char>(),
            buffer.len(),
            &mut result,
        )
    };
    group_result(rc, result, group, span)
}

pub(crate) fn add(name: &str, gid: Option<i64>, span: Span) -> Result<Value, RuntimeError> {
    validate_name(name, "group-name", span)?;
    let path = group_file();
    let mut groups = parse_group(&read_optional(&path, span, "group-add")?);
    if groups.iter().any(|entry| entry.name == name) {
        return Err(RuntimeError::new("group-add", "group already exists").with_span(span));
    }
    let gid = gid.unwrap_or_else(|| next_id(groups.iter().map(|entry| entry.gid)));
    groups.push(GroupEntry {
        name: name.to_string(),
        gid,
        members: Vec::new(),
    });
    write_atomic(&path, &render_group(&groups), span, "group-add")?;
    Ok(Value::Record(crate::runtime::value::RecordMap::from([
        (Arc::from("name"), Value::Str(name.into())),
        (Arc::from("gid"), Value::Int(gid)),
        (Arc::from("members"), Value::List(Vec::new())),
    ])))
}

pub(crate) fn remove(name: &str, span: Span) -> Result<Value, RuntimeError> {
    validate_name(name, "group-name", span)?;
    let path = group_file();
    let mut groups = parse_group(&read_optional(&path, span, "group-remove")?);
    if !groups.iter().any(|entry| entry.name == name) {
        return Err(RuntimeError::new("group-remove", "group was not found").with_span(span));
    }
    groups.retain(|entry| entry.name != name);
    write_atomic(&path, &render_group(&groups), span, "group-remove")?;
    Ok(Value::Unit)
}

pub(crate) fn gid_from_i64(gid: i64, span: Span) -> Result<u32, RuntimeError> {
    if !(0..=u32::MAX as i64).contains(&gid) {
        return Err(RuntimeError::new("gid-range", "gid is out of range").with_span(span));
    }
    Ok(gid as u32)
}

fn group_result(
    rc: libc::c_int,
    result: *mut libc::group,
    group: libc::group,
    span: Span,
) -> Result<Value, RuntimeError> {
    if rc != 0 {
        return Err(RuntimeError::new(
            "group-lookup",
            std::io::Error::from_raw_os_error(rc).to_string(),
        )
        .with_span(span));
    }
    if result.is_null() {
        return Err(RuntimeError::new("group-not-found", "group was not found").with_span(span));
    }
    Ok(group_record(&group))
}

fn group_record(group: &libc::group) -> Value {
    Value::Record(crate::runtime::value::RecordMap::from([
        (
            Arc::from("name"),
            Value::Str(cstr_to_string(group.gr_name).into()),
        ),
        (Arc::from("gid"), Value::Int(group.gr_gid as i64)),
        (
            Arc::from("members"),
            Value::List(group_members(group.gr_mem)),
        ),
    ]))
}

fn group_members(mut members: *mut *mut libc::c_char) -> Vec<Value> {
    let mut values = Vec::new();
    if members.is_null() {
        return values;
    }
    unsafe {
        while !(*members).is_null() {
            values.push(Value::Str(cstr_to_string(*members).into()));
            members = members.add(1);
        }
    }
    values
}

fn group_buffer_size() -> usize {
    let size = unsafe { libc::sysconf(libc::_SC_GETGR_R_SIZE_MAX) };
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

#[derive(Clone)]
struct GroupEntry {
    name: String,
    gid: i64,
    members: Vec<String>,
}

fn parse_group(text: &str) -> Vec<GroupEntry> {
    text.lines()
        .filter_map(|line| {
            let fields = line.split(':').collect::<Vec<_>>();
            (fields.len() >= 4).then(|| GroupEntry {
                name: fields[0].to_string(),
                gid: fields[2].parse().unwrap_or(0),
                members: fields[3]
                    .split(',')
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
                    .collect(),
            })
        })
        .collect()
}

fn render_group(entries: &[GroupEntry]) -> String {
    entries
        .iter()
        .map(|entry| format!("{}:x:{}:{}", entry.name, entry.gid, entry.members.join(",")))
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

fn group_file() -> PathBuf {
    std::env::var_os("XSH_GROUP_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/etc/group"))
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
