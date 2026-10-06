#![allow(clippy::single_call_fn)]

use super::block::path_value;
use super::{MODULE_EXTENSIONS, ModuleEntry, ModuleIndex, ModuleMetadata, str_value};
use crate::modules::compression::linux_module_reader;
use crate::runtime::value::{RuntimeError, Value};
use crate::source::Span;
use rustc_hash::{FxHashMap, FxHashSet};
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
        .map_err(|error| RuntimeError::host("linux-modinfo", &error).with_span(span))?;
    modinfo_impl_in_root(name, &root, span)
}

pub(super) fn modinfo_impl_in_root(
    name: &str,
    root: &Path,
    span: Span,
) -> Result<Value, RuntimeError> {
    let entry = if Path::new(name).exists() {
        let path = PathBuf::from(name);
        ModuleEntry {
            name: module_name_from_path(name),
            relative_path: path.to_string_lossy().into_owned(),
            metadata: read_module_metadata(&path)
                .map_err(|error| RuntimeError::host("linux-modinfo", &error).with_span(span))?,
            path,
        }
    } else {
        let index = ModuleIndex::scan(root)
            .map_err(|error| RuntimeError::host("linux-modinfo", &error).with_span(span))?;
        index
            .get(name)
            .cloned()
            .ok_or_else(|| RuntimeError::new("linux-modinfo", "module not found").with_span(span))?
    };
    module_info_record(&entry, span)
}

pub(super) fn modprobe_impl_with(name: &str, params: &str, remove: bool, span: Span) -> Result<(), RuntimeError> {
    if params.contains('\0') {
        return Err(RuntimeError::new("linux-modprobe", "params contain NUL").with_span(span));
    }
    let loaded = loaded_modules().map_err(|error| RuntimeError::host("linux-modprobe", &error).with_span(span))?;
    let root = module_tree_dir("").map_err(|error| RuntimeError::host("linux-modprobe", &error).with_span(span))?;
    let config = ModuleConfig::load(&root).map_err(|error| RuntimeError::host("linux-modprobe", &error).with_span(span))?;
    if !remove && loaded.contains(&normalize_module_name(name)) {
        if config.install.contains(&normalize_module_name(name)) {
            return Err(RuntimeError::new("linux-modprobe", "loaded module has an unsupported install command directive").with_span(span));
        }
        return Ok(());
    }
    let index = ModuleIndex::scan(&root).map_err(|error| RuntimeError::host("linux-modprobe", &error).with_span(span))?;
    let plan = build_module_plan(&index, &config, name, params, remove, span)?;
    for module in &plan {
        if remove {
            if !loaded.contains(&module.entry.name) {
                continue;
            }
            // Shared dependencies remain loaded. The first requested module
            // still reaches delete_module so a busy target reports its errno.
            if !module.requested && module_ref_count(&module.entry.name).map_err(|error| RuntimeError::host("linux-modprobe", &error).with_span(span))? != 0 {
                continue;
            }
            remove_module(&module.entry.name, span)?;
        } else if !loaded.contains(&module.entry.name) {
            insmod_path(&module.entry.path, &module.params, span)?;
        }
    }
    Ok(())
}

pub(super) fn module_plan_impl(name: &str, params: &str, remove: bool, span: Span) -> Result<Value, RuntimeError> {
    let root = module_tree_dir("").map_err(|error| RuntimeError::host("linux-module-plan", &error).with_span(span))?;
    let index = ModuleIndex::scan(&root).map_err(|error| RuntimeError::host("linux-module-plan", &error).with_span(span))?;
    let config = ModuleConfig::load(&root).map_err(|error| RuntimeError::host("linux-module-plan", &error).with_span(span))?;
    let loaded = loaded_modules().map_err(|error| RuntimeError::host("linux-module-plan", &error).with_span(span))?;
    let plan = build_module_plan(&index, &config, name, params, remove, span)?;
    let records = plan.into_iter().map(|module| {
        Ok(Value::Record(crate::runtime::value::RecordMap::from([
            (Arc::from("name"), str_value(module.entry.name.clone())),
            (Arc::from("filename"), Value::Path(path_value(&module.entry.path, span)?)),
            (Arc::from("params"), str_value(module.params)),
            (Arc::from("loaded"), Value::Bool(loaded.contains(&module.entry.name))),
        ])))
    }).collect::<Result<Vec<_>, RuntimeError>>()?;
    Ok(Value::List(records))
}

pub(super) fn depmod_impl(version: &str, span: Span) -> Result<(), RuntimeError> {
    let root = module_tree_dir(version)
        .map_err(|error| RuntimeError::host("linux-depmod", &error).with_span(span))?;
    depmod_impl_in_root(&root, span)
}

pub(super) fn depmod_impl_in_root(root: &Path, span: Span) -> Result<(), RuntimeError> {
    let index = ModuleIndex::scan(root)
        .map_err(|error| RuntimeError::host("linux-depmod", &error).with_span(span))?;
    let builtins = builtin_modules(root)
        .map_err(|error| RuntimeError::host("linux-depmod", &error).with_span(span))?;
    let mut lines = Vec::new();
    let mut aliases = Vec::new();
    let mut softdeps = Vec::new();
    for entry in &index.entries {
        let mut order = Vec::new();
        dependency_order(&index, entry, &builtins, &mut FxHashSet::default(), &mut FxHashSet::default(), &mut order)
            .map_err(|message| RuntimeError::new("linux-depmod", message).with_span(span))?;
        let deps = order.into_iter().filter(|module| module.name != entry.name)
            .rev().map(|module| module.relative_path.clone()).collect::<Vec<_>>().join(" ");
        lines.push(if deps.is_empty() { format!("{}:\n", entry.relative_path) } else { format!("{}: {deps}\n", entry.relative_path) });
        for alias in entry.metadata.values("alias") {
            aliases.push(format!("alias {alias} {}\n", entry.name));
        }
        for softdep in entry.metadata.values("softdep") {
            parse_softdep(softdep).map_err(|error| RuntimeError::host("linux-depmod", &error).with_span(span))?;
            softdeps.push(format!("softdep {} {softdep}\n", entry.name));
        }
    }
    lines.sort_unstable();
    aliases.sort_unstable();
    softdeps.sort_unstable();
    // Validate the complete tree before replacing any output. Each index is
    // replaced atomically; a write failure is reported rather than partial
    // content being advertised as a usable index.
    for (name, contents) in [("modules.dep", lines.concat()), ("modules.alias", aliases.concat()), ("modules.softdep", softdeps.concat())] {
        let mut temporary = tempfile::NamedTempFile::new_in(root)
            .map_err(|error| RuntimeError::host("linux-depmod", &error).with_span(span))?;
        std::io::Write::write_all(&mut temporary, contents.as_bytes())
            .map_err(|error| RuntimeError::host("linux-depmod", &error).with_span(span))?;
        use std::os::unix::fs::PermissionsExt;
        temporary.as_file().set_permissions(fs::Permissions::from_mode(0o644))
            .map_err(|error| RuntimeError::host("linux-depmod", &error).with_span(span))?;
        temporary.persist(root.join(name)).map_err(|error| RuntimeError::host("linux-depmod", &error.error).with_span(span))?;
    }
    Ok(())
}

impl ModuleMetadata {
    fn parse(bytes: &[u8]) -> io::Result<Self> {
        let fields = bytes
            .split(|byte| *byte == 0)
            .filter(|chunk| !chunk.is_empty())
            .map(|chunk| {
                let text = std::str::from_utf8(chunk).map_err(|_| invalid_module("modinfo field is not UTF-8"))?;
                let (key, value) = text.split_once('=').ok_or_else(|| invalid_module("modinfo field has no value"))?;
                if key.is_empty() || !key.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_') {
                    return Err(invalid_module("modinfo field name is invalid"));
                }
                Ok((key.to_string(), value.to_string()))
            })
            .collect::<io::Result<Vec<_>>>()?;
        Ok(Self { fields })
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
            let read_dir = fs::read_dir(&path)?;
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
            let kind = entry.metadata.values("parmtype")
                .find_map(|value| value.split_once(':').filter(|(field, _)| *field == name).map(|(_, kind)| kind.to_string()))
                .unwrap_or(kind);
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
        (Arc::from("fields"), Value::List(entry.metadata.fields.iter().map(|(name, value)| {
            Value::Record(crate::runtime::value::RecordMap::from([
                (Arc::from("name"), str_value(name.clone())),
                (Arc::from("value"), str_value(value.clone())),
            ]))
        }).collect())),
    ])))
}

fn dependency_order<'a>(
    index: &'a ModuleIndex,
    entry: &'a ModuleEntry,
    builtins: &FxHashSet<String>,
    active: &mut FxHashSet<String>,
    seen: &mut FxHashSet<String>,
    order: &mut Vec<&'a ModuleEntry>,
) -> Result<(), String> {
    if seen.contains(&entry.name) {
        return Ok(());
    }
    if !active.insert(entry.name.clone()) {
        return Err(format!("dependency cycle at module `{}`", entry.name));
    }
    for name in entry.metadata.depends() {
        if builtins.contains(&name) {
            continue;
        }
        let dep = index.get(&name).ok_or_else(|| format!("module `{}` depends on missing module `{name}`", entry.name))?;
        dependency_order(index, dep, builtins, active, seen, order)?;
    }
    active.remove(&entry.name);
    seen.insert(entry.name.clone());
    order.push(entry);
    Ok(())
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
        if path.extension().is_some_and(|extension| extension == "ko") {
            let file = File::open(path)
                .map_err(|error| RuntimeError::host("linux-modprobe", &error).with_span(span))?;
            // The descriptor pins the uncompressed image during insertion.
            let rc = unsafe { libc::syscall(libc::SYS_finit_module, file.as_raw_fd(), params.as_ptr(), 0) };
            if rc == 0 { return Ok(()); }
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::EEXIST) { return Ok(()); }
            if !matches!(error.raw_os_error(), Some(libc::ENOSYS | libc::EINVAL)) {
                return Err(RuntimeError::host("linux-modprobe", &error).with_span(span));
            }
        }
        let image = read_module_file(path)
            .map_err(|error| RuntimeError::host("linux-modprobe", &error).with_span(span))?;
        // The decompression adapter produces the image consumed by the kernel;
        // compression support does not depend on kernel build-time codecs.
        let rc = unsafe { libc::syscall(libc::SYS_init_module, image.as_ptr(), image.len(), params.as_ptr()) };
        if rc == 0 { return Ok(()); }
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::EEXIST) { return Ok(()); }
        Err(RuntimeError::host("linux-modprobe", &error).with_span(span))
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
    let bytes = read_module_file(path)?;
    ModuleMetadata::parse(elf_modinfo(&bytes)?)
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


fn invalid_module(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

/// Only the named ELF section is metadata; strings in code, debug sections,
/// and appended signatures must never become dependency or alias declarations.
fn elf_modinfo(bytes: &[u8]) -> io::Result<&[u8]> {
    if !bytes.starts_with(b"\x7fELF") || bytes.get(6) != Some(&1) {
        return Err(invalid_module("module is not an ELF image"));
    }
    let little = match bytes.get(5) {
        Some(1) => true,
        Some(2) => false,
        _ => return Err(invalid_module("invalid ELF byte order")),
    };
    let read = |offset: usize, width: usize| -> io::Result<usize> {
        let end = offset.checked_add(width).ok_or_else(|| invalid_module("ELF offset overflow"))?;
        let slice = bytes.get(offset..end).ok_or_else(|| invalid_module("truncated ELF image"))?;
        let value = if little {
            slice.iter().rev().fold(0u64, |value, byte| (value << 8) | u64::from(*byte))
        } else {
            slice.iter().fold(0u64, |value, byte| (value << 8) | u64::from(*byte))
        };
        usize::try_from(value).map_err(|_| invalid_module("ELF offset is outside the host range"))
    };
    let (table, stride, mut count, mut names_index, offset_field, size_field, link_field, width, minimum) = match bytes.get(4) {
        Some(1) => (read(32, 4)?, read(46, 2)?, read(48, 2)?, read(50, 2)?, 16, 20, 24, 4, 40),
        Some(2) => (read(40, 8)?, read(58, 2)?, read(60, 2)?, read(62, 2)?, 24, 32, 40, 8, 64),
        _ => return Err(invalid_module("invalid ELF class")),
    };
    if table == 0 {
        return Err(invalid_module("module has no ELF section table"));
    }
    if stride < minimum {
        return Err(invalid_module("ELF section header is too small"));
    }
    if count == 0 {
        count = read(table.checked_add(size_field).ok_or_else(|| invalid_module("ELF offset overflow"))?, width)?;
    }
    if names_index == 0xffff {
        names_index = read(table.checked_add(link_field).ok_or_else(|| invalid_module("ELF offset overflow"))?, 4)?;
    }
    let table_size = stride.checked_mul(count).and_then(|size| table.checked_add(size))
        .ok_or_else(|| invalid_module("ELF section table overflow"))?;
    if table_size > bytes.len() || names_index >= count {
        return Err(invalid_module("invalid ELF section table"));
    }
    let section = |index: usize| -> io::Result<&[u8]> {
        let header = table + index * stride;
        let offset = read(header + offset_field, width)?;
        let size = read(header + size_field, width)?;
        let end = offset.checked_add(size).ok_or_else(|| invalid_module("ELF section overflow"))?;
        bytes.get(offset..end).ok_or_else(|| invalid_module("truncated ELF section"))
    };
    let names = section(names_index)?;
    for index in 0..count {
        let name_offset = read(table + index * stride, 4)?;
        let name = names.get(name_offset..).ok_or_else(|| invalid_module("invalid ELF section name"))?;
        let end = name.iter().position(|byte| *byte == 0).ok_or_else(|| invalid_module("unterminated ELF section name"))?;
        if &name[..end] == b".modinfo" {
            return section(index);
        }
    }
    Err(invalid_module("module has no .modinfo section"))
}

#[derive(Default)]
struct ModuleConfig {
    aliases: Vec<(String, String)>,
    options: FxHashMap<String, String>,
    softdeps: FxHashMap<String, (Vec<String>, Vec<String>)>,
    blacklist: FxHashSet<String>,
    install: FxHashSet<String>,
    remove: FxHashSet<String>,
    builtins: FxHashSet<String>,
}

impl ModuleConfig {
    fn load(root: &Path) -> io::Result<Self> {
        let mut config = Self { builtins: builtin_modules(root)?, ..Self::default() };
        let mut selected = FxHashMap::default();
        for directory in ["/etc/modprobe.d", "/run/modprobe.d", "/usr/local/lib/modprobe.d", "/usr/lib/modprobe.d", "/lib/modprobe.d"] {
            let files = match fs::read_dir(directory) {
                Ok(files) => files,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error),
            };
            for file in files {
                let file = file?;
                if file.path().extension().is_some_and(|extension| extension == "conf") {
                    selected.entry(file.file_name()).or_insert_with(|| file.path());
                }
            }
        }
        let mut files = selected.into_iter().collect::<Vec<_>>();
        files.sort_by(|left, right| left.0.cmp(&right.0));
        for (_, file) in files {
            config.parse(&fs::read_to_string(&file)?)?;
        }
        Ok(config)
    }

    fn parse(&mut self, text: &str) -> io::Result<()> {
        let mut logical = String::new();
        for line in text.lines() {
            let line = line.trim_end();
            if let Some(part) = line.strip_suffix('\\') {
                logical.push_str(part);
                logical.push(' ');
                continue;
            }
            logical.push_str(line);
            let line = config_line(&logical)?;
            let mut words = line.split_whitespace();
            if let Some(directive) = words.next() {
                let name = words.next().ok_or_else(|| invalid_module("modprobe directive has no module name"))?;
                let module = normalize_module_name(name);
                let rest = line.strip_prefix(directive).unwrap().trim_start().strip_prefix(name).unwrap().trim_start();
                match directive {
                    "alias" => {
                        let target = words.next().ok_or_else(|| invalid_module("alias directive has no target"))?;
                        if words.next().is_some() { return Err(invalid_module("alias directive has extra arguments")); }
                        self.aliases.push((name.to_string(), normalize_module_name(target)));
                    }
                    "options" => {
                        let options = self.options.entry(module).or_default();
                        if !options.is_empty() && !rest.is_empty() { options.push(' '); }
                        options.push_str(rest);
                    }
                    "softdep" => { self.softdeps.insert(module, parse_softdep(rest)?); }
                    "blacklist" => {
                        if words.next().is_some() { return Err(invalid_module("blacklist directive has extra arguments")); }
                        self.blacklist.insert(module);
                    }
                    "install" | "remove" => {
                        if rest.is_empty() { return Err(invalid_module("module command directive has no command")); }
                        if directive == "install" { self.install.insert(module); } else { self.remove.insert(module); }
                    }
                    _ => return Err(invalid_module(&format!("unsupported modprobe directive `{directive}`"))),
                }
            }
            logical.clear();
        }
        if !logical.is_empty() {
            return Err(invalid_module("unterminated modprobe configuration continuation"));
        }
        Ok(())
    }
}

fn config_line(line: &str) -> io::Result<&str> {
    let mut quote = None;
    let mut escaped = false;
    for (index, ch) in line.char_indices() {
        if escaped { escaped = false; continue; }
        if ch == '\\' { escaped = true; continue; }
        if ch == '"' || ch == '\'' {
            if quote == Some(ch) { quote = None; } else if quote.is_none() { quote = Some(ch); }
        } else if ch == '#' && quote.is_none() {
            return Ok(line[..index].trim());
        }
    }
    if quote.is_some() { return Err(invalid_module("unterminated quote in modprobe configuration")); }
    Ok(line.trim())
}

fn parse_softdep(text: &str) -> io::Result<(Vec<String>, Vec<String>)> {
    let (mut before, mut after) = (Vec::new(), Vec::new());
    let mut section = None;
    for word in text.split_whitespace() {
        match word {
            "pre:" => section = Some(false),
            "post:" => section = Some(true),
            _ => match section {
                Some(false) => before.push(normalize_module_name(word)),
                Some(true) => after.push(normalize_module_name(word)),
                None => return Err(invalid_module("softdep module must follow pre: or post:")),
            },
        }
    }
    Ok((before, after))
}

fn builtin_modules(root: &Path) -> io::Result<FxHashSet<String>> {
    match fs::read_to_string(root.join("modules.builtin")) {
        Ok(text) => Ok(text.lines().filter(|line| !line.is_empty()).map(module_name_from_path).collect()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(FxHashSet::default()),
        Err(error) => Err(error),
    }
}

fn loaded_modules() -> io::Result<FxHashSet<String>> {
    fs::read_dir("/sys/module")?.map(|entry| {
        let entry = entry?;
        entry.file_name().into_string().map(|name| normalize_module_name(&name))
            .map_err(|_| invalid_module("loaded module name is not UTF-8"))
    }).collect()
}

fn alias_matches(pattern: &str, name: &str) -> io::Result<bool> {
    let pattern = std::ffi::CString::new(pattern).map_err(|_| invalid_module("module alias contains NUL"))?;
    let name = std::ffi::CString::new(name).map_err(|_| invalid_module("module name contains NUL"))?;
    // Both pointers refer to terminated strings and fnmatch retains neither.
    let result = unsafe { libc::fnmatch(pattern.as_ptr(), name.as_ptr(), 0) };
    if result == 0 { Ok(true) } else if result == libc::FNM_NOMATCH { Ok(false) } else { Err(invalid_module("invalid module alias pattern")) }
}

fn resolve_modules<'a>(index: &'a ModuleIndex, config: &ModuleConfig, name: &str, active: &mut FxHashSet<String>) -> Result<Vec<&'a ModuleEntry>, String> {
    let normalized = normalize_module_name(name);
    if config.builtins.contains(&normalized) || normalized == "off" || normalized == "null" { return Ok(Vec::new()); }
    if let Some(entry) = index.get(name) { return Ok(vec![entry]); }
    if !active.insert(name.to_string()) { return Err(format!("alias cycle at `{name}`")); }
    let mut resolved = Vec::new();
    let mut matched_config = false;
    for (pattern, target) in &config.aliases {
        if alias_matches(pattern, name).map_err(|error| error.to_string())? {
            matched_config = true;
            resolved.extend(resolve_modules(index, config, target, active)?);
        }
    }
    if !matched_config {
        for entry in &index.entries {
            if config.blacklist.contains(&entry.name) { continue; }
            for pattern in entry.metadata.values("alias") {
                if alias_matches(pattern, name).map_err(|error| error.to_string())? {
                    resolved.push(entry);
                    break;
                }
            }
        }
    }
    active.remove(name);
    if resolved.is_empty() && !matched_config { return Err(format!("module or alias `{name}` not found")); }
    Ok(resolved)
}

struct ModulePlanEntry<'a> {
    entry: &'a ModuleEntry,
    params: String,
    requested: bool,
}

fn build_module_plan<'a>(index: &'a ModuleIndex, config: &ModuleConfig, name: &str, params: &str, remove: bool, span: Span) -> Result<Vec<ModulePlanEntry<'a>>, RuntimeError> {
    if params.contains('\0') || (remove && !params.is_empty()) {
        return Err(RuntimeError::new("linux-modprobe", "module parameters contain NUL or were supplied for removal").with_span(span));
    }
    let targets = resolve_modules(index, config, name, &mut FxHashSet::default())
        .map_err(|message| RuntimeError::new("linux-modprobe", message).with_span(span))?;
    let target_names = targets.iter().map(|entry| entry.name.clone()).collect::<FxHashSet<_>>();
    let mut order = Vec::new();
    let mut seen = FxHashSet::default();
    for target in targets {
        visit_module(index, config, target, remove, &mut FxHashSet::default(), &mut seen, &mut order)
            .map_err(|message| RuntimeError::new("linux-modprobe", message).with_span(span))?;
    }
    if remove { order.reverse(); }
    Ok(order.into_iter().map(|entry| {
        let requested = target_names.contains(&entry.name);
        let mut options = config.options.get(&entry.name).cloned().unwrap_or_default();
        if requested {
            let alias_options = if normalize_module_name(name) != entry.name { config.options.get(&normalize_module_name(name)).map(String::as_str).unwrap_or("") } else { "" };
            for extra in [alias_options, params] {
                if !extra.is_empty() {
                    if !options.is_empty() { options.push(' '); }
                    options.push_str(extra);
                }
            }
        }
        ModulePlanEntry { entry, params: options, requested }
    }).collect())
}

fn visit_module<'a>(index: &'a ModuleIndex, config: &ModuleConfig, entry: &'a ModuleEntry, remove: bool, active: &mut FxHashSet<String>, seen: &mut FxHashSet<String>, order: &mut Vec<&'a ModuleEntry>) -> Result<(), String> {
    if seen.contains(&entry.name) { return Ok(()); }
    if !active.insert(entry.name.clone()) { return Err(format!("dependency cycle at module `{}`", entry.name)); }
    if (if remove { &config.remove } else { &config.install }).contains(&entry.name) {
        return Err(format!("module `{}` has an unsupported {} command directive", entry.name, if remove { "remove" } else { "install" }));
    }
    let mut softdeps = (Vec::new(), Vec::new());
    if let Some(configured) = config.softdeps.get(&entry.name) {
        softdeps = configured.clone();
    } else {
        for field in entry.metadata.values("softdep") {
            let (before, after) = parse_softdep(field).map_err(|error| error.to_string())?;
            softdeps.0.extend(before);
            softdeps.1.extend(after);
        }
    }
    for name in softdeps.0.iter().chain(entry.metadata.depends().iter()) {
        if config.builtins.contains(name) { continue; }
        let dependency = index.get(name).ok_or_else(|| format!("module `{}` depends on missing module `{name}`", entry.name))?;
        visit_module(index, config, dependency, remove, active, seen, order)?;
    }
    active.remove(&entry.name);
    seen.insert(entry.name.clone());
    order.push(entry);
    for name in &softdeps.1 {
        if config.builtins.contains(name) { continue; }
        let dependency = index.get(name).ok_or_else(|| format!("module `{}` depends on missing module `{name}`", entry.name))?;
        visit_module(index, config, dependency, remove, active, seen, order)?;
    }
    Ok(())
}

fn module_ref_count(name: &str) -> io::Result<i64> {
    let text = match fs::read_to_string(Path::new("/sys/module").join(name).join("refcnt")) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error),
    };
    text.trim().parse().map_err(|_| invalid_module("module reference count is not an integer"))
}

fn remove_module(name: &str, span: Span) -> Result<(), RuntimeError> {
    #[cfg(target_os = "linux")]
    {
        let name = std::ffi::CString::new(name).map_err(|_| RuntimeError::new("linux-modprobe", "module name contains NUL").with_span(span))?;
        // Nonblocking deletion reports busy modules instead of waiting for
        // references to disappear; this never requests forced removal.
        let result = unsafe { libc::syscall(libc::SYS_delete_module, name.as_ptr(), libc::O_NONBLOCK) };
        if result == 0 { return Ok(()); }
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ENOENT) { return Ok(()); }
        Err(RuntimeError::host("linux-modprobe", &error).with_span(span))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = name;
        Err(RuntimeError::new("linux-modprobe", "module removal requires Linux").with_span(span))
    }
}


#[cfg(test)]
pub(super) fn test_module_plan(root: &Path, text: &str, name: &str, params: &str, remove: bool) -> Result<Vec<(String, String)>, String> {
    let index = ModuleIndex::scan(root).map_err(|error| error.to_string())?;
    let mut config = ModuleConfig { builtins: builtin_modules(root).map_err(|error| error.to_string())?, ..ModuleConfig::default() };
    config.parse(text).map_err(|error| error.to_string())?;
    let span = Span::new(crate::source::SourceId::new(0), 0, 0);
    build_module_plan(&index, &config, name, params, remove, span)
        .map(|plan| plan.into_iter().map(|module| (module.entry.name.clone(), module.params)).collect())
        .map_err(|error| error.message)
}
