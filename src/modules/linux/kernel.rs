#![allow(clippy::single_call_fn)]

use super::block::path_value;
use super::{MODULE_EXTENSIONS, ModuleEntry, ModuleIndex, ModuleMetadata, str_value};
use crate::modules::compression::linux_module_reader;
use crate::runtime::value::{RuntimeError, Value};
use crate::source::Span;
use rustc_hash::FxHashSet;
use std::fs;
#[cfg(target_os = "linux")]
use std::fs::File;
use std::io::{self, Read};
#[cfg(target_os = "linux")]
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub(super) fn modinfo_impl(name: &str, span: Span) -> Result<Value, RuntimeError> {
    let root = module_tree_dir("")
        .map_err(|error| RuntimeError::new("linux-modinfo", error.to_string()).with_span(span))?;
    modinfo_impl_in_root(name, &root, span)
}

pub(super) fn modinfo_impl_in_root(
    name: &str,
    root: &Path,
    span: Span,
) -> Result<Value, RuntimeError> {
    let index = ModuleIndex::scan(root)
        .map_err(|error| RuntimeError::new("linux-modinfo", error.to_string()).with_span(span))?;
    let entry = if Path::new(name).exists() {
        let path = PathBuf::from(name);
        ModuleEntry {
            name: module_name_from_path(name),
            relative_path: path.to_string_lossy().into_owned(),
            metadata: read_module_metadata(&path).map_err(|error| {
                RuntimeError::new("linux-modinfo", error.to_string()).with_span(span)
            })?,
            path,
        }
    } else {
        index
            .get(name)
            .cloned()
            .ok_or_else(|| RuntimeError::new("linux-modinfo", "module not found").with_span(span))?
    };
    module_info_record(&entry, span)
}

pub(super) fn modprobe_impl(name: &str, params: &str, span: Span) -> Result<(), RuntimeError> {
    let root = module_tree_dir("")
        .map_err(|error| RuntimeError::new("linux-modprobe", error.to_string()).with_span(span))?;
    let index = ModuleIndex::scan(&root)
        .map_err(|error| RuntimeError::new("linux-modprobe", error.to_string()).with_span(span))?;
    let entry = index
        .get(name)
        .ok_or_else(|| RuntimeError::new("linux-modprobe", "module not found").with_span(span))?;
    let mut order = Vec::new();
    let mut seen = FxHashSet::default();
    dependency_order(&index, entry, &mut seen, &mut order);
    for module in order {
        let module_params = if module.name == entry.name {
            params
        } else {
            ""
        };
        insmod_path(&module.path, module_params, span)?;
    }
    Ok(())
}

pub(super) fn depmod_impl(version: &str, span: Span) -> Result<(), RuntimeError> {
    let root = module_tree_dir(version)
        .map_err(|error| RuntimeError::new("linux-depmod", error.to_string()).with_span(span))?;
    depmod_impl_in_root(&root, span)
}

pub(super) fn depmod_impl_in_root(root: &Path, span: Span) -> Result<(), RuntimeError> {
    let index = ModuleIndex::scan(root)
        .map_err(|error| RuntimeError::new("linux-depmod", error.to_string()).with_span(span))?;
    let mut lines = index
        .entries
        .iter()
        .map(|entry| {
            let deps = entry
                .metadata
                .depends()
                .into_iter()
                .filter_map(|dep| index.get(&dep).map(|entry| entry.relative_path.clone()))
                .collect::<Vec<_>>()
                .join(" ");
            if deps.is_empty() {
                format!("{}:\n", entry.relative_path)
            } else {
                format!("{}: {deps}\n", entry.relative_path)
            }
        })
        .collect::<Vec<_>>();
    lines.sort_unstable();
    fs::write(root.join("modules.dep"), lines.concat())
        .map_err(|error| RuntimeError::new("linux-depmod", error.to_string()).with_span(span))
}

impl ModuleMetadata {
    fn parse(bytes: &[u8]) -> Self {
        let fields = bytes
            .split(|byte| *byte == 0)
            .filter_map(|chunk| {
                if chunk.is_empty() || !chunk.is_ascii() {
                    return None;
                }
                let text = std::str::from_utf8(chunk).ok()?;
                let (key, value) = text.split_once('=')?;
                (!key.is_empty()
                    && key
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_'))
                .then(|| (key.to_string(), value.to_string()))
            })
            .collect();
        Self { fields }
    }

    fn values<'a>(&'a self, key: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        self.fields
            .iter()
            .filter_map(move |(field, value)| (field == key).then_some(value.as_str()))
    }

    fn first(&self, key: &str) -> String {
        self.values(key).next().unwrap_or("").to_string()
    }

    fn depends(&self) -> Vec<String> {
        self.values("depends")
            .flat_map(|value| value.split(','))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(normalize_module_name)
            .collect()
    }
}

impl ModuleIndex {
    pub(super) fn scan(root: &Path) -> io::Result<Self> {
        let mut entries = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(path) = stack.pop() {
            let read_dir = match fs::read_dir(&path) {
                Ok(read_dir) => read_dir,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error),
            };
            for entry in read_dir {
                let entry = entry?;
                let path = entry.path();
                if entry.metadata()?.is_dir() {
                    stack.push(path);
                    continue;
                }
                if !is_module_path(&path) {
                    continue;
                }
                let relative_path = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                entries.push(ModuleEntry {
                    name: module_name_from_path(&relative_path),
                    metadata: read_module_metadata(&path)?,
                    relative_path,
                    path,
                });
            }
        }
        entries.sort_unstable_by(|left, right| left.relative_path.cmp(&right.relative_path));
        let by_name = entries
            .iter()
            .enumerate()
            .map(|(index, entry)| (entry.name.clone(), index))
            .collect();
        Ok(Self { entries, by_name })
    }

    pub(super) fn get(&self, name: &str) -> Option<&ModuleEntry> {
        self.by_name
            .get(&normalize_module_name(name))
            .and_then(|index| self.entries.get(*index))
    }
}

pub(super) fn module_info_record(entry: &ModuleEntry, span: Span) -> Result<Value, RuntimeError> {
    let params = entry
        .metadata
        .values("parm")
        .map(|value| {
            let (name, rest) = value.split_once(':').unwrap_or((value, ""));
            let (description, kind) = rest
                .rsplit_once('(')
                .map(|(description, kind)| {
                    (
                        description.trim().to_string(),
                        kind.trim_end_matches(')').to_string(),
                    )
                })
                .unwrap_or_else(|| (rest.to_string(), String::new()));
            Value::Record(crate::runtime::value::RecordMap::from([
                (Arc::from("name"), str_value(name.to_string())),
                (Arc::from("type"), str_value(kind)),
                (Arc::from("description"), str_value(description)),
            ]))
        })
        .collect();
    Ok(Value::Record(crate::runtime::value::RecordMap::from([
        (Arc::from("name"), str_value(entry.name.clone())),
        (
            Arc::from("filename"),
            Value::Path(path_value(&entry.path, span)?),
        ),
        (
            Arc::from("description"),
            str_value(entry.metadata.first("description")),
        ),
        (
            Arc::from("license"),
            str_value(entry.metadata.first("license")),
        ),
        (
            Arc::from("version"),
            str_value(entry.metadata.first("version")),
        ),
        (Arc::from("params"), Value::List(params)),
    ])))
}

fn dependency_order<'a>(
    index: &'a ModuleIndex,
    entry: &'a ModuleEntry,
    seen: &mut FxHashSet<String>,
    order: &mut Vec<&'a ModuleEntry>,
) {
    if !seen.insert(entry.name.clone()) {
        return;
    }
    for dep in entry.metadata.depends() {
        if let Some(dep) = index.get(&dep) {
            dependency_order(index, dep, seen, order);
        }
    }
    order.push(entry);
}

fn insmod_path(path: &Path, params: &str, span: Span) -> Result<(), RuntimeError> {
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (path, params);
        Err(RuntimeError::new("linux-modprobe", "module insertion requires Linux").with_span(span))
    }

    #[cfg(target_os = "linux")]
    {
        let params = std::ffi::CString::new(params).map_err(|_| {
            RuntimeError::new("linux-modprobe", "params contain NUL").with_span(span)
        })?;
        let file = File::open(path).map_err(|error| {
            RuntimeError::new("linux-modprobe", error.to_string()).with_span(span)
        })?;
        let rc =
            unsafe { libc::syscall(libc::SYS_finit_module, file.as_raw_fd(), params.as_ptr(), 0) };
        if rc == 0 {
            Ok(())
        } else {
            Err(
                RuntimeError::new("linux-modprobe", io::Error::last_os_error().to_string())
                    .with_span(span),
            )
        }
    }
}

fn module_tree_dir(version: &str) -> io::Result<PathBuf> {
    if let Some(path) = std::env::var_os("XSH_MODULES_DIR") {
        return Ok(PathBuf::from(path));
    }
    let release = if version.is_empty() {
        rustix::system::uname().release().to_bytes().to_vec()
    } else {
        version.as_bytes().to_vec()
    };
    Ok(PathBuf::from("/lib/modules").join(String::from_utf8_lossy(&release).as_ref()))
}

fn read_module_metadata(path: &Path) -> io::Result<ModuleMetadata> {
    read_module_file(path).map(|bytes| ModuleMetadata::parse(&bytes))
}

fn read_module_file(path: &Path) -> io::Result<Vec<u8>> {
    let mut reader = linux_module_reader(path)?;
    let mut data = Vec::new();
    reader.read_to_end(&mut data)?;
    Ok(data)
}

fn is_module_path(path: &Path) -> bool {
    let path = path.to_string_lossy();
    MODULE_EXTENSIONS
        .iter()
        .any(|suffix| path.ends_with(suffix))
}

fn normalize_module_name(name: &str) -> String {
    let mut name = name;
    for suffix in MODULE_EXTENSIONS {
        if let Some(stripped) = name.strip_suffix(suffix) {
            name = stripped;
            break;
        }
    }
    Path::new(name)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(name)
        .replace('-', "_")
}

fn module_name_from_path(path: &str) -> String {
    normalize_module_name(path)
}
