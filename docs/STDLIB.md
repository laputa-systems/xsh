# XSH Standard Library

Generated from `src/modules/signature.rs`, `src/sema/records.rs`, and the docs engine. Do not edit by hand.

This file is the generated standard-library manual for modules, value methods, and standard record schemas. See `STDLIB-PROPOSALS.md` for stdlib design criteria and open proposals. See `docs/REFERENCE.md` for non-stdlib language and tooling reference data.

## Module Index

- `applet` - Internal primitives for shipped core applet scripts. (8 function(s))
- `archive` - Archive creation, extraction, listing, compression, and decompression. (11 function(s))
- `bytes` - Byte inspection, encoding, decoding, copying, and hashing helpers. (14 function(s))
- `cli` - Script command-line parsing into typed option records. (5 function(s))
- `cpu` - CPU capability queries. (1 function(s))
- `diff` - Unified diff generation. (1 function(s))
- `dns` - DNS lookup and name resolution helpers. (4 function(s))
- `elf` - ELF file-format inspection and dynamic dependency metadata. (1 function(s))
- `env` - Environment variable and PATH manipulation. (8 function(s))
- `fs` - Filesystem reads, writes, metadata, links, permissions, locking, and installation. (60 function(s))
- `group` - Unix group lookup records. (5 function(s))
- `hash` - Digest calculation and checksum verification. (8 function(s))
- `ini` - INI decoding, encoding, and file helpers. (4 function(s))
- `io` - Script stdin and stdout helpers. (5 function(s))
- `json` - JSON encoding, decoding, files, and streams. (9 function(s))
- `linux` - Linux-specific boot, mount, device, and shutdown operations. (65 function(s))
- `map` - Map collection helpers. (1 function(s))
- `mime` - MIME type lookup and media-type parsing helpers. (3 function(s))
- `module` - User module loading helpers. (1 function(s))
- `net` - HTTP request, transfer, and connection-pool helpers. (6 function(s))
- `patch` - Rooted patch application. (1 function(s))
- `path` - Path normalization and resolution. (1 function(s))
- `process` - Process discovery, command construction, execution, spawning, and signals. (16 function(s))
- `record` - Record inspection helpers. (1 function(s))
- `regex` - Regex compilation, matching, captures, and replacement. (1 function(s))
- `set` - String-key set helpers backed by Map[Bool]. (5 function(s))
- `shlex` - POSIX-like shell word rendering helpers. (2 function(s))
- `system` - Host system identity records. (4 function(s))
- `test` - Native XSH test assertions, temp resources, and host-effect mocks. (13 function(s))
- `time` - Clock, sleep, command measurement, and Jiff strtime formatting. (7 function(s))
- `tui` - Terminal styling, control sequences, and width-aware text padding. (19 function(s))
- `unix` - Unix process-group, PID 1, hostname, uptime, exec, and reaping helpers. (19 function(s))
- `user` - Unix user lookup records. (5 function(s))
- `utils` - Process-scoped utility helpers. (1 function(s))

## Modules

### `applet`

Internal primitives for shipped core applet scripts.

- `applet.current_euid() -> Int` - effect; Returns `Int`. ID `module.applet.current_euid.0`.
- `applet.current_exe() -> Result[Path, Error]` - effect; Returns `Path` or `Error` failure data. ID `module.applet.current_exe.0`.
- `applet.hash_password(password: Str, algorithm: Str) -> Result[Str, Error]` - effect; Returns `Str` or `Error` failure data. ID `module.applet.hash_password.0`.
  Params: `password: Str`, `algorithm: Str`
- `applet.login_session(user: {gid: Int, home: Path, name: Str, shell: Str, uid: Int}, preserve_env: Bool, host: Str) -> Result[Int, Error]` - effect; Returns `Int` or `Error` failure data. ID `module.applet.login_session.0`.
  Params: `user: {gid: Int, home: Path, name: Str, shell: Str, uid: Int}`, `preserve_env: Bool`, `host: Str`
- `applet.mdev(argv: List[Str]) -> Result[Int, Error]` - effect; Returns `Int` or `Error` failure data. ID `module.applet.mdev.0`.
  Params: `argv: List[Str]`
- `applet.su_session(user: {gid: Int, home: Path, name: Str, shell: Str, uid: Int}, login: Bool, preserve_env: Bool, shell: Str, command: Str, extra_args: List[Str]) -> Result[Int, Error]` - effect; Returns `Int` or `Error` failure data. ID `module.applet.su_session.0`.
  Params: `user: {gid: Int, home: Path, name: Str, shell: Str, uid: Int}`, `login: Bool`, `preserve_env: Bool`, `shell: Str`, `command: Str`, `extra_args: List[Str]`
- `applet.sulogin_session(user: {gid: Int, home: Path, name: Str, shell: Str, uid: Int}) -> Result[Int, Error]` - effect; Returns `Int` or `Error` failure data. ID `module.applet.sulogin_session.0`.
  Params: `user: {gid: Int, home: Path, name: Str, shell: Str, uid: Int}`
- `applet.verify_password(password: Str, hash: Str) -> Bool` - effect; Returns `Bool`. ID `module.applet.verify_password.0`.
  Params: `password: Str`, `hash: Str`

### `archive`

Archive creation, extraction, listing, compression, and decompression.

- `archive.compress(source: Path, dest: Path, format: Str = default, level: Int = default, overwrite: Bool = default) -> Result[Unit, Error]` - effect; Returns `Unit` or `Error` failure data. ID `module.archive.compress.0`.
  Params: `source: Path`, `dest: Path`, `format: Str = default`, `level: Int = default`, `overwrite: Bool = default`
- `archive.cpio_create(path: Path, root: Path, entries: List[Path], overwrite: Bool = default) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.archive.cpio_create.0`.
  Params: `path: Path`, `root: Path`, `entries: List[Path]`, `overwrite: Bool = default`
- `archive.cpio_extract(path: Path, dest: Path, overwrite: Bool = default) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.archive.cpio_extract.0`.
  Params: `path: Path`, `dest: Path`, `overwrite: Bool = default`
- `archive.cpio_list(path: Path) -> Result[Stream[{kind: Str, link_name: Str, mode: Int, modified: Int, path: Path, size: Int}], Error]` - effect; Returns a live archive-order stream or `Error` failure data. ID `module.archive.cpio_list.0`.
  Params: `path: Path`
- `archive.decompress(source: Path, dest: Path, format: Str = default, overwrite: Bool = default) -> Result[Unit, Error]` - effect; Returns `Unit` or `Error` failure data. ID `module.archive.decompress.0`.
  Params: `source: Path`, `dest: Path`, `format: Str = default`, `overwrite: Bool = default`
- `archive.decompress_bytes(source: Path, format: Str = default) -> Result[Bytes, Error]` - effect; Returns `Bytes` or `Error` failure data. ID `module.archive.decompress_bytes.0`.
  Params: `source: Path`, `format: Str = default`
- `archive.tar_create(path: Path, root: Path, entries: List[Path], compression: Str = default, overwrite: Bool = default) -> Result[Unit, Error]` - effect; Returns `Unit` or `Error` failure data. ID `module.archive.tar_create.0`.
  Params: `path: Path`, `root: Path`, `entries: List[Path]`, `compression: Str = default`, `overwrite: Bool = default`
- `archive.tar_extract(path: Path, dest: Path, strip_components: Int = default, compression: Str = default, overwrite: Bool = default, members: List[Path] = default) -> Result[Unit, Error]` - effect; Returns `Unit` or `Error` failure data. ID `module.archive.tar_extract.0`.
  Params: `path: Path`, `dest: Path`, `strip_components: Int = default`, `compression: Str = default`, `overwrite: Bool = default`, `members: List[Path] = default`
- `archive.tar_list(path: Path, compression: Str = default, members: List[Path] = default) -> Result[Stream[{kind: Str, link_name: Str, mode: Int, modified: Int, path: Path, size: Int}], Error]` - effect; Returns a lazy archive-order stream of selected entries or `Error` failure data. ID `module.archive.tar_list.0`.
  Params: `path: Path`, `compression: Str = default`, `members: List[Path] = default`
- `archive.zip_extract(path: Path, dest: Path, overwrite: Bool = default) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.archive.zip_extract.0`.
  Params: `path: Path`, `dest: Path`, `overwrite: Bool = default`
- `archive.zip_list(path: Path) -> Result[Stream[{kind: Str, link_name: Str, mode: Int, modified: Int, path: Path, size: Int}], Error]` - effect; Returns a live archive-order stream or `Error` failure data. ID `module.archive.zip_list.0`.
  Params: `path: Path`

### `bytes`

Byte inspection, encoding, decoding, copying, and hashing helpers.

- `bytes.concat(chunks: List[Bytes]) -> Bytes` - pure; Returns `Bytes`. ID `module.bytes.concat.0`.
  Params: `chunks: List[Bytes]`
- `bytes.copy(source: Path, dest: Path, block_size: Int = default, count: Int = default, skip: Int = default, seek: Int = default, overwrite: Bool = default) -> Result[{blocks: Int, bytes: Int}, Error]` - effect; Returns `{blocks: Int, bytes: Int}` or `Error` failure data. ID `module.bytes.copy.0`.
  Params: `source: Path`, `dest: Path`, `block_size: Int = default`, `count: Int = default`, `skip: Int = default`, `seek: Int = default`, `overwrite: Bool = default`
- `bytes.copy_file(source: Path, dest: Path, source_offset: Int = default, dest_offset: Int = default, length: Int = default, create: Bool = default, truncate: Bool = default) -> Result[{blocks: Int, bytes: Int}, Error]` - effect; Returns `{blocks: Int, bytes: Int}` or `Error` failure data. ID `module.bytes.copy_file.0`.
  Params: `source: Path`, `dest: Path`, `source_offset: Int = default`, `dest_offset: Int = default`, `length: Int = default`, `create: Bool = default`, `truncate: Bool = default`
- `bytes.from_ints(values: List[Int]) -> Result[Bytes, Error]` - pure; Returns `Bytes` or `Error` failure data. ID `module.bytes.from_ints.0`.
  Params: `values: List[Int]`
- `bytes.from_text(text: Str) -> Bytes` - pure; Returns `Bytes`. ID `module.bytes.from_text.0`.
  Params: `text: Str`
- `bytes.human(size: Int) -> Str` - pure; Returns `Str`. ID `module.bytes.human.0`.
  Params: `size: Int`
- `bytes.pack_be(value: Int, width: Int) -> Result[Bytes, Error]` - pure; Returns `Bytes` or `Error` failure data. ID `module.bytes.pack_be.0`.
  Params: `value: Int`, `width: Int`
- `bytes.pack_le(value: Int, width: Int) -> Result[Bytes, Error]` - pure; Returns `Bytes` or `Error` failure data. ID `module.bytes.pack_le.0`.
  Params: `value: Int`, `width: Int`
- `bytes.read_at(path: Path, offset: Int, length: Int) -> Result[Bytes, Error]` - effect; Returns `Bytes` or `Error` failure data. ID `module.bytes.read_at.0`.
  Params: `path: Path`, `offset: Int`, `length: Int`
- `bytes.unpack_be(data: Bytes, width: Int, offset: Int = default) -> Result[Int, Error]` - pure; Returns `Int` or `Error` failure data. ID `module.bytes.unpack_be.0`.
  Params: `data: Bytes`, `width: Int`, `offset: Int = default`
- `bytes.unpack_le(data: Bytes, width: Int, offset: Int = default) -> Result[Int, Error]` - pure; Returns `Int` or `Error` failure data. ID `module.bytes.unpack_le.0`.
  Params: `data: Bytes`, `width: Int`, `offset: Int = default`
- `bytes.write_at(path: Path, offset: Int, data: Bytes, create: Bool = default) -> Result[Int, Error]` - effect; Returns `Int` or `Error` failure data. ID `module.bytes.write_at.0`.
  Params: `path: Path`, `offset: Int`, `data: Bytes`, `create: Bool = default`
- `bytes.zero(length: Int) -> Result[Bytes, Error]` - pure; Returns `Bytes` or `Error` failure data. ID `module.bytes.zero.0`.
  Params: `length: Int`
- `bytes.zero_at(path: Path, offset: Int, length: Int, create: Bool = default) -> Result[Int, Error]` - effect; Returns `Int` or `Error` failure data. ID `module.bytes.zero_at.0`.
  Params: `path: Path`, `offset: Int`, `length: Int`, `create: Bool = default`

### `cli`

Script command-line parsing into typed option records.

- `cli.commands(argv: List[Str], commands: Record) -> Result[Record, Error]` - pure; Returns `Record` or `Error` failure data. ID `module.cli.commands.0`.
  Params: `argv: List[Str]`, `commands: Record`
- `cli.commands(argv: List[Str], rootless_default: Str, commands: Record, fallback_command: Record = default) -> Result[Record, Error]` - pure; Returns `Record` or `Error` failure data. ID `module.cli.commands.1`.
  Params: `argv: List[Str]`, `rootless_default: Str`, `commands: Record`, `fallback_command: Record = default`
- `cli.parse(argv: List[Str], schema: Record, command: Str = default) -> Result[Record, Error]` - pure; Returns `Record` or `Error` failure data. ID `module.cli.parse.0`.
  Params: `argv: List[Str]`, `schema: Record`, `command: Str = default`
- `cli.parse_full(argv: List[Str], schema: Record, env: Record = default, command: Str = default) -> Result[Record, Error]` - pure; Returns `Record` or `Error` failure data. ID `module.cli.parse_full.0`.
  Params: `argv: List[Str]`, `schema: Record`, `env: Record = default`, `command: Str = default`
- `cli.tokens(argv: List[Str], value_flags: List[Str] = default) -> Result[List[{kind: Str, name: Str, value: Str}], Error]` - pure; Returns `List[{kind: Str, name: Str, value: Str}]` or `Error` failure data. ID `module.cli.tokens.0`.
  Params: `argv: List[Str]`, `value_flags: List[Str] = default`
- `cli.usage(schema: Record, command: Str = default) -> Str` - pure; Returns `Str`. ID `module.cli.usage.0`.
  Params: `schema: Record`, `command: Str = default`

### `cpu`

CPU capability queries.

- `cpu.count() -> Int` - pure; Returns `Int`. ID `module.cpu.count.0`.

### `diff`

Unified diff generation.

- `diff.unified(original: Path, modified: Path, context: Int = default) -> Result[{files: Int, hunks: Int, text: Str}, Error]` - effect; Returns `{files: Int, hunks: Int, text: Str}` or `Error` failure data. ID `module.diff.unified.0`.
  Params: `original: Path`, `modified: Path`, `context: Int = default`

### `dns`

DNS lookup and name resolution helpers.

- `dns.lookup(name: Str, record: Str = default, server: Str = default, timeout: Duration = default) -> Result[List[{name: Str, record: Str, ttl: Int, value: Str}], Error]` - effect; Returns `List[{name: Str, record: Str, ttl: Int, value: Str}]` or `Error` failure data. ID `module.dns.lookup.0`.
  Params: `name: Str`, `record: Str = default`, `server: Str = default`, `timeout: Duration = default`
- `dns.nameservers() -> Result[List[Str], Error]` - effect; Returns `List[Str]` or `Error` failure data. ID `module.dns.nameservers.0`.
- `dns.resolve_host(name: Str, family: Str = default) -> Result[List[{addr: Str, family: Str, name: Str}], Error]` - effect; Returns `List[{addr: Str, family: Str, name: Str}]` or `Error` failure data. ID `module.dns.resolve_host.0`.
  Params: `name: Str`, `family: Str = default`
- `dns.reverse(addr: Str) -> Result[List[Str], Error]` - effect; Returns `List[Str]` or `Error` failure data. ID `module.dns.reverse.0`.
  Params: `addr: Str`

### `elf`

ELF file-format inspection and dynamic dependency metadata.

- `elf.inspect(path: Path) -> Result[{class: Str, dynamic_tags: List[{tag: Str, value: Int}], endian: Str, flags: List[Str], interpreter: Str, machine: Str, needed: List[Str], os_abi: Str, path: Path, rpath: Str, runpath: Str, soname: Str, type: Str}, Error]` - effect; Returns `{class: Str, dynamic_tags: List[{tag: Str, value: Int}], endian: Str, flags: List[Str], interpreter: Str, machine: Str, needed: List[Str], os_abi: Str, path: Path, rpath: Str, runpath: Str, soname: Str, type: Str}` or `Error` failure data. ID `module.elf.inspect.0`.
  Params: `path: Path`

### `env`

Environment variable and PATH manipulation.

- `env.bool(name: Str, fallback: Bool = default) -> Result[Bool, Error]` - effect; Returns `Bool` or `Error` failure data. ID `module.env.bool.0`.
  Params: `name: Str`, `fallback: Bool = default`
- `env.get(name: Str) -> Result[Str, Error]` - effect; Returns `Str` or `Error` failure data. ID `module.env.get.0`.
  Params: `name: Str`
- `env.get_or(name: Str, fallback: Str = default) -> Result[Str, Error]` - effect; Returns `Str` or `Error` failure data. ID `module.env.get_or.0`.
  Params: `name: Str`, `fallback: Str = default`
- `env.int(name: Str, fallback: Int = default) -> Result[Int, Error]` - effect; Returns `Int` or `Error` failure data. ID `module.env.int.0`.
  Params: `name: Str`, `fallback: Int = default`
- `env.list() -> Result[List[{name: Str, value: Str}], Error]` - effect; Returns `List[{name: Str, value: Str}]` or `Error` failure data. ID `module.env.list.0`.
- `env.path(name: Str, fallback: Path = default) -> Result[Path, Error]` - effect; Returns `Path` or `Error` failure data. ID `module.env.path.0`.
  Params: `name: Str`, `fallback: Path = default`
- `env.path_entries(name: Str) -> Result[List[{empty: Bool, index: Int, path: Path, raw: Str}], Error]` - effect; Returns `List[{empty: Bool, index: Int, path: Path, raw: Str}]` or `Error` failure data. ID `module.env.path_entries.0`.
  Params: `name: Str`
- `env.path_list(name: Str) -> Result[List[Path], Error]` - effect; Returns `List[Path]` or `Error` failure data. ID `module.env.path_list.0`.
  Params: `name: Str`

### `fs`

Filesystem reads, writes, metadata, links, permissions, locking, and installation.

- `fs.chgrp(path: Path, group: {gid: Int, members: List[Str], name: Str}, follow_symlinks: Bool = default) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.fs.chgrp.0`.
  Params: `path: Path`, `group: {gid: Int, members: List[Str], name: Str}`, `follow_symlinks: Bool = default`
- `fs.children(path: Path, stat: Bool = default, ordered: Bool = default) -> Result[Stream[{accessed: Int, blocks_512: Int, executable: Bool, ext: Str, gid: Int, group_executable: Bool, kind: Str, mode: Int, modified: Int, name: Str, other_executable: Bool, owner_executable: Bool, path: Path, setgid: Bool, setuid: Bool, size: Int, sticky: Bool, uid: Int, world_writable: Bool}], Error]` - effect; Returns `Stream[{accessed: Int, blocks_512: Int, executable: Bool, ext: Str, gid: Int, group_executable: Bool, kind: Str, mode: Int, modified: Int, name: Str, other_executable: Bool, owner_executable: Bool, path: Path, setgid: Bool, setuid: Bool, size: Int, sticky: Bool, uid: Int, world_writable: Bool}]` or `Error` failure data. ID `module.fs.children.0`.
  Params: `path: Path`, `stat: Bool = default`, `ordered: Bool = default`
- `fs.chmod(path: Path, mode: Int) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.fs.chmod.0`.
  Params: `path: Path`, `mode: Int`
- `fs.chown(path: Path, owner: {gid: Int, home: Path, name: Str, shell: Str, uid: Int}, follow_symlinks: Bool = default) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.fs.chown.0`.
  Params: `path: Path`, `owner: {gid: Int, home: Path, name: Str, shell: Str, uid: Int}`, `follow_symlinks: Bool = default`
- `fs.close_root(root: {id: Int}) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.fs.close_root.0`.
  Params: `root: {id: Int}`
- `fs.copy(source: Path, dest: Path, overwrite: Bool = default) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.fs.copy.0`.
  Params: `source: Path`, `dest: Path`, `overwrite: Bool = default`
- `fs.copy_tree(source: Path, dest: Path, parents: Bool = default, overwrite: Bool = default, follow_symlinks: Bool = default) -> Result[{dirs: Int, files: Int, symlinks: Int}, Error]` - effect; Returns `{dirs: Int, files: Int, symlinks: Int}` or `Error` failure data. ID `module.fs.copy_tree.0`.
  Params: `source: Path`, `dest: Path`, `parents: Bool = default`, `overwrite: Bool = default`, `follow_symlinks: Bool = default`
- `fs.cwd() -> Result[Path, Error]` - effect; Returns `Path` or `Error` failure data. ID `module.fs.cwd.0`.
- `fs.dirs(path: Path, gitignore: Bool = default, stat: Bool = default, hidden: Bool = default) -> Result[Stream[{accessed: Int, blocks_512: Int, executable: Bool, ext: Str, gid: Int, group_executable: Bool, kind: Str, mode: Int, modified: Int, name: Str, other_executable: Bool, owner_executable: Bool, path: Path, setgid: Bool, setuid: Bool, size: Int, sticky: Bool, uid: Int, world_writable: Bool}], Error]` - effect; Returns `Stream[{accessed: Int, blocks_512: Int, executable: Bool, ext: Str, gid: Int, group_executable: Bool, kind: Str, mode: Int, modified: Int, name: Str, other_executable: Bool, owner_executable: Bool, path: Path, setgid: Bool, setuid: Bool, size: Int, sticky: Bool, uid: Int, world_writable: Bool}]` or `Error` failure data. ID `module.fs.dirs.0`.
  Params: `path: Path`, `gitignore: Bool = default`, `stat: Bool = default`, `hidden: Bool = default`
- `fs.executable(path: Path) -> Result[Bool, Error]` - effect; Returns `Bool` or `Error` failure data. ID `module.fs.executable.0`.
  Params: `path: Path`
- `fs.executable(mode: Int) -> Bool` - pure; Returns `Bool`. ID `module.fs.executable.1`.
  Params: `mode: Int`
- `fs.exists(path: Path) -> Result[Bool, Error]` - effect; Returns `Bool` or `Error` failure data. ID `module.fs.exists.0`.
  Params: `path: Path`
- `fs.files(path: Path, gitignore: Bool = default, stat: Bool = default, exts: List[Str] = default, hidden: Bool = default) -> Result[Stream[{accessed: Int, blocks_512: Int, executable: Bool, ext: Str, gid: Int, group_executable: Bool, kind: Str, mode: Int, modified: Int, name: Str, other_executable: Bool, owner_executable: Bool, path: Path, setgid: Bool, setuid: Bool, size: Int, sticky: Bool, uid: Int, world_writable: Bool}], Error]` - effect; Returns `Stream[{accessed: Int, blocks_512: Int, executable: Bool, ext: Str, gid: Int, group_executable: Bool, kind: Str, mode: Int, modified: Int, name: Str, other_executable: Bool, owner_executable: Bool, path: Path, setgid: Bool, setuid: Bool, size: Int, sticky: Bool, uid: Int, world_writable: Bool}]` or `Error` failure data. ID `module.fs.files.0`.
  Params: `path: Path`, `gitignore: Bool = default`, `stat: Bool = default`, `exts: List[Str] = default`, `hidden: Bool = default`
- `fs.filesystem_stats(path: Path) -> Result[{available_1k: Int, blocks_1k: Int, capacity_percent: Int, used_1k: Int}, Error]` - effect; Returns `{available_1k: Int, blocks_1k: Int, capacity_percent: Int, used_1k: Int}` or `Error` failure data. ID `module.fs.filesystem_stats.0`.
  Params: `path: Path`
- `fs.fsync(path: Path) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.fs.fsync.0`.
  Params: `path: Path`
- `fs.gitroot() -> Result[Path, Error]` - effect; Returns `Path` or `Error` failure data. ID `module.fs.gitroot.0`.
- `fs.group_executable(mode: Int) -> Bool` - pure; Returns `Bool`. ID `module.fs.group_executable.0`.
  Params: `mode: Int`
- `fs.install(source: Path, dest: Path, mode: Int, parents: Bool = default, overwrite: Bool = default) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.fs.install.0`.
  Params: `source: Path`, `dest: Path`, `mode: Int`, `parents: Bool = default`, `overwrite: Bool = default`
- `fs.install_as(source: Path, dest: Path, mode: Int, owner: {gid: Int, home: Path, name: Str, shell: Str, uid: Int}, group: {gid: Int, members: List[Str], name: Str}, parents: Bool = default, overwrite: Bool = default) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.fs.install_as.0`.
  Params: `source: Path`, `dest: Path`, `mode: Int`, `owner: {gid: Int, home: Path, name: Str, shell: Str, uid: Int}`, `group: {gid: Int, members: List[Str], name: Str}`, `parents: Bool = default`, `overwrite: Bool = default`
- `fs.lock(path: Path, shared: Bool = default, nonblocking: Bool = default) -> Result[{id: Int, path: Path, shared: Bool}, Error]` - effect; Returns `{id: Int, path: Path, shared: Bool}` or `Error` failure data. ID `module.fs.lock.0`.
  Params: `path: Path`, `shared: Bool = default`, `nonblocking: Bool = default`
- `fs.ls(path: Path, stat: Bool = default, ordered: Bool = default) -> Result[Stream[{accessed: Int, blocks_512: Int, executable: Bool, ext: Str, gid: Int, group_executable: Bool, kind: Str, mode: Int, modified: Int, name: Str, other_executable: Bool, owner_executable: Bool, path: Path, setgid: Bool, setuid: Bool, size: Int, sticky: Bool, uid: Int, world_writable: Bool}], Error]` - effect; Returns `Stream[{accessed: Int, blocks_512: Int, executable: Bool, ext: Str, gid: Int, group_executable: Bool, kind: Str, mode: Int, modified: Int, name: Str, other_executable: Bool, owner_executable: Bool, path: Path, setgid: Bool, setuid: Bool, size: Int, sticky: Bool, uid: Int, world_writable: Bool}]` or `Error` failure data. ID `module.fs.ls.0`.
  Params: `path: Path`, `stat: Bool = default`, `ordered: Bool = default`
- `fs.metadata(path: Path) -> Result[{accessed: Int, blocks_512: Int, executable: Bool, ext: Str, gid: Int, group_executable: Bool, kind: Str, mode: Int, modified: Int, name: Str, other_executable: Bool, owner_executable: Bool, path: Path, setgid: Bool, setuid: Bool, size: Int, sticky: Bool, uid: Int, world_writable: Bool}, Error]` - effect; Returns `{accessed: Int, blocks_512: Int, executable: Bool, ext: Str, gid: Int, group_executable: Bool, kind: Str, mode: Int, modified: Int, name: Str, other_executable: Bool, owner_executable: Bool, path: Path, setgid: Bool, setuid: Bool, size: Int, sticky: Bool, uid: Int, world_writable: Bool}` or `Error` failure data. ID `module.fs.metadata.0`.
  Params: `path: Path`
- `fs.mkdir(path: Path, parents: Bool = default) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.fs.mkdir.0`.
  Params: `path: Path`, `parents: Bool = default`
- `fs.mkfifo(path: Path, mode: Int) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.fs.mkfifo.0`.
  Params: `path: Path`, `mode: Int`
- `fs.mount_for(path: Path) -> Result[{available_1k: Int, blocks_1k: Int, capacity_percent: Int, files: Int, files_capacity_percent: Int, files_free: Int, files_used: Int, filesystem: Str, fstype: Str, mounted_on: Path, readonly: Bool, used_1k: Int}, Error]` - effect; Returns `{available_1k: Int, blocks_1k: Int, capacity_percent: Int, files: Int, files_capacity_percent: Int, files_free: Int, files_used: Int, filesystem: Str, fstype: Str, mounted_on: Path, readonly: Bool, used_1k: Int}` or `Error` failure data. ID `module.fs.mount_for.0`.
  Params: `path: Path`
- `fs.mounts() -> Result[Stream[{available_1k: Int, blocks_1k: Int, capacity_percent: Int, files: Int, files_capacity_percent: Int, files_free: Int, files_used: Int, filesystem: Str, fstype: Str, mounted_on: Path, readonly: Bool, used_1k: Int}], Error]` - effect; Returns a single-use stream or `Error` failure data. ID `module.fs.mounts.0`.
- `fs.open_root(path: Path) -> Result[{id: Int}, Error]` - effect; Returns `{id: Int}` or `Error` failure data. ID `module.fs.open_root.0`.
  Params: `path: Path`
- `fs.other_executable(mode: Int) -> Bool` - pure; Returns `Bool`. ID `module.fs.other_executable.0`.
  Params: `mode: Int`
- `fs.owner_executable(mode: Int) -> Bool` - pure; Returns `Bool`. ID `module.fs.owner_executable.0`.
  Params: `mode: Int`
- `fs.project_root(kind: Str, qualifier: Str, organization: Str, application: Str) -> Result[{id: Int}, Error]` - effect; Returns `{id: Int}` or `Error` failure data. ID `module.fs.project_root.0`.
  Params: `kind: Str`, `qualifier: Str`, `organization: Str`, `application: Str`
- `fs.read_text(path: Path) -> Result[Str, Error]` - effect; Returns `Str` or `Error` failure data. ID `module.fs.read_text.0`.
  Params: `path: Path`
- `fs.remove(path: Path, missing_ok: Bool = default) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.fs.remove.0`.
  Params: `path: Path`, `missing_ok: Bool = default`
- `fs.remove_manifest(root: Path, manifest: List[Path], missing_ok: Bool = default, prune_dirs: Bool = default) -> Result[{missing: Int, pruned_dirs: Int, removed: Int}, Error]` - effect; Returns `{missing: Int, pruned_dirs: Int, removed: Int}` or `Error` failure data. ID `module.fs.remove_manifest.0`.
  Params: `root: Path`, `manifest: List[Path]`, `missing_ok: Bool = default`, `prune_dirs: Bool = default`
- `fs.rename(source: Path, dest: Path, overwrite: Bool = default) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.fs.rename.0`.
  Params: `source: Path`, `dest: Path`, `overwrite: Bool = default`
- `fs.root(root: {id: Int}, path: Path) -> Result[{id: Int}, Error]` - effect; Returns `{id: Int}` or `Error` failure data. ID `module.fs.root.0`.
  Params: `root: {id: Int}`, `path: Path`
- `fs.root_chmod(root: {id: Int}, path: Path, mode: Int) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.fs.root_chmod.0`.
  Params: `root: {id: Int}`, `path: Path`, `mode: Int`
- `fs.root_exists(root: {id: Int}, path: Path) -> Result[Bool, Error]` - effect; Returns `Bool` or `Error` failure data. ID `module.fs.root_exists.0`.
  Params: `root: {id: Int}`, `path: Path`
- `fs.root_install_file(source_root: {id: Int}, source: Path, dest_root: {id: Int}, dest: Path, mode: Int, parents: Bool = default, overwrite: Bool = default) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.fs.root_install_file.0`.
  Params: `source_root: {id: Int}`, `source: Path`, `dest_root: {id: Int}`, `dest: Path`, `mode: Int`, `parents: Bool = default`, `overwrite: Bool = default`
- `fs.root_metadata(root: {id: Int}, path: Path) -> Result[{accessed: Int, blocks_512: Int, executable: Bool, ext: Str, gid: Int, group_executable: Bool, kind: Str, mode: Int, modified: Int, name: Str, other_executable: Bool, owner_executable: Bool, path: Path, setgid: Bool, setuid: Bool, size: Int, sticky: Bool, uid: Int, world_writable: Bool}, Error]` - effect; Returns `{accessed: Int, blocks_512: Int, executable: Bool, ext: Str, gid: Int, group_executable: Bool, kind: Str, mode: Int, modified: Int, name: Str, other_executable: Bool, owner_executable: Bool, path: Path, setgid: Bool, setuid: Bool, size: Int, sticky: Bool, uid: Int, world_writable: Bool}` or `Error` failure data. ID `module.fs.root_metadata.0`.
  Params: `root: {id: Int}`, `path: Path`
- `fs.root_mkdir(root: {id: Int}, path: Path, mode: Int = default, parents: Bool = default) -> Result[Unit, Error]` - effect; Returns `Unit` or `Error` failure data. ID `module.fs.root_mkdir.0`.
  Params: `root: {id: Int}`, `path: Path`, `mode: Int = default`, `parents: Bool = default`
- `fs.root_path(root: {id: Int}) -> Result[Path, Error]` - effect; Returns `Path` or `Error` failure data. ID `module.fs.root_path.0`.
  Params: `root: {id: Int}`
- `fs.root_read(root: {id: Int}, path: Path) -> Result[Bytes, Error]` - effect; Returns `Bytes` or `Error` failure data. ID `module.fs.root_read.0`.
  Params: `root: {id: Int}`, `path: Path`
- `fs.root_read_text(root: {id: Int}, path: Path) -> Result[Str, Error]` - effect; Returns `Str` or `Error` failure data. ID `module.fs.root_read_text.0`.
  Params: `root: {id: Int}`, `path: Path`
- `fs.root_readlink(root: {id: Int}, path: Path) -> Result[Path, Error]` - effect; Returns `Path` or `Error` failure data. ID `module.fs.root_readlink.0`.
  Params: `root: {id: Int}`, `path: Path`
- `fs.root_remove(root: {id: Int}, path: Path, dir: Bool = default) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.fs.root_remove.0`.
  Params: `root: {id: Int}`, `path: Path`, `dir: Bool = default`
- `fs.root_symlink(root: {id: Int}, target: Path, path: Path, parents: Bool = default, overwrite: Bool = default) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.fs.root_symlink.0`.
  Params: `root: {id: Int}`, `target: Path`, `path: Path`, `parents: Bool = default`, `overwrite: Bool = default`
- `fs.root_write(root: {id: Int}, path: Path, data: Bytes) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.fs.root_write.0`.
  Params: `root: {id: Int}`, `path: Path`, `data: Bytes`
- `fs.root_write(root: {id: Int}, path: Path, data: Str) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.fs.root_write.1`.
  Params: `root: {id: Int}`, `path: Path`, `data: Str`
- `fs.root_write_atomic(root: {id: Int}, path: Path, data: Bytes) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.fs.root_write_atomic.0`.
  Params: `root: {id: Int}`, `path: Path`, `data: Bytes`
- `fs.root_write_atomic(root: {id: Int}, path: Path, data: Str) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.fs.root_write_atomic.1`.
  Params: `root: {id: Int}`, `path: Path`, `data: Str`
- `fs.setgid(mode: Int) -> Bool` - pure; Returns `Bool`. ID `module.fs.setgid.0`.
  Params: `mode: Int`
- `fs.setuid(mode: Int) -> Bool` - pure; Returns `Bool`. ID `module.fs.setuid.0`.
  Params: `mode: Int`
- `fs.sticky(mode: Int) -> Bool` - pure; Returns `Bool`. ID `module.fs.sticky.0`.
  Params: `mode: Int`
- `fs.symlink(target: Path, path: Path) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.fs.symlink.0`.
  Params: `target: Path`, `path: Path`
- `fs.sync() -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.fs.sync.0`.
- `fs.tempdir() -> Result[{id: Int}, Error]` - effect; Returns `{id: Int}` or `Error` failure data. ID `module.fs.tempdir.0`.
- `fs.tempfile() -> Result[{path: Path, root: {id: Int}}, Error]` - effect; Returns `{path: Path, root: {id: Int}}` or `Error` failure data. ID `module.fs.tempfile.0`.
- `fs.unlock(lock: {id: Int, path: Path, shared: Bool}) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.fs.unlock.0`.
  Params: `lock: {id: Int, path: Path, shared: Bool}`
- `fs.user_root(kind: Str) -> Result[{id: Int}, Error]` - effect; Returns `{id: Int}` or `Error` failure data. ID `module.fs.user_root.0`.
  Params: `kind: Str`
- `fs.walk(path: Path, gitignore: Bool = default, stat: Bool = default, hidden: Bool = default) -> Result[Stream[{accessed: Int, blocks_512: Int, executable: Bool, ext: Str, gid: Int, group_executable: Bool, kind: Str, mode: Int, modified: Int, name: Str, other_executable: Bool, owner_executable: Bool, path: Path, setgid: Bool, setuid: Bool, size: Int, sticky: Bool, uid: Int, world_writable: Bool}], Error]` - effect; Returns `Stream[{accessed: Int, blocks_512: Int, executable: Bool, ext: Str, gid: Int, group_executable: Bool, kind: Str, mode: Int, modified: Int, name: Str, other_executable: Bool, owner_executable: Bool, path: Path, setgid: Bool, setuid: Bool, size: Int, sticky: Bool, uid: Int, world_writable: Bool}]` or `Error` failure data. ID `module.fs.walk.0`.
  Params: `path: Path`, `gitignore: Bool = default`, `stat: Bool = default`, `hidden: Bool = default`
- `fs.world_writable(mode: Int) -> Bool` - pure; Returns `Bool`. ID `module.fs.world_writable.0`.
  Params: `mode: Int`
- `fs.write(path: Path, data: Bytes) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.fs.write.0`.
  Params: `path: Path`, `data: Bytes`
- `fs.write(path: Path, data: Str) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.fs.write.1`.
  Params: `path: Path`, `data: Str`
- `fs.write_atomic(path: Path, data: Bytes) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.fs.write_atomic.0`.
  Params: `path: Path`, `data: Bytes`
- `fs.write_atomic(path: Path, data: Str) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.fs.write_atomic.1`.
  Params: `path: Path`, `data: Str`

### `group`

Unix group lookup records.

- `group.add(name: Str, gid: Int = default) -> Result[{gid: Int, members: List[Str], name: Str}, Error]` - effect; Returns `{gid: Int, members: List[Str], name: Str}` or `Error` failure data. ID `module.group.add.0`.
  Params: `name: Str`, `gid: Int = default`
- `group.by_gid(gid: Int) -> Result[{gid: Int, members: List[Str], name: Str}, Error]` - effect; Returns `{gid: Int, members: List[Str], name: Str}` or `Error` failure data. ID `module.group.by_gid.0`.
  Params: `gid: Int`
- `group.current() -> Result[{gid: Int, members: List[Str], name: Str}, Error]` - effect; Returns `{gid: Int, members: List[Str], name: Str}` or `Error` failure data. ID `module.group.current.0`.
- `group.lookup(name: Str) -> Result[{gid: Int, members: List[Str], name: Str}, Error]` - effect; Returns `{gid: Int, members: List[Str], name: Str}` or `Error` failure data. ID `module.group.lookup.0`.
  Params: `name: Str`
- `group.remove(name: Str) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.group.remove.0`.
  Params: `name: Str`

### `hash`

Digest calculation and checksum verification.

- `hash.crc32(data: Bytes) -> Int` - pure; Returns `Int`. ID `module.hash.crc32.0`.
  Params: `data: Bytes`
- `hash.crc32c(data: Bytes) -> Int` - pure; Returns `Int`. ID `module.hash.crc32c.0`.
  Params: `data: Bytes`
- `hash.md5(data: Bytes) -> Digest` - pure; Returns `Digest`. ID `module.hash.md5.0`.
  Params: `data: Bytes`
- `hash.md5(path: Path) -> Result[Digest, Error]` - effect; Returns `Digest` or `Error` failure data. ID `module.hash.md5.1`.
  Params: `path: Path`
- `hash.parse_check_line(line: Str) -> Result[Record, Error]` - pure; Returns `Record` or `Error` failure data. ID `module.hash.parse_check_line.0`.
  Params: `line: Str`
- `hash.sha1(data: Bytes) -> Digest` - pure; Returns `Digest`. ID `module.hash.sha1.0`.
  Params: `data: Bytes`
- `hash.sha1(path: Path) -> Result[Digest, Error]` - effect; Returns `Digest` or `Error` failure data. ID `module.hash.sha1.1`.
  Params: `path: Path`
- `hash.sha256(data: Bytes) -> Digest` - pure; Returns `Digest`. ID `module.hash.sha256.0`.
  Params: `data: Bytes`
- `hash.sha256(path: Path) -> Result[Digest, Error]` - effect; Returns `Digest` or `Error` failure data. ID `module.hash.sha256.1`.
  Params: `path: Path`
- `hash.sha512(data: Bytes) -> Digest` - pure; Returns `Digest`. ID `module.hash.sha512.0`.
  Params: `data: Bytes`
- `hash.sha512(path: Path) -> Result[Digest, Error]` - effect; Returns `Digest` or `Error` failure data. ID `module.hash.sha512.1`.
  Params: `path: Path`
- `hash.verify_file(path: Path, sha256: Str = default) -> Result[Unit, Error]` - effect; Returns `Unit` or `Error` failure data. ID `module.hash.verify_file.0`.
  Params: `path: Path`, `sha256: Str = default`

### `ini`

INI decoding, encoding, and file helpers.

- `ini.decode(text: Str) -> Result[Record, Error]` - pure; Returns `Record` or `Error` failure data. ID `module.ini.decode.0`.
  Params: `text: Str`
- `ini.encode(value: Record) -> Result[Str, Error]` - pure; Returns `Str` or `Error` failure data. ID `module.ini.encode.0`.
  Params: `value: Record`
- `ini.read(path: Path) -> Result[Record, Error]` - effect; Returns `Record` or `Error` failure data. ID `module.ini.read.0`.
  Params: `path: Path`
- `ini.write(path: Path, value: Record, overwrite: Bool = default) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.ini.write.0`.
  Params: `path: Path`, `value: Record`, `overwrite: Bool = default`

### `io`

Script stdin and stdout helpers.

- `io.stdin_bytes() -> Result[Bytes, Error]` - effect; Returns `Bytes` or `Error` failure data. ID `module.io.stdin_bytes.0`.
- `io.stdin_line() -> Result[Str, Error]` - effect; Returns `Str` or `Error` failure data. ID `module.io.stdin_line.0`.
- `io.stdin_text() -> Result[Str, Error]` - effect; Returns `Str` or `Error` failure data. ID `module.io.stdin_text.0`.
- `io.write_stdout(text: Str) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.io.write_stdout.0`.
  Params: `text: Str`
- `io.write_stdout_bytes(data: Bytes) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.io.write_stdout_bytes.0`.
  Params: `data: Bytes`

### `json`

JSON encoding, decoding, files, and streams.

- `json.decode(s: Str) -> Result[Any, Error]` - pure; Returns `Any` or `Error` failure data. ID `module.json.decode.0`.
  Params: `s: Str`
- `json.encode(value: Any, pretty: Bool = default) -> Result[Str, Error]` - pure; Returns `Str` or `Error` failure data. ID `module.json.encode.0`.
  Params: `value: Any`, `pretty: Bool = default`
- `json.encode_lines(values: List[Any]) -> Result[Str, Error]` - pure; Returns `Str` or `Error` failure data. ID `module.json.encode_lines.0`.
  Params: `values: List[Any]`
- `json.get(value: Any, path: List[Any]) -> Result[Any, Error]` - pure; Returns `Any` or `Error` failure data. ID `module.json.get.0`.
  Params: `value: Any`, `path: List[Any]`
- `json.get(value: Any, path: List[Any], fallback: Any) -> Any` - pure; Returns `Any`. ID `module.json.get.1`.
  Params: `value: Any`, `path: List[Any]`, `fallback: Any`
- `json.read(path: Path) -> Result[Any, Error]` - effect; Returns `Any` or `Error` failure data. ID `module.json.read.0`.
  Params: `path: Path`
- `json.remove(value: Any, path: List[Any]) -> Result[Any, Error]` - pure; Returns `Any` or `Error` failure data. ID `module.json.remove.0`.
  Params: `value: Any`, `path: List[Any]`
- `json.set(value: Any, path: List[Any], replacement: Any) -> Result[Any, Error]` - pure; Returns `Any` or `Error` failure data. ID `module.json.set.0`.
  Params: `value: Any`, `path: List[Any]`, `replacement: Any`
- `json.write(path: Path, value: Any, pretty: Bool = default) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.json.write.0`.
  Params: `path: Path`, `value: Any`, `pretty: Bool = default`
- `json.write_lines(path: Path, values: List[Any]) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.json.write_lines.0`.
  Params: `path: Path`, `values: List[Any]`

### `linux`

Linux-specific boot, mount, device, and shutdown operations.

- `linux.add_default_ipv4_route(gateway: Str, interface: Str = default) -> Result[Unit, Error]` - effect; Returns `Unit` or `Error` failure data. ID `module.linux.add_default_ipv4_route.0`.
  Params: `gateway: Str`, `interface: Str = default`
- `linux.blkid(device: Path) -> Result[{label: Str, part_entry_uuid: Str, part_table_type: Str, type: Str, uuid: Str}, Error]` - effect; Returns `{label: Str, part_entry_uuid: Str, part_table_type: Str, type: Str, uuid: Str}` or `Error` failure data. ID `module.linux.blkid.0`.
  Params: `device: Path`
- `linux.block_devices() -> Result[Stream[{name: Str, partitioned: Bool, partitions: List[Path], path: Path, removable: Bool, rotational: Bool, sector_size: Int, sectors: Int, size: Int}], Error]` - effect; Returns a single-use stream or `Error` failure data. ID `module.linux.block_devices.0`.
- `linux.chroot(path: Path) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.linux.chroot.0`.
  Params: `path: Path`
- `linux.del_default_ipv4_route(gateway: Str, interface: Str) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.linux.del_default_ipv4_route.0`.
  Params: `gateway: Str`, `interface: Str`
- `linux.depmod(version: Str = default) -> Result[Unit, Error]` - effect; Returns `Unit` or `Error` failure data. ID `module.linux.depmod.0`.
  Params: `version: Str = default`
- `linux.dhcp_close(fd: Int) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.linux.dhcp_close.0`.
  Params: `fd: Int`
- `linux.dhcp_recv(fd: Int, timeout_ms: Int) -> Result[Bytes, Error]` - effect; Returns `Bytes` or `Error` failure data. ID `module.linux.dhcp_recv.0`.
  Params: `fd: Int`, `timeout_ms: Int`
- `linux.dhcp_send(fd: Int, payload: Bytes) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.linux.dhcp_send.0`.
  Params: `fd: Int`, `payload: Bytes`
- `linux.dhcp_send_release(interface: Str, address: Str, server_id: Str) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.linux.dhcp_send_release.0`.
  Params: `interface: Str`, `address: Str`, `server_id: Str`
- `linux.dhcp_socket(interface: Str) -> Result[Int, Error]` - effect; Returns `Int` or `Error` failure data. ID `module.linux.dhcp_socket.0`.
  Params: `interface: Str`
- `linux.disk_usage(path: Path = default) -> Result[Stream[{available: Int, device: Str, fstype: Str, mount: Str, total: Int, used: Int}], Error]` - effect; Returns a single-use stream or `Error` failure data. ID `module.linux.disk_usage.0`.
  Params: `path: Path = default`
- `linux.dmesg() -> Result[Stream[Str], Error]` - effect; Returns a single-use stream or `Error` failure data. ID `module.linux.dmesg.0`.
- `linux.file_attrs(path: Path) -> Result[{append_only: Bool, compression_requested: Bool, dirsync: Bool, flags: Int, immutable: Bool, indexed_directory: Bool, journaled_data: Bool, no_atime: Bool, no_dump: Bool, no_tailmerging: Bool, secure_deletion: Bool, sync: Bool, top_of_directory_hierarchies: Bool, undelete: Bool}, Error]` - effect; Returns `{append_only: Bool, compression_requested: Bool, dirsync: Bool, flags: Int, immutable: Bool, indexed_directory: Bool, journaled_data: Bool, no_atime: Bool, no_dump: Bool, no_tailmerging: Bool, secure_deletion: Bool, sync: Bool, top_of_directory_hierarchies: Bool, undelete: Bool}` or `Error` failure data. ID `module.linux.file_attrs.0`.
  Params: `path: Path`
- `linux.file_version(path: Path) -> Result[Int, Error]` - effect; Returns `Int` or `Error` failure data. ID `module.linux.file_version.0`.
  Params: `path: Path`
- `linux.flush_ipv4_addresses(interface: Str) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.linux.flush_ipv4_addresses.0`.
  Params: `interface: Str`
- `linux.fsck(device: Path, fstype: Str = default, repair: Bool = default) -> Result[{errors: List[Str], status: Int}, Error]` - effect; Returns `{errors: List[Str], status: Int}` or `Error` failure data. ID `module.linux.fsck.0`.
  Params: `device: Path`, `fstype: Str = default`, `repair: Bool = default`
- `linux.halt() -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.linux.halt.0`.
- `linux.hwclock() -> Result[Int, Error]` - effect; Returns `Int` or `Error` failure data. ID `module.linux.hwclock.0`.
- `linux.insmod(path: Path, params: Str = default) -> Result[Unit, Error]` - effect; Returns `Unit` or `Error` failure data. ID `module.linux.insmod.0`.
  Params: `path: Path`, `params: Str = default`
- `linux.interfaces() -> Result[Stream[{addresses: List[{addr: Str, family: Str, prefix_len: Int}], flags: List[Str], mac: Str, mtu: Int, name: Str}], Error]` - effect; Returns a single-use stream or `Error` failure data. ID `module.linux.interfaces.0`.
- `linux.is_mountpoint(path: Path) -> Result[Bool, Error]` - effect; Returns `Bool` or `Error` failure data. ID `module.linux.is_mountpoint.0`.
  Params: `path: Path`
- `linux.kill_all(signal: Str = default, except_pid1: Bool = default) -> Result[Unit, Error]` - effect; Returns `Unit` or `Error` failure data. ID `module.linux.kill_all.0`.
  Params: `signal: Str = default`, `except_pid1: Bool = default`
- `linux.link_down(interface: Str) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.linux.link_down.0`.
  Params: `interface: Str`
- `linux.link_up(interface: Str) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.linux.link_up.0`.
  Params: `interface: Str`
- `linux.loop_attach(file: Path, device: Path = default) -> Result[Path, Error]` - effect; Returns `Path` or `Error` failure data. ID `module.linux.loop_attach.0`.
  Params: `file: Path`, `device: Path = default`
- `linux.loop_detach(device: Path) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.linux.loop_detach.0`.
  Params: `device: Path`
- `linux.loop_list() -> Result[Stream[{device: Path, file: Path, offset: Int, size: Int}], Error]` - effect; Returns a single-use stream or `Error` failure data. ID `module.linux.loop_list.0`.
- `linux.meminfo() -> Result[{available: Int, buffers: Int, cached: Int, free: Int, swap_free: Int, swap_total: Int, total: Int}, Error]` - effect; Returns `{available: Int, buffers: Int, cached: Int, free: Int, swap_free: Int, swap_total: Int, total: Int}` or `Error` failure data. ID `module.linux.meminfo.0`.
- `linux.mknod(path: Path, kind: Str, major: Int, minor: Int) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.linux.mknod.0`.
  Params: `path: Path`, `kind: Str`, `major: Int`, `minor: Int`
- `linux.mkswap(device: Path) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.linux.mkswap.0`.
  Params: `device: Path`
- `linux.modinfo(name: Str) -> Result[{description: Str, filename: Path, license: Str, name: Str, params: List[{description: Str, name: Str, type: Str}], version: Str}, Error]` - effect; Returns `{description: Str, filename: Path, license: Str, name: Str, params: List[{description: Str, name: Str, type: Str}], version: Str}` or `Error` failure data. ID `module.linux.modinfo.0`.
  Params: `name: Str`
- `linux.modprobe(name: Str, params: Str = default) -> Result[Unit, Error]` - effect; Returns `Unit` or `Error` failure data. ID `module.linux.modprobe.0`.
  Params: `name: Str`, `params: Str = default`
- `linux.modules() -> Result[Stream[{name: Str, size: Int, used_by: List[Str]}], Error]` - effect; Returns a single-use stream or `Error` failure data. ID `module.linux.modules.0`.
- `linux.mount(source: Str, target: Path, fstype: Str = default, options: List[Str] = default) -> Result[Unit, Error]` - effect; Returns `Unit` or `Error` failure data. ID `module.linux.mount.0`.
  Params: `source: Str`, `target: Path`, `fstype: Str = default`, `options: List[Str] = default`
- `linux.mount_all() -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.linux.mount_all.0`.
- `linux.open_files(pid: Int = default) -> Result[Stream[{command: Str, fd: Int, inode: Int, local: Str, path: Path, pid: Int, protocol: Str, remote: Str, type: Str}], Error]` - effect; Returns a single-use stream or `Error` failure data. ID `module.linux.open_files.0`.
  Params: `pid: Int = default`
- `linux.partition_table(device: Path) -> Result[{id: Str, label: Str, partitions: List[{end: Int, index: Int, name: Str, size: Int, start: Int, type: Str, uuid: Str}], sector_size: Int}, Error]` - effect; Returns `{id: Str, label: Str, partitions: List[{end: Int, index: Int, name: Str, size: Int, start: Int, type: Str, uuid: Str}], sector_size: Int}` or `Error` failure data. ID `module.linux.partition_table.0`.
  Params: `device: Path`
- `linux.pivot_root(new_root: Path, put_old: Path) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.linux.pivot_root.0`.
  Params: `new_root: Path`, `put_old: Path`
- `linux.poweroff() -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.linux.poweroff.0`.
- `linux.read_device(device: Path, dest: Path, bytes: Int) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.linux.read_device.0`.
  Params: `device: Path`, `dest: Path`, `bytes: Int`
- `linux.reboot() -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.linux.reboot.0`.
- `linux.rfkill_block(id: Int) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.linux.rfkill_block.0`.
  Params: `id: Int`
- `linux.rfkill_list() -> Result[Stream[{hard_blocked: Bool, id: Int, name: Str, soft_blocked: Bool, type: Str}], Error]` - effect; Returns a single-use stream or `Error` failure data. ID `module.linux.rfkill_list.0`.
- `linux.rfkill_unblock(id: Int) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.linux.rfkill_unblock.0`.
  Params: `id: Int`
- `linux.rmmod(name: Str, force: Bool = default) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.linux.rmmod.0`.
  Params: `name: Str`, `force: Bool = default`
- `linux.root_device() -> Result[Str, Error]` - effect; Returns `Str` or `Error` failure data. ID `module.linux.root_device.0`.
- `linux.routes() -> Result[Stream[{dev: Str, dst: Str, family: Str, flags: List[Str], gateway: Str, metric: Int, prefix_len: Int}], Error]` - effect; Returns a single-use stream or `Error` failure data. ID `module.linux.routes.0`.
- `linux.set_file_attrs(path: Path, flags: Int) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.linux.set_file_attrs.0`.
  Params: `path: Path`, `flags: Int`
- `linux.set_file_version(path: Path, version: Int) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.linux.set_file_version.0`.
  Params: `path: Path`, `version: Int`
- `linux.set_hwclock(epoch_ms: Int) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.linux.set_hwclock.0`.
  Params: `epoch_ms: Int`
- `linux.set_ipv4_address(interface: Str, address: Str, netmask: Str) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.linux.set_ipv4_address.0`.
  Params: `interface: Str`, `address: Str`, `netmask: Str`
- `linux.set_system_clock(epoch_ms: Int) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.linux.set_system_clock.0`.
  Params: `epoch_ms: Int`
- `linux.swapoff(device: Path) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.linux.swapoff.0`.
  Params: `device: Path`
- `linux.swapoff_all() -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.linux.swapoff_all.0`.
- `linux.swapon(device: Path, priority: Int = default) -> Result[Unit, Error]` - effect; Returns `Unit` or `Error` failure data. ID `module.linux.swapon.0`.
  Params: `device: Path`, `priority: Int = default`
- `linux.swapon_all() -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.linux.swapon_all.0`.
- `linux.switch_root(new_root: Path, init: Path) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.linux.switch_root.0`.
  Params: `new_root: Path`, `init: Path`
- `linux.sysctl_get(key: Str) -> Result[Str, Error]` - effect; Returns `Str` or `Error` failure data. ID `module.linux.sysctl_get.0`.
  Params: `key: Str`
- `linux.sysctl_load_dirs(dirs: List[Path], fallback: Path = default) -> Result[Unit, Error]` - effect; Returns `Unit` or `Error` failure data. ID `module.linux.sysctl_load_dirs.0`.
  Params: `dirs: List[Path]`, `fallback: Path = default`
- `linux.sysctl_set(key: Str, value: Str) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.linux.sysctl_set.0`.
  Params: `key: Str`, `value: Str`
- `linux.uevent_stream() -> Result[Stream[{action: Str, devname: Str, devpath: Str, env: List[{name: Str, value: Str}], subsystem: Str}], Error]` - effect; Returns `Stream[{action: Str, devname: Str, devpath: Str, env: List[{name: Str, value: Str}], subsystem: Str}]` or `Error` failure data. ID `module.linux.uevent_stream.0`.
- `linux.umount_all(types: List[Str] = default) -> Result[Unit, Error]` - effect; Returns `Unit` or `Error` failure data. ID `module.linux.umount_all.0`.
  Params: `types: List[Str] = default`
- `linux.write_device(device: Path, source: Path) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.linux.write_device.0`.
  Params: `device: Path`, `source: Path`
- `linux.write_partition_table(device: Path, table: Record) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.linux.write_partition_table.0`.
  Params: `device: Path`, `table: Record`

### `map`

Map collection helpers.

- `map.empty() -> Map[Any]` - pure; Returns `Map[Any]`. ID `module.map.empty.0`.

### `mime`

MIME type lookup and media-type parsing helpers.

- `mime.lookup_ext(ext: Str) -> {exts: List[Str], mime: Str}?` - effect; Returns `{exts: List[Str], mime: Str}?`. ID `module.mime.lookup_ext.0`.
  Params: `ext: Str`
- `mime.lookup_path(path: Path) -> {exts: List[Str], mime: Str}?` - effect; Returns `{exts: List[Str], mime: Str}?`. ID `module.mime.lookup_path.0`.
  Params: `path: Path`
- `mime.parse(value: Str) -> Result[{params: Map[Str], type: Str}, Error]` - pure; Returns `{params: Map[Str], type: Str}` or `Error` failure data. ID `module.mime.parse.0`.
  Params: `value: Str`

### `module`

User module loading helpers.

- `module.load(path: Path) -> Result[Module, Error]` - effect; Returns `Module` or `Error` failure data. ID `module.module.load.0`.
  Params: `path: Path`

### `net`

HTTP request, transfer, and connection-pool helpers.

- `net.close_all_pools() -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.net.close_all_pools.0`.
- `net.close_pool(name: Str = default) -> Result[Unit, Error]` - effect; Returns `Unit` or `Error` failure data. ID `module.net.close_pool.0`.
  Params: `name: Str = default`
- `net.download(request: Record) -> Result[{bytes: Int, headers: List[{name: Str, value: Str}], reason: Str, status: Int, url: Str}, Error]` - effect; Returns `{bytes: Int, headers: List[{name: Str, value: Str}], reason: Str, status: Int, url: Str}` or `Error` failure data. ID `module.net.download.0`.
  Params: `request: Record`
- `net.download_many(batch: Record) -> Result[List[Result[{bytes: Int, headers: List[{name: Str, value: Str}], reason: Str, status: Int, url: Str}, Error]], Error]` - effect; Returns ordered download results or `Error` failure data. ID `module.net.download_many.0`.
  Params: `batch: Record`
- `net.pool(name: Str = default, max_idle_per_host: Int = default, idle_timeout: Duration = default) -> Result[{idle_timeout_ms: Int, max_idle_per_host: Int, name: Str}, Error]` - effect; Returns `{idle_timeout_ms: Int, max_idle_per_host: Int, name: Str}` or `Error` failure data. ID `module.net.pool.0`.
  Params: `name: Str = default`, `max_idle_per_host: Int = default`, `idle_timeout: Duration = default`
- `net.request(request: Record) -> Result[{body: Bytes, bytes: Int, headers: List[{name: Str, value: Str}], reason: Str, status: Int, url: Str}, Error]` - effect; Returns `{body: Bytes, bytes: Int, headers: List[{name: Str, value: Str}], reason: Str, status: Int, url: Str}` or `Error` failure data. ID `module.net.request.0`.
  Params: `request: Record`
- `net.request_many(batch: Record) -> Result[List[Result[{body: Bytes, bytes: Int, headers: List[{name: Str, value: Str}], reason: Str, status: Int, url: Str}, Error]], Error]` - effect; Returns ordered request results or `Error` failure data. ID `module.net.request_many.0`.
  Params: `batch: Record`
- `net.upload(request: Record) -> Result[{bytes: Int, headers: List[{name: Str, value: Str}], reason: Str, status: Int, url: Str}, Error]` - effect; Returns `{bytes: Int, headers: List[{name: Str, value: Str}], reason: Str, status: Int, url: Str}` or `Error` failure data. ID `module.net.upload.0`.
  Params: `request: Record`

### `patch`

Rooted patch application.

- `patch.apply(root: Path, text: Str, strip_components: Int = default, overwrite: Bool = default) -> Result[{files: Int, hunks: Int}, Error]` - effect; Returns `{files: Int, hunks: Int}` or `Error` failure data. ID `module.patch.apply.0`.
  Params: `root: Path`, `text: Str`, `strip_components: Int = default`, `overwrite: Bool = default`

### `path`

Path normalization and resolution.

- `path.absolute(path: Path) -> Result[Path, Error]` - effect; Returns `Path` or `Error` failure data. ID `module.path.absolute.0`.
  Params: `path: Path`

### `process`

Process discovery, command construction, execution, spawning, and signals.

- `process.argv_words(text: Str) -> Result[List[Str], Error]` - pure; Returns `List[Str]` or `Error` failure data. ID `module.process.argv_words.0`.
  Params: `text: Str`
- `process.command() -> Command` - effect; Returns `Command`. ID `module.process.command.0`.
- `process.command_argv(target: Str, argv: List[Str], cwd: Path = default, env: Record = default, stdin: Path = default, stdout: Path = default, stderr: Path = default, stdout_append: Bool = default, stderr_append: Bool = default, timeout: Duration = default, detach: Bool = default, new_session: Bool = default, ignore_hup: Bool = default, cpu_max: Int = default) -> Command` - pure; Returns `Command`. ID `module.process.command_argv.0`.
  Params: `target: Str`, `argv: List[Str]`, `cwd: Path = default`, `env: Record = default`, `stdin: Path = default`, `stdout: Path = default`, `stderr: Path = default`, `stdout_append: Bool = default`, `stderr_append: Bool = default`, `timeout: Duration = default`, `detach: Bool = default`, `new_session: Bool = default`, `ignore_hup: Bool = default`, `cpu_max: Int = default`
- `process.command_argv(target: Str, argv: List[Path], cwd: Path = default, env: Record = default, stdin: Path = default, stdout: Path = default, stderr: Path = default, stdout_append: Bool = default, stderr_append: Bool = default, timeout: Duration = default, detach: Bool = default, new_session: Bool = default, ignore_hup: Bool = default, cpu_max: Int = default) -> Command` - pure; Returns `Command`. ID `module.process.command_argv.1`.
  Params: `target: Str`, `argv: List[Path]`, `cwd: Path = default`, `env: Record = default`, `stdin: Path = default`, `stdout: Path = default`, `stderr: Path = default`, `stdout_append: Bool = default`, `stderr_append: Bool = default`, `timeout: Duration = default`, `detach: Bool = default`, `new_session: Bool = default`, `ignore_hup: Bool = default`, `cpu_max: Int = default`
- `process.command_argv(target: Path, argv: List[Str], cwd: Path = default, env: Record = default, stdin: Path = default, stdout: Path = default, stderr: Path = default, stdout_append: Bool = default, stderr_append: Bool = default, timeout: Duration = default, detach: Bool = default, new_session: Bool = default, ignore_hup: Bool = default, cpu_max: Int = default) -> Command` - pure; Returns `Command`. ID `module.process.command_argv.2`.
  Params: `target: Path`, `argv: List[Str]`, `cwd: Path = default`, `env: Record = default`, `stdin: Path = default`, `stdout: Path = default`, `stderr: Path = default`, `stdout_append: Bool = default`, `stderr_append: Bool = default`, `timeout: Duration = default`, `detach: Bool = default`, `new_session: Bool = default`, `ignore_hup: Bool = default`, `cpu_max: Int = default`
- `process.command_argv(target: Path, argv: List[Path], cwd: Path = default, env: Record = default, stdin: Path = default, stdout: Path = default, stderr: Path = default, stdout_append: Bool = default, stderr_append: Bool = default, timeout: Duration = default, detach: Bool = default, new_session: Bool = default, ignore_hup: Bool = default, cpu_max: Int = default) -> Command` - pure; Returns `Command`. ID `module.process.command_argv.3`.
  Params: `target: Path`, `argv: List[Path]`, `cwd: Path = default`, `env: Record = default`, `stdin: Path = default`, `stdout: Path = default`, `stderr: Path = default`, `stdout_append: Bool = default`, `stderr_append: Bool = default`, `timeout: Duration = default`, `detach: Bool = default`, `new_session: Bool = default`, `ignore_hup: Bool = default`, `cpu_max: Int = default`
- `process.current_pid() -> Result[Int, Error]` - effect; Returns `Int` or `Error` failure data. ID `module.process.current_pid.0`.
- `process.kill(pid: Int, signal: Str = default) -> Result[Unit, Error]` - effect; Returns `Unit` or `Error` failure data. ID `module.process.kill.0`.
  Params: `pid: Int`, `signal: Str = default`
- `process.list() -> Result[Stream[{argv: Str, argv0: Str, command: Str, parent_pid: Int, pid: Int, runtime_seconds: Int, start_time: Str, start_time_ms: Int, status: Str, uid: Int, user: Str}], Error]` - effect; Returns `Stream[{argv: Str, argv0: Str, command: Str, parent_pid: Int, pid: Int, runtime_seconds: Int, start_time: Str, start_time_ms: Int, status: Str, uid: Int, user: Str}]` or `Error` failure data. ID `module.process.list.0`.
- `process.port(port: Int) -> Result[Stream[{argv: Str, argv0: Str, command: Str, fd: Int, inode: Int, local: Str, local_address: Str, local_port: Int, parent_pid: Int, pid: Int, protocol: Str, remote: Str, remote_address: Str, remote_port: Int, state: Str, uid: Int, user: Str}], Error]` - effect; Returns `Stream[{argv: Str, argv0: Str, command: Str, fd: Int, inode: Int, local: Str, local_address: Str, local_port: Int, parent_pid: Int, pid: Int, protocol: Str, remote: Str, remote_address: Str, remote_port: Int, state: Str, uid: Int, user: Str}]` or `Error` failure data. ID `module.process.port.0`.
  Params: `port: Int`
- `process.ports() -> Result[Stream[{argv: Str, argv0: Str, command: Str, fd: Int, inode: Int, local: Str, local_address: Str, local_port: Int, parent_pid: Int, pid: Int, protocol: Str, remote: Str, remote_address: Str, remote_port: Int, state: Str, uid: Int, user: Str}], Error]` - effect; Returns `Stream[{argv: Str, argv0: Str, command: Str, fd: Int, inode: Int, local: Str, local_address: Str, local_port: Int, parent_pid: Int, pid: Int, protocol: Str, remote: Str, remote_address: Str, remote_port: Int, state: Str, uid: Int, user: Str}]` or `Error` failure data. ID `module.process.ports.0`.
- `process.ports(pid: Int) -> Result[Stream[{argv: Str, argv0: Str, command: Str, fd: Int, inode: Int, local: Str, local_address: Str, local_port: Int, parent_pid: Int, pid: Int, protocol: Str, remote: Str, remote_address: Str, remote_port: Int, state: Str, uid: Int, user: Str}], Error]` - effect; Returns `Stream[{argv: Str, argv0: Str, command: Str, fd: Int, inode: Int, local: Str, local_address: Str, local_port: Int, parent_pid: Int, pid: Int, protocol: Str, remote: Str, remote_address: Str, remote_port: Int, state: Str, uid: Int, user: Str}]` or `Error` failure data. ID `module.process.ports.1`.
  Params: `pid: Int`
- `process.run(command: Command) -> Result[Status, ProcessError]` - effect; Returns `Status` or `ProcessError` failure data. ID `module.process.run.0`.
  Params: `command: Command`
- `process.signal(signal: Str) -> Result[{name: Str, number: Int}, Error]` - effect; Returns `{name: Str, number: Int}` or `Error` failure data. ID `module.process.signal.0`.
  Params: `signal: Str`
- `process.spawn(command: Command) -> Result[{argv: Str, command: Str, detach: Bool, ignore_hup: Bool, new_session: Bool, pid: Int}, Error]` - effect; Returns `{argv: Str, command: Str, detach: Bool, ignore_hup: Bool, new_session: Bool, pid: Int}` or `Error` failure data. ID `module.process.spawn.0`.
  Params: `command: Command`
- `process.stats(pid: Int) -> Result[{rss_kb: Int, vsz_kb: Int}, Error]` - effect; Returns `{rss_kb: Int, vsz_kb: Int}` or `Error` failure data. ID `module.process.stats.0`.
  Params: `pid: Int`
- `process.threads() -> Result[Stream[{argv: Str, argv0: Str, command: Str, owner_pid: Int, parent_pid: Int, pid: Int, runtime_seconds: Int, start_time: Str, start_time_ms: Int, status: Str, thread_id: Int, thread_name: Str, uid: Int, user: Str}], Error]` - effect; Returns `Stream[{argv: Str, argv0: Str, command: Str, owner_pid: Int, parent_pid: Int, pid: Int, runtime_seconds: Int, start_time: Str, start_time_ms: Int, status: Str, thread_id: Int, thread_name: Str, uid: Int, user: Str}]` or `Error` failure data. ID `module.process.threads.0`.
- `process.threads(pid: Int) -> Result[Stream[{argv: Str, argv0: Str, command: Str, owner_pid: Int, parent_pid: Int, pid: Int, runtime_seconds: Int, start_time: Str, start_time_ms: Int, status: Str, thread_id: Int, thread_name: Str, uid: Int, user: Str}], Error]` - effect; Returns `Stream[{argv: Str, argv0: Str, command: Str, owner_pid: Int, parent_pid: Int, pid: Int, runtime_seconds: Int, start_time: Str, start_time_ms: Int, status: Str, thread_id: Int, thread_name: Str, uid: Int, user: Str}]` or `Error` failure data. ID `module.process.threads.1`.
  Params: `pid: Int`
- `process.wait_any(handles: List[ProcessHandle]) -> Result[{index: Int, pid: Int, status: Status}, ProcessError]` - effect; Returns `{index: Int, pid: Int, status: Status}` or `ProcessError` failure data. ID `module.process.wait_any.0`.
  Params: `handles: List[ProcessHandle]`
- `process.wait_ready(handles: List[ProcessHandle]) -> Result[List[{index: Int, pid: Int, status: Status}], ProcessError]` - effect; Returns `List[{index: Int, pid: Int, status: Status}]` or `ProcessError` failure data. ID `module.process.wait_ready.0`.
  Params: `handles: List[ProcessHandle]`
- `process.which(name: Str) -> Result[Path, Error]` - effect; Returns `Path` or `Error` failure data. ID `module.process.which.0`.
  Params: `name: Str`

### `record`

Record inspection helpers.

- `record.require(record: Record, required: Record, optional: Record = default, source: Path = default) -> Result[Record, Error]` - effect; Returns `Record` or `Error` failure data. ID `module.record.require.0`.
  Params: `record: Record`, `required: Record`, `optional: Record = default`, `source: Path = default`

### `regex`

Regex compilation, matching, captures, and replacement.

- `regex.compile(pattern: Str) -> Result[Regex, Error]` - pure; Returns `Regex` or `Error` failure data. ID `module.regex.compile.0`.
  Params: `pattern: Str`

### `set`

String-key set helpers backed by Map[Bool].

- `set.add(set: Map[Bool], item: Str) -> Map[Bool]` - pure; Returns `Map[Bool]`. ID `module.set.add.0`.
  Params: `set: Map[Bool]`, `item: Str`
- `set.empty() -> Map[Bool]` - pure; Returns `Map[Bool]`. ID `module.set.empty.0`.
- `set.from(items: List[Str]) -> Map[Bool]` - pure; Returns `Map[Bool]`. ID `module.set.from.0`.
  Params: `items: List[Str]`
- `set.has(set: Map[Bool], item: Str) -> Bool` - pure; Returns `Bool`. ID `module.set.has.0`.
  Params: `set: Map[Bool]`, `item: Str`
- `set.remove(set: Map[Bool], item: Str) -> Map[Bool]` - pure; Returns `Map[Bool]`. ID `module.set.remove.0`.
  Params: `set: Map[Bool]`, `item: Str`

### `shlex`

POSIX-like shell word rendering helpers.

- `shlex.join(argv: List[Str]) -> Str` - pure; Returns `Str`. ID `module.shlex.join.0`.
  Params: `argv: List[Str]`
- `shlex.quote(value: Str) -> Str` - pure; Returns `Str`. ID `module.shlex.quote.0`.
  Params: `value: Str`

### `system`

Host system identity records.

- `system.hostname() -> Result[Str, Error]` - effect; Returns `Str` or `Error` failure data. ID `module.system.hostname.0`.
- `system.memory() -> Result[{available: Int, free: Int, swap_free: Int, swap_total: Int, total: Int}, Error]` - effect; Returns `{available: Int, free: Int, swap_free: Int, swap_total: Int, total: Int}` or `Error` failure data. ID `module.system.memory.0`.
- `system.os_release() -> Result[{id: Str, name: Str, pretty_name: Str, version: Str, version_id: Str}, Error]` - effect; Returns `{id: Str, name: Str, pretty_name: Str, version: Str, version_id: Str}` or `Error` failure data. ID `module.system.os_release.0`.
- `system.uname() -> Result[{machine: Str, nodename: Str, release: Str, sysname: Str, version: Str}, Error]` - effect; Returns `{machine: Str, nodename: Str, release: Str, sysname: Str, version: Str}` or `Error` failure data. ID `module.system.uname.0`.

### `test`

Native XSH test assertions, temp resources, and host-effect mocks.

- `test.calls(ctx: {file: Path, name: Str, temp_root: Path}, op: Str = default) -> List[{args: Record, op: Str}]` - effect; Returns `List[{args: Record, op: Str}]`. ID `module.test.calls.0`.
  Params: `ctx: {file: Path, name: Str, temp_root: Path}`, `op: Str = default`
- `test.contains(haystack: Any, needle: Any, message: Str = default) -> Result[Unit, Error]` - pure; Returns `Unit` or `Error` failure data. ID `module.test.contains.0`.
  Params: `haystack: Any`, `needle: Any`, `message: Str = default`
- `test.eq(left: Any, right: Any, message: Str = default) -> Result[Unit, Error]` - pure; Returns `Unit` or `Error` failure data. ID `module.test.eq.0`.
  Params: `left: Any`, `right: Any`, `message: Str = default`
- `test.error_kind(value: Any, kind: Str, message: Str = default) -> Result[Unit, Error]` - pure; Returns `Unit` or `Error` failure data. ID `module.test.error_kind.0`.
  Params: `value: Any`, `kind: Str`, `message: Str = default`
- `test.fail(message: Str = default) -> Result[Unit, Error]` - pure; Returns `Unit` or `Error` failure data. ID `module.test.fail.0`.
  Params: `message: Str = default`
- `test.mock(ctx: {file: Path, name: Str, temp_root: Path}, op: Str, matcher: Record, result: Any, times: Int = default) -> Result[Unit, Error]` - effect; Returns `Unit` or `Error` failure data. ID `module.test.mock.0`.
  Params: `ctx: {file: Path, name: Str, temp_root: Path}`, `op: Str`, `matcher: Record`, `result: Any`, `times: Int = default`
- `test.ne(left: Any, right: Any, message: Str = default) -> Result[Unit, Error]` - pure; Returns `Unit` or `Error` failure data. ID `module.test.ne.0`.
  Params: `left: Any`, `right: Any`, `message: Str = default`
- `test.not_contains(haystack: Any, needle: Any, message: Str = default) -> Result[Unit, Error]` - pure; Returns `Unit` or `Error` failure data. ID `module.test.not_contains.0`.
  Params: `haystack: Any`, `needle: Any`, `message: Str = default`
- `test.ok(condition: Bool, message: Str = default) -> Result[Unit, Error]` - pure; Returns `Unit` or `Error` failure data. ID `module.test.ok.0`.
  Params: `condition: Bool`, `message: Str = default`
- `test.run_script(ctx: {file: Path, name: Str, temp_root: Path}, source: Str, args: List[Str] = default, env: Record = default, stdin: Bytes = default, name: Str = default) -> Result[{status: Int, stderr: Str, stderr_bytes: Bytes, stdout: Str, stdout_bytes: Bytes, success: Bool}, Error]` - effect; Returns `{status: Int, stderr: Str, stderr_bytes: Bytes, stdout: Str, stdout_bytes: Bytes, success: Bool}` or `Error` failure data. ID `module.test.run_script.0`.
- `test.run_xsh(ctx: {file: Path, name: Str, temp_root: Path}, source: Str, xsh_args: List[Str] = default, script_args: List[Str] = default, env: Record = default, stdin: Bytes = default, name: Str = default) -> Result[{status: Int, stderr: Str, stderr_bytes: Bytes, stdout: Str, stdout_bytes: Bytes, success: Bool}, Error]` - effect; Returns `{status: Int, stderr: Str, stderr_bytes: Bytes, stdout: Str, stdout_bytes: Bytes, success: Bool}` or `Error` failure data. ID `module.test.run_xsh.0`.
- `test.run_xsht_trace(ctx: {file: Path, name: Str, temp_root: Path}, source: Str, trace_args: List[Str] = default, script_args: List[Str] = default, env: Record = default, stdin: Bytes = default, name: Str = default) -> Result[{status: Int, stderr: Str, stderr_bytes: Bytes, stdout: Str, stdout_bytes: Bytes, success: Bool}, Error]` - effect; Returns `{status: Int, stderr: Str, stderr_bytes: Bytes, stdout: Str, stdout_bytes: Bytes, success: Bool}` or `Error` failure data. ID `module.test.run_xsht_trace.0`.
  Params: `ctx: {file: Path, name: Str, temp_root: Path}`, `source: Str`, `args: List[Str] = default`, `env: Record = default`, `stdin: Bytes = default`, `name: Str = default`
- `test.skip(message: Str = default) -> Result[Unit, Error]` - pure; Returns `Unit` or `Error` failure data. ID `module.test.skip.0`.
  Params: `message: Str = default`
- `test.temp_dir(ctx: {file: Path, name: Str, temp_root: Path}, name: Str = default) -> Result[Path, Error]` - effect; Returns `Path` or `Error` failure data. ID `module.test.temp_dir.0`.
  Params: `ctx: {file: Path, name: Str, temp_root: Path}`, `name: Str = default`
- `test.temp_file(ctx: {file: Path, name: Str, temp_root: Path}, name: Str = default, contents: Bytes = default) -> Result[Path, Error]` - effect; Returns `Path` or `Error` failure data. ID `module.test.temp_file.0`.
  Params: `ctx: {file: Path, name: Str, temp_root: Path}`, `name: Str = default`, `contents: Bytes = default`
- `test.temp_path(ctx: {file: Path, name: Str, temp_root: Path}, name: Str = default) -> Path` - effect; Returns `Path`. ID `module.test.temp_path.0`.
  Params: `ctx: {file: Path, name: Str, temp_root: Path}`, `name: Str = default`

### `time`

Clock, sleep, command measurement, and Jiff strtime formatting.

- `time.duration_compact(seconds: Int) -> Str` - pure; Returns `Str`. ID `module.time.duration_compact.0`.
  Params: `seconds: Int`
- `time.format(epoch_ms: Int, format: Str, utc: Bool = default) -> Result[Str, Error]` - effect; Returns `Str` or `Error` failure data. ID `module.time.format.0`.
  Params: `epoch_ms: Int`, `format: Str`, `utc: Bool = default`
- `time.measure(command: Command, quiet: Bool = default) -> Result[{duration_ms: Int, status: Status, system_ns: Int, user_ns: Int, wall_ns: Int}, Error]` - effect; Returns `{duration_ms: Int, status: Status, system_ns: Int, user_ns: Int, wall_ns: Int}` or `Error` failure data. ID `module.time.measure.0`.
  Params: `command: Command`, `quiet: Bool = default`
- `time.millis(ms: Int) -> Duration` - pure; Returns `Duration`. ID `module.time.millis.0`.
  Params: `ms: Int`
- `time.now() -> Int` - effect; Returns `Int`. ID `module.time.now.0`.
- `time.seconds(seconds: Int) -> Duration` - pure; Returns `Duration`. ID `module.time.seconds.0`.
  Params: `seconds: Int`
- `time.sleep(duration: Duration) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.time.sleep.0`.
  Params: `duration: Duration`

### `tui`

Terminal styling, control sequences, and width-aware text padding.

- `tui.blue() -> Str` - pure; Returns `Str`. ID `module.tui.blue.0`.
- `tui.bold() -> Str` - pure; Returns `Str`. ID `module.tui.bold.0`.
- `tui.clear() -> Str` - pure; Returns `Str`. ID `module.tui.clear.0`.
- `tui.cyan() -> Str` - pure; Returns `Str`. ID `module.tui.cyan.0`.
- `tui.dim() -> Str` - pure; Returns `Str`. ID `module.tui.dim.0`.
- `tui.erase_line() -> Str` - pure; Returns `Str`. ID `module.tui.erase_line.0`.
- `tui.gray() -> Str` - pure; Returns `Str`. ID `module.tui.gray.0`.
- `tui.green() -> Str` - pure; Returns `Str`. ID `module.tui.green.0`.
- `tui.hide_cursor() -> Str` - pure; Returns `Str`. ID `module.tui.hide_cursor.0`.
- `tui.home() -> Str` - pure; Returns `Str`. ID `module.tui.home.0`.
- `tui.left_pad(text: Str, width: Int) -> Str` - pure; Returns `Str`. ID `module.tui.left_pad.0`.
  Params: `text: Str`, `width: Int`
- `tui.magenta() -> Str` - pure; Returns `Str`. ID `module.tui.magenta.0`.
- `tui.read_secret(prompt: Str) -> Result[Str, Error]` - effect; Returns `Str` or `Error` failure data. ID `module.tui.read_secret.0`.
  Params: `prompt: Str`
- `tui.red() -> Str` - pure; Returns `Str`. ID `module.tui.red.0`.
- `tui.reset() -> Str` - pure; Returns `Str`. ID `module.tui.reset.0`.
- `tui.right_pad(text: Str, width: Int) -> Str` - pure; Returns `Str`. ID `module.tui.right_pad.0`.
  Params: `text: Str`, `width: Int`
- `tui.show_cursor() -> Str` - pure; Returns `Str`. ID `module.tui.show_cursor.0`.
- `tui.white() -> Str` - pure; Returns `Str`. ID `module.tui.white.0`.
- `tui.yellow() -> Str` - pure; Returns `Str`. ID `module.tui.yellow.0`.

### `unix`

Unix process-group, PID 1, hostname, uptime, exec, and reaping helpers.

- `unix.exec(command: Command) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.unix.exec.0`.
  Params: `command: Command`
- `unix.id() -> Result[{egid: Int, euid: Int, gid: Int, groups: List[{gid: Int, name: Str}], uid: Int}, Error]` - effect; Returns `{egid: Int, euid: Int, gid: Int, groups: List[{gid: Int, name: Str}], uid: Int}` or `Error` failure data. ID `module.unix.id.0`.
- `unix.kill_all(name: Str, signal: Str = default) -> Result[{matched: Int, signaled: Int}, Error]` - effect; Returns `{matched: Int, signaled: Int}` or `Error` failure data. ID `module.unix.kill_all.0`.
  Params: `name: Str`, `signal: Str = default`
- `unix.kill_process_group(pid: Int, signal: Str) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.unix.kill_process_group.0`.
  Params: `pid: Int`, `signal: Str`
- `unix.notify_close(fd: Int) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.unix.notify_close.0`.
  Params: `fd: Int`
- `unix.notify_ready(fd: Int) -> Result[Bool, Error]` - effect; Returns `Bool` or `Error` failure data. ID `module.unix.notify_ready.0`.
  Params: `fd: Int`
- `unix.pid1_setup(signals: List[Str], subreaper: Bool = default, allow_non_pid1: Bool = default) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.unix.pid1_setup.0`.
  Params: `signals: List[Str]`, `subreaper: Bool = default`, `allow_non_pid1: Bool = default`
- `unix.reap_child_events() -> Result[Stream[{pid: Int, status: Status}], Error]` - effect; Returns a single-use stream of currently available events or `Error` failure data. ID `module.unix.reap_child_events.0`.
- `unix.set_hostname(hostname: Str) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.unix.set_hostname.0`.
  Params: `hostname: Str`
- `unix.set_tty_attrs(attrs: Record, fd: Int = default) -> Result[Unit, Error]` - effect; Returns `Unit` or `Error` failure data. ID `module.unix.set_tty_attrs.0`.
  Params: `attrs: Record`, `fd: Int = default`
- `unix.shutdown_process_groups(groups: List[Int], term_timeout: Duration, kill_timeout: Duration = default) -> Result[{kill_sent: Int, reaped: List[{pid: Int, status: Status}], remaining: List[Int], term_sent: Int}, Error]` - effect; Returns `{kill_sent: Int, reaped: List[{pid: Int, status: Status}], remaining: List[Int], term_sent: Int}` or `Error` failure data. ID `module.unix.shutdown_process_groups.0`.
  Params: `groups: List[Int]`, `term_timeout: Duration`, `kill_timeout: Duration = default`
- `unix.spawn_logged_process_group(command: Command, logger: Command) -> Result[{argv: List[Str], command: Str, detach: Bool, ignore_hup: Bool, log_pid: Int, new_session: Bool, pid: Int}, Error]` - effect; Returns `{argv: List[Str], command: Str, detach: Bool, ignore_hup: Bool, log_pid: Int, new_session: Bool, pid: Int}` or `Error` failure data. ID `module.unix.spawn_logged_process_group.0`.
  Params: `command: Command`, `logger: Command`
- `unix.spawn_process_group(command: Command, notify: Bool = default) -> Result[{argv: List[Str], command: Str, detach: Bool, ignore_hup: Bool, new_session: Bool, notify_fd: Int, pid: Int}, Error]` - effect; Returns `{argv: List[Str], command: Str, detach: Bool, ignore_hup: Bool, new_session: Bool, notify_fd: Int, pid: Int}` or `Error` failure data. ID `module.unix.spawn_process_group.0`.
  Params: `command: Command`, `notify: Bool = default`
- `unix.spawn_process_group_log(command: Command, log: Path, notify: Bool = default) -> Result[{argv: List[Str], command: Str, detach: Bool, ignore_hup: Bool, new_session: Bool, notify_fd: Int, pid: Int}, Error]` - effect; Returns `{argv: List[Str], command: Str, detach: Bool, ignore_hup: Bool, new_session: Bool, notify_fd: Int, pid: Int}` or `Error` failure data. ID `module.unix.spawn_process_group_log.0`.
  Params: `command: Command`, `log: Path`, `notify: Bool = default`
- `unix.spawn_with_tty(command: Command, tty: Str) -> Result[{argv: List[Str], command: Str, detach: Bool, ignore_hup: Bool, new_session: Bool, notify_fd: Int, pid: Int}, Error]` - effect; Returns `{argv: List[Str], command: Str, detach: Bool, ignore_hup: Bool, new_session: Bool, notify_fd: Int, pid: Int}` or `Error` failure data. ID `module.unix.spawn_with_tty.0`.
  Params: `command: Command`, `tty: Str`
- `unix.tty() -> Result[Str, Error]` - effect; Returns `Str` or `Error` failure data. ID `module.unix.tty.0`.
- `unix.tty_attrs(fd: Int = default) -> Result[{cflag: Int, control_chars: List[Int], crnl: Bool, echo: Bool, iflag: Int, ispeed: Int, lflag: Int, oflag: Int, ospeed: Int, raw: Bool}, Error]` - effect; Returns `{cflag: Int, control_chars: List[Int], crnl: Bool, echo: Bool, iflag: Int, ispeed: Int, lflag: Int, oflag: Int, ospeed: Int, raw: Bool}` or `Error` failure data. ID `module.unix.tty_attrs.0`.
  Params: `fd: Int = default`
- `unix.uptime_seconds() -> Result[Int, Error]` - effect; Returns `Int` or `Error` failure data. ID `module.unix.uptime_seconds.0`.
- `unix.wait_pid1_event(timeout: Duration = default) -> Result[{children: List[{pid: Int, status: Status}], kind: Str, signal: Str}, Error]` - effect; Returns `{children: List[{pid: Int, status: Status}], kind: Str, signal: Str}` or `Error` failure data. ID `module.unix.wait_pid1_event.0`.
  Params: `timeout: Duration = default`

### `user`

Unix user lookup records.

- `user.add(name: Str, uid: Int = default, gid: Int = default, home: Path = default, shell: Path = default, gecos: Str = default) -> Result[{gid: Int, home: Path, name: Str, shell: Str, uid: Int}, Error]` - effect; Returns `{gid: Int, home: Path, name: Str, shell: Str, uid: Int}` or `Error` failure data. ID `module.user.add.0`.
  Params: `name: Str`, `uid: Int = default`, `gid: Int = default`, `home: Path = default`, `shell: Path = default`, `gecos: Str = default`
- `user.by_uid(uid: Int) -> Result[{gid: Int, home: Path, name: Str, shell: Str, uid: Int}, Error]` - effect; Returns `{gid: Int, home: Path, name: Str, shell: Str, uid: Int}` or `Error` failure data. ID `module.user.by_uid.0`.
  Params: `uid: Int`
- `user.current() -> Result[{gid: Int, home: Path, name: Str, shell: Str, uid: Int}, Error]` - effect; Returns `{gid: Int, home: Path, name: Str, shell: Str, uid: Int}` or `Error` failure data. ID `module.user.current.0`.
- `user.lookup(name: Str) -> Result[{gid: Int, home: Path, name: Str, shell: Str, uid: Int}, Error]` - effect; Returns `{gid: Int, home: Path, name: Str, shell: Str, uid: Int}` or `Error` failure data. ID `module.user.lookup.0`.
  Params: `name: Str`
- `user.remove(name: Str, remove_home: Bool = default) -> Result[Unit, Error]` - effect, command; Returns `Unit` or `Error` failure data. ID `module.user.remove.0`.
  Params: `name: Str`, `remove_home: Bool = default`

### `utils`

Process-scoped utility helpers.

- `utils.cache(fn: Any, args: List[Any] = default) -> Any` - effect; Returns `Any`. ID `module.utils.cache.0`.
  Params: `fn: Any`, `args: List[Any] = default`

## Method Index

- `Bytes` - 21 method(s)
- `Digest` - 2 method(s)
- `EnvPathList` - 3 method(s)
- `Float` - 13 method(s)
- `Int` - 1 method(s)
- `List` - 6 method(s)
- `Map` - 8 method(s)
- `Path` - 31 method(s)
- `PathConstructor` - 1 method(s)
- `ProcessHandle` - 1 method(s)
- `Record` - 3 method(s)
- `Regex` - 4 method(s)
- `Result` - 1 method(s)
- `Status` - 5 method(s)
- `Str` - 28 method(s)
- `Stream` - 1 method(s)

## Methods

### `Bytes` Methods

- `Bytes.base32() -> Str` - pure; Returns `Str`. ID `method.Bytes.base32.0`.
- `Bytes.base64() -> Str` - pure; Returns `Str`. ID `method.Bytes.base64.0`.
- `Bytes.byte_at(index: Int, default: Int = default) -> Int` - pure; Returns `Int`. ID `method.Bytes.byte_at.0`.
  Params: `index: Int`, `default: Int = default`
- `Bytes.chunks(size: Int) -> List[Bytes]` - pure; Returns `List[Bytes]`. ID `method.Bytes.chunks.0`.
  Params: `size: Int`
- `Bytes.compare(other: Bytes) -> {byte: Int, equal: Bool, left: Int, line: Int, right: Int}` - pure; Returns `{byte: Int, equal: Bool, left: Int, line: Int, right: Int}`. ID `method.Bytes.compare.0`.
  Params: `other: Bytes`
- `Bytes.contains(needle: Bytes) -> Bool` - pure; Returns `Bool`. ID `method.Bytes.contains.0`.
  Params: `needle: Bytes`
- `Bytes.count_lines() -> Int` - pure; Returns `Int`. ID `method.Bytes.count_lines.0`.
- `Bytes.dump(format: Str = default) -> Str` - pure; Returns `Str`. ID `method.Bytes.dump.0`.
  Params: `format: Str = default`
- `Bytes.ends_with(suffix: Bytes) -> Bool` - pure; Returns `Bool`. ID `method.Bytes.ends_with.0`.
  Params: `suffix: Bytes`
- `Bytes.len() -> Int` - pure; Returns `Int`. ID `method.Bytes.len.0`.
- `Bytes.lines() -> Stream[Bytes]` - pure; Returns `Stream[Bytes]`. ID `method.Bytes.lines.0`.
- `Bytes.lower() -> Bytes` - pure; Returns `Bytes`. ID `method.Bytes.lower.0`.
- `Bytes.md5() -> Digest` - pure; Returns `Digest`. ID `method.Bytes.md5.0`.
- `Bytes.sha1() -> Digest` - pure; Returns `Digest`. ID `method.Bytes.sha1.0`.
- `Bytes.sha256() -> Digest` - pure; Returns `Digest`. ID `method.Bytes.sha256.0`.
- `Bytes.sha512() -> Digest` - pure; Returns `Digest`. ID `method.Bytes.sha512.0`.
- `Bytes.slice(offset: Int, length: Int = default) -> Bytes` - pure; Returns `Bytes`. ID `method.Bytes.slice.0`.
  Params: `offset: Int`, `length: Int = default`
- `Bytes.starts_with(prefix: Bytes) -> Bool` - pure; Returns `Bool`. ID `method.Bytes.starts_with.0`.
  Params: `prefix: Bytes`
- `Bytes.strings(min_len: Int = default) -> List[Str]` - pure; Returns `List[Str]`. ID `method.Bytes.strings.0`.
  Params: `min_len: Int = default`
- `Bytes.trim() -> Bytes` - pure; Returns `Bytes`. ID `method.Bytes.trim.0`.
- `Bytes.utf8() -> Result[Str, Error]` - pure; Returns `Str` or `Error` failure data. ID `method.Bytes.utf8.0`.

### `Digest` Methods

- `Digest.base64() -> Str` - pure; Returns `Str`. ID `method.Digest.base64.0`.
- `Digest.hex() -> Str` - pure; Returns `Str`. ID `method.Digest.hex.0`.

### `EnvPathList` Methods

- `EnvPathList.append(path: Path) -> Result[Unit, Error]` - effect; Returns `Unit` or `Error` failure data. ID `method.EnvPathList.append.0`.
  Params: `path: Path`
- `EnvPathList.pop() -> Result[Path, Error]` - effect; Returns `Path` or `Error` failure data. ID `method.EnvPathList.pop.0`.
- `EnvPathList.prepend(path: Path) -> Result[Unit, Error]` - effect; Returns `Unit` or `Error` failure data. ID `method.EnvPathList.prepend.0`.
  Params: `path: Path`

### `Float` Methods

- `Float.abs() -> Float` - pure; Returns `Float`. ID `method.Float.abs.0`.
- `Float.ceil() -> Result[Int, Error]` - pure; Returns `Int` or `Error` failure data. ID `method.Float.ceil.0`.
- `Float.cos() -> Float` - pure; Returns `Float`. ID `method.Float.cos.0`.
- `Float.exp() -> Float` - pure; Returns `Float`. ID `method.Float.exp.0`.
- `Float.floor() -> Result[Int, Error]` - pure; Returns `Int` or `Error` failure data. ID `method.Float.floor.0`.
- `Float.format(precision: Int = default) -> Str` - pure; Returns `Str`. ID `method.Float.format.0`.
  Params: `precision: Int = default`
- `Float.ln() -> Float` - pure; Returns `Float`. ID `method.Float.ln.0`.
- `Float.log(base: Float) -> Float` - pure; Returns `Float`. ID `method.Float.log.0`.
  Params: `base: Float`
- `Float.pow(exp: Float) -> Float` - pure; Returns `Float`. ID `method.Float.pow.0`.
  Params: `exp: Float`
- `Float.round() -> Result[Int, Error]` - pure; Returns `Int` or `Error` failure data. ID `method.Float.round.0`.
- `Float.sin() -> Float` - pure; Returns `Float`. ID `method.Float.sin.0`.
- `Float.sqrt() -> Float` - pure; Returns `Float`. ID `method.Float.sqrt.0`.
- `Float.tan() -> Float` - pure; Returns `Float`. ID `method.Float.tan.0`.

### `Int` Methods

- `Int.float() -> Float` - pure; Returns `Float`. ID `method.Int.float.0`.

### `List` Methods

- `List.contains(item: Any) -> Bool` - pure; Returns `Bool`. ID `method.List.contains.0`.
  Params: `item: Any`
- `List.extend(other: List[Any]) -> List[Any]` - pure; Returns `List[Any]`. ID `method.List.extend.0`.
  Params: `other: List[Any]`
- `List.get(index: Int) -> Result[Any, Error]` - pure; Returns `Any` or `Error` failure data. ID `method.List.get.0`.
  Params: `index: Int`
- `List.get(index: Int, fallback: Any) -> Any` - pure; Returns `Any`. ID `method.List.get.1`.
  Params: `index: Int`, `fallback: Any`
- `List.join(separator: Str = default) -> Str` - pure; Returns `Str`. ID `method.List.join.0`.
  Params: `separator: Str = default`
- `List.len() -> Int` - pure; Returns `Int`. ID `method.List.len.0`.
- `List.push(item: Any) -> List[Any]` - pure; Returns `List[Any]`. ID `method.List.push.0`.
  Params: `item: Any`

### `Map` Methods

- `Map.get(key: Str) -> Result[Any, Error]` - pure; Returns `Any` or `Error` failure data. ID `method.Map.get.0`.
  Params: `key: Str`
- `Map.get(key: Str, fallback: Any) -> Any` - pure; Returns `Any`. ID `method.Map.get.1`.
  Params: `key: Str`, `fallback: Any`
- `Map.has(key: Str) -> Bool` - pure; Returns `Bool`. ID `method.Map.has.0`.
  Params: `key: Str`
- `Map.keys() -> List[Str]` - pure; Returns `List[Str]`. ID `method.Map.keys.0`.
- `Map.len() -> Int` - pure; Returns `Int`. ID `method.Map.len.0`.
- `Map.push(key: Str, value: Any) -> Map[List[Any]]` - pure; Returns `Map[List[Any]]`. ID `method.Map.push.0`.
  Params: `key: Str`, `value: Any`
- `Map.remove(key: Str) -> Map[Any]` - pure; Returns `Map[Any]`. ID `method.Map.remove.0`.
  Params: `key: Str`
- `Map.set(key: Str, value: Any) -> Map[Any]` - pure; Returns `Map[Any]`. ID `method.Map.set.0`.
  Params: `key: Str`, `value: Any`
- `Map.values() -> List[Any]` - pure; Returns `List[Any]`. ID `method.Map.values.0`.

### `Path` Methods

- `Path.bytes_lines() -> Result[Stream[Bytes], Error]` - effect; Returns `Stream[Bytes]` or `Error` failure data. ID `method.Path.bytes_lines.0`.
- `Path.chmod(mode: Int) -> Result[Unit, Error]` - effect; Returns `Unit` or `Error` failure data. ID `method.Path.chmod.0`.
  Params: `mode: Int`
- `Path.copy(dest: Path, overwrite: Bool = default) -> Result[Unit, Error]` - effect; Returns `Unit` or `Error` failure data. ID `method.Path.copy.0`.
  Params: `dest: Path`, `overwrite: Bool = default`
- `Path.display() -> Str` - pure; Returns `Str`. ID `method.Path.display.0`.
- `Path.du() -> Result[Int, Error]` - effect; Returns `Int` or `Error` failure data. ID `method.Path.du.0`.
- `Path.executable() -> Result[Bool, Error]` - effect; Returns `Bool` or `Error` failure data. ID `method.Path.executable.0`.
- `Path.exists() -> Result[Bool, Error]` - effect; Returns `Bool` or `Error` failure data. ID `method.Path.exists.0`.
- `Path.ext() -> Str` - pure; Returns `Str`. ID `method.Path.ext.0`.
- `Path.hardlink(path: Path) -> Result[Unit, Error]` - effect; Returns `Unit` or `Error` failure data. ID `method.Path.hardlink.0`.
  Params: `path: Path`
- `Path.lines() -> Result[Stream[Str], Error]` - effect; Returns `Stream[Str]` or `Error` failure data. ID `method.Path.lines.0`.
- `Path.metadata() -> Result[{accessed: Int, blocks_512: Int, executable: Bool, ext: Str, gid: Int, group_executable: Bool, kind: Str, mode: Int, modified: Int, name: Str, other_executable: Bool, owner_executable: Bool, path: Path, setgid: Bool, setuid: Bool, size: Int, sticky: Bool, uid: Int, world_writable: Bool}, Error]` - effect; Returns `{accessed: Int, blocks_512: Int, executable: Bool, ext: Str, gid: Int, group_executable: Bool, kind: Str, mode: Int, modified: Int, name: Str, other_executable: Bool, owner_executable: Bool, path: Path, setgid: Bool, setuid: Bool, size: Int, sticky: Bool, uid: Int, world_writable: Bool}` or `Error` failure data. ID `method.Path.metadata.0`.
- `Path.mkdir(parents: Bool = default) -> Result[Unit, Error]` - effect; Returns `Unit` or `Error` failure data. ID `method.Path.mkdir.0`.
  Params: `parents: Bool = default`
- `Path.name() -> Str` - pure; Returns `Str`. ID `method.Path.name.0`.
- `Path.normalize() -> Path` - pure; Returns `Path`. ID `method.Path.normalize.0`.
- `Path.parent() -> Path` - pure; Returns `Path`. ID `method.Path.parent.0`.
- `Path.read_bytes() -> Result[Bytes, Error]` - effect; Returns `Bytes` or `Error` failure data. ID `method.Path.read_bytes.0`.
- `Path.read_text() -> Result[Str, Error]` - effect; Returns `Str` or `Error` failure data. ID `method.Path.read_text.0`.
- `Path.readlink() -> Result[Path, Error]` - effect; Returns `Path` or `Error` failure data. ID `method.Path.readlink.0`.
- `Path.relative_to(base: Path) -> Path` - pure; Returns `Path`. ID `method.Path.relative_to.0`.
  Params: `base: Path`
- `Path.remove(missing_ok: Bool = default) -> Result[Unit, Error]` - effect; Returns `Unit` or `Error` failure data. ID `method.Path.remove.0`.
  Params: `missing_ok: Bool = default`
- `Path.remove_dir() -> Result[Unit, Error]` - effect; Returns `Unit` or `Error` failure data. ID `method.Path.remove_dir.0`.
- `Path.rename(dest: Path, overwrite: Bool = default) -> Result[Unit, Error]` - effect; Returns `Unit` or `Error` failure data. ID `method.Path.rename.0`.
  Params: `dest: Path`, `overwrite: Bool = default`
- `Path.resolve() -> Result[Path, Error]` - effect; Returns `Path` or `Error` failure data. ID `method.Path.resolve.0`.
- `Path.strip_prefix(prefix: Path) -> Result[Path, Error]` - pure; Returns `Path` or `Error` failure data. ID `method.Path.strip_prefix.0`.
  Params: `prefix: Path`
- `Path.touch(create: Bool = default) -> Result[Unit, Error]` - effect; Returns `Unit` or `Error` failure data. ID `method.Path.touch.0`.
  Params: `create: Bool = default`
- `Path.touch_from(reference: Path) -> Result[Unit, Error]` - effect; Returns `Unit` or `Error` failure data. ID `method.Path.touch_from.0`.
  Params: `reference: Path`
- `Path.truncate(size: Int) -> Result[Unit, Error]` - effect; Returns `Unit` or `Error` failure data. ID `method.Path.truncate.0`.
  Params: `size: Int`
- `Path.unlink() -> Result[Unit, Error]` - effect; Returns `Unit` or `Error` failure data. ID `method.Path.unlink.0`.
- `Path.with_ext(ext: Str) -> Path` - pure; Returns `Path`. ID `method.Path.with_ext.0`.
  Params: `ext: Str`
- `Path.write(data: Bytes) -> Result[Unit, Error]` - effect; Returns `Unit` or `Error` failure data. ID `method.Path.write.0`.
  Params: `data: Bytes`
- `Path.write(data: Str) -> Result[Unit, Error]` - effect; Returns `Unit` or `Error` failure data. ID `method.Path.write.1`.
  Params: `data: Str`
- `Path.write_atomic(data: Bytes) -> Result[Unit, Error]` - effect; Returns `Unit` or `Error` failure data. ID `method.Path.write_atomic.0`.
  Params: `data: Bytes`
- `Path.write_atomic(data: Str) -> Result[Unit, Error]` - effect; Returns `Unit` or `Error` failure data. ID `method.Path.write_atomic.1`.
  Params: `data: Str`

### `PathConstructor` Methods

- `PathConstructor.parse_bytes(bytes: Bytes) -> Result[Path, Error]` - pure; Returns `Path` or `Error` failure data. ID `method.PathConstructor.parse_bytes.0`.
  Params: `bytes: Bytes`

### `ProcessHandle` Methods

- `ProcessHandle.cancel(signal: Str = default, kill_after: Duration = default) -> Result[Unit, ProcessError]` - effect; Returns `Unit` or `ProcessError` failure data. ID `method.ProcessHandle.cancel.0`.
  Params: `signal: Str = default`, `kill_after: Duration = default`

### `Record` Methods

- `Record.get(field: Str) -> Result[Any, Error]` - pure; Returns `Any` or `Error` failure data. ID `method.Record.get.0`.
  Params: `field: Str`
- `Record.has(field: Str) -> Bool` - pure; Returns `Bool`. ID `method.Record.has.0`.
  Params: `field: Str`
- `Record.keys() -> List[Str]` - pure; Returns `List[Str]`. ID `method.Record.keys.0`.

### `Regex` Methods

- `Regex.captures(text: Str) -> List[Str]` - pure; Returns `List[Str]`. ID `method.Regex.captures.0`.
  Params: `text: Str`
- `Regex.find(text: Str) -> List[{end: Int, start: Int, text: Str}]` - pure; Returns `List[{end: Int, start: Int, text: Str}]`. ID `method.Regex.find.0`.
  Params: `text: Str`
- `Regex.matches(text: Str) -> Bool` - pure; Returns `Bool`. ID `method.Regex.matches.0`.
  Params: `text: Str`
- `Regex.replace(text: Str, replacement: Str) -> Str` - pure; Returns `Str`. ID `method.Regex.replace.0`.
  Params: `text: Str`, `replacement: Str`

### `Result` Methods

- `Result.context(kind: Str, message: Str = default) -> Self` - pure; Returns the receiver result type. ID `method.Result.context.0`.
  Params: `kind: Str`, `message: Str = default`

### `Status` Methods

- `Status.exit_code() -> Result[Int, Error]` - pure; Returns `Int` or `Error` failure data. ID `method.Status.exit_code.0`.
- `Status.exited() -> Bool` - pure; Returns `Bool`. ID `method.Status.exited.0`.
- `Status.exited_with(code: Int) -> Bool` - pure; Returns `Bool`. ID `method.Status.exited_with.0`.
  Params: `code: Int`
- `Status.signal_number() -> Result[Int, Error]` - pure; Returns `Int` or `Error` failure data. ID `method.Status.signal_number.0`.
- `Status.signaled() -> Bool` - pure; Returns `Bool`. ID `method.Status.signaled.0`.

### `Str` Methods

- `Str.base32_decode() -> Result[Bytes, Error]` - pure; Returns `Bytes` or `Error` failure data. ID `method.Str.base32_decode.0`.
- `Str.base64_decode() -> Result[Bytes, Error]` - pure; Returns `Bytes` or `Error` failure data. ID `method.Str.base64_decode.0`.
- `Str.byte_at(index: Int, default: Int = default) -> Int` - pure; Returns `Int`. ID `method.Str.byte_at.0`.
  Params: `index: Int`, `default: Int = default`
- `Str.byte_len() -> Int` - pure; Returns `Int`. ID `method.Str.byte_len.0`.
- `Str.byte_slice(offset: Int, length: Int = default) -> Str` - pure; Returns `Str`. ID `method.Str.byte_slice.0`.
  Params: `offset: Int`, `length: Int = default`
- `Str.contains(needle: Str) -> Bool` - pure; Returns `Bool`. ID `method.Str.contains.0`.
  Params: `needle: Str`
- `Str.count_bytes() -> Int` - pure; Returns `Int`. ID `method.Str.count_bytes.0`.
- `Str.count_chars() -> Int` - pure; Returns `Int`. ID `method.Str.count_chars.0`.
- `Str.count_lines() -> Int` - pure; Returns `Int`. ID `method.Str.count_lines.0`.
- `Str.count_words() -> Int` - pure; Returns `Int`. ID `method.Str.count_words.0`.
- `Str.delete(chars: Str) -> Str` - pure; Returns `Str`. ID `method.Str.delete.0`.
  Params: `chars: Str`
- `Str.ends_with(suffix: Str) -> Bool` - pure; Returns `Bool`. ID `method.Str.ends_with.0`.
  Params: `suffix: Str`
- `Str.fields(delimiter: Str = default) -> List[Str]` - pure; Returns `List[Str]`. ID `method.Str.fields.0`.
  Params: `delimiter: Str = default`
- `Str.find(needle: Str, start: Int = default) -> Int` - pure; Returns `Int`. ID `method.Str.find.0`.
  Params: `needle: Str`, `start: Int = default`
- `Str.lines() -> Stream[Str]` - pure; Returns `Stream[Str]`. ID `method.Str.lines.0`.
- `Str.lower() -> Str` - pure; Returns `Str`. ID `method.Str.lower.0`.
- `Str.parse_float() -> Result[Float, Error]` - pure; Returns `Float` or `Error` failure data. ID `method.Str.parse_float.0`.
- `Str.parse_int() -> Result[Int, Error]` - pure; Returns `Int` or `Error` failure data. ID `method.Str.parse_int.0`.
- `Str.replace(from: Str, to: Str) -> Str` - pure; Returns `Str`. ID `method.Str.replace.0`.
  Params: `from: Str`, `to: Str`
- `Str.reverse() -> Str` - pure; Returns `Str`. ID `method.Str.reverse.0`.
- `Str.split(separator: Str) -> List[Str]` - pure; Returns `List[Str]`. ID `method.Str.split.0`.
  Params: `separator: Str`
- `Str.squeeze(chars: Str = default) -> Str` - pure; Returns `Str`. ID `method.Str.squeeze.0`.
  Params: `chars: Str = default`
- `Str.starts_with(prefix: Str) -> Bool` - pure; Returns `Bool`. ID `method.Str.starts_with.0`.
  Params: `prefix: Str`
- `Str.translate(from: Str, to: Str) -> Str` - pure; Returns `Str`. ID `method.Str.translate.0`.
  Params: `from: Str`, `to: Str`
- `Str.trim() -> Str` - pure; Returns `Str`. ID `method.Str.trim.0`.
- `Str.upper() -> Str` - pure; Returns `Str`. ID `method.Str.upper.0`.
- `Str.words() -> List[Str]` - pure; Returns `List[Str]`. ID `method.Str.words.0`.
- `Str.wrap(width: Int) -> List[Str]` - pure; Returns `List[Str]`. ID `method.Str.wrap.0`.
  Params: `width: Int`

### `Stream` Methods

- `Stream.collect() -> List[Any]` - pure; Returns `List[Any]`. ID `method.Stream.collect.0`.

## Records

### `ArchiveEntry`

- `kind: Str`
- `link_name: Str`
- `mode: Int`
- `modified: Int`
- `path: Path`
- `size: Int`

### `DiffResult`

- `files: Int`
- `hunks: Int`
- `text: Str`

### `DnsHost`

- `addr: Str`
- `family: Str`
- `name: Str`

### `DnsLookup`

- `name: Str`
- `record: Str`
- `ttl: Int`
- `value: Str`

### `ElfDynamicTag`

- `tag: Str`
- `value: Int`

### `ElfInfo`

- `class: Str`
- `dynamic_tags: List[{tag: Str, value: Int}]`
- `endian: Str`
- `flags: List[Str]`
- `interpreter: Str`
- `machine: Str`
- `needed: List[Str]`
- `os_abi: Str`
- `path: Path`
- `rpath: Str`
- `runpath: Str`
- `soname: Str`
- `type: Str`

### `EnvEntry`

- `name: Str`
- `value: Str`

### `EnvPathEntry`

- `empty: Bool`
- `index: Int`
- `path: Path`
- `raw: Str`

### `FsCopyTreeResult`

- `dirs: Int`
- `files: Int`
- `symlinks: Int`

### `FsEntry`

- `accessed: Int`
- `blocks_512: Int`
- `executable: Bool`
- `ext: Str`
- `gid: Int`
- `group_executable: Bool`
- `kind: Str`
- `mode: Int`
- `modified: Int`
- `name: Str`
- `other_executable: Bool`
- `owner_executable: Bool`
- `path: Path`
- `setgid: Bool`
- `setuid: Bool`
- `size: Int`
- `sticky: Bool`
- `uid: Int`
- `world_writable: Bool`

### `FsFilesystemStats`

- `available_1k: Int`
- `blocks_1k: Int`
- `capacity_percent: Int`
- `used_1k: Int`

### `FsLock`

- `id: Int`
- `path: Path`
- `shared: Bool`

### `FsMount`

- `available_1k: Int`
- `blocks_1k: Int`
- `capacity_percent: Int`
- `files: Int`
- `files_capacity_percent: Int`
- `files_free: Int`
- `files_used: Int`
- `filesystem: Str`
- `fstype: Str`
- `mounted_on: Path`
- `readonly: Bool`
- `used_1k: Int`

### `FsRemoveManifestResult`

- `missing: Int`
- `pruned_dirs: Int`
- `removed: Int`

### `FsRoot`

- `id: Int`

### `Group`

- `gid: Int`
- `members: List[Str]`
- `name: Str`

### `LinuxBlkid`

- `label: Str`
- `part_entry_uuid: Str`
- `part_table_type: Str`
- `type: Str`
- `uuid: Str`

### `LinuxBlockDevice`

- `name: Str`
- `partitioned: Bool`
- `partitions: List[Path]`
- `path: Path`
- `removable: Bool`
- `rotational: Bool`
- `sector_size: Int`
- `sectors: Int`
- `size: Int`

### `LinuxDiskUsage`

- `available: Int`
- `device: Str`
- `fstype: Str`
- `mount: Str`
- `total: Int`
- `used: Int`

### `LinuxFileAttrs`

- `append_only: Bool`
- `compression_requested: Bool`
- `dirsync: Bool`
- `flags: Int`
- `immutable: Bool`
- `indexed_directory: Bool`
- `journaled_data: Bool`
- `no_atime: Bool`
- `no_dump: Bool`
- `no_tailmerging: Bool`
- `secure_deletion: Bool`
- `sync: Bool`
- `top_of_directory_hierarchies: Bool`
- `undelete: Bool`

### `LinuxFsck`

- `errors: List[Str]`
- `status: Int`

### `LinuxInterface`

- `addresses: List[{addr: Str, family: Str, prefix_len: Int}]`
- `flags: List[Str]`
- `mac: Str`
- `mtu: Int`
- `name: Str`

### `LinuxInterfaceAddress`

- `addr: Str`
- `family: Str`
- `prefix_len: Int`

### `LinuxLoopDevice`

- `device: Path`
- `file: Path`
- `offset: Int`
- `size: Int`

### `LinuxMemInfo`

- `available: Int`
- `buffers: Int`
- `cached: Int`
- `free: Int`
- `swap_free: Int`
- `swap_total: Int`
- `total: Int`

### `LinuxModinfo`

- `description: Str`
- `filename: Path`
- `license: Str`
- `name: Str`
- `params: List[{description: Str, name: Str, type: Str}]`
- `version: Str`

### `LinuxModule`

- `name: Str`
- `size: Int`
- `used_by: List[Str]`

### `LinuxModuleParam`

- `description: Str`
- `name: Str`
- `type: Str`

### `LinuxOpenFile`

- `command: Str`
- `fd: Int`
- `inode: Int`
- `local: Str`
- `path: Path`
- `pid: Int`
- `protocol: Str`
- `remote: Str`
- `type: Str`

### `LinuxPartition`

- `end: Int`
- `index: Int`
- `name: Str`
- `size: Int`
- `start: Int`
- `type: Str`
- `uuid: Str`

### `LinuxPartitionTable`

- `id: Str`
- `label: Str`
- `partitions: List[{end: Int, index: Int, name: Str, size: Int, start: Int, type: Str, uuid: Str}]`
- `sector_size: Int`

### `LinuxRfkill`

- `hard_blocked: Bool`
- `id: Int`
- `name: Str`
- `soft_blocked: Bool`
- `type: Str`

### `LinuxRoute`

- `dev: Str`
- `dst: Str`
- `family: Str`
- `flags: List[Str]`
- `gateway: Str`
- `metric: Int`
- `prefix_len: Int`

### `LinuxUevent`

- `action: Str`
- `devname: Str`
- `devpath: Str`
- `env: List[{name: Str, value: Str}]`
- `subsystem: Str`

### `MeasuredCommand`

- `duration_ms: Int`
- `status: Status`
- `system_ns: Int`
- `user_ns: Int`
- `wall_ns: Int`

### `MimeInfo`

- `exts: List[Str]`
- `mime: Str`

### `MimeParse`

- `params: Map[Str]`
- `type: Str`

### `NetHeader`

- `name: Str`
- `value: Str`

### `NetPool`

- `idle_timeout_ms: Int`
- `max_idle_per_host: Int`
- `name: Str`

### `NetResponse`

- `body: Bytes`
- `bytes: Int`
- `headers: List[{name: Str, value: Str}]`
- `reason: Str`
- `status: Int`
- `url: Str`

### `PatchResult`

- `files: Int`
- `hunks: Int`

### `ProcessEntry`

- `argv: Str`
- `argv0: Str`
- `command: Str`
- `parent_pid: Int`
- `pid: Int`
- `runtime_seconds: Int`
- `start_time: Str`
- `start_time_ms: Int`
- `status: Str`
- `uid: Int`
- `user: Str`

### `ProcessPort`

- `argv: Str`
- `argv0: Str`
- `command: Str`
- `fd: Int`
- `inode: Int`
- `local: Str`
- `local_address: Str`
- `local_port: Int`
- `parent_pid: Int`
- `pid: Int`
- `protocol: Str`
- `remote: Str`
- `remote_address: Str`
- `remote_port: Int`
- `state: Str`
- `uid: Int`
- `user: Str`

### `ProcessThread`

- `argv: Str`
- `argv0: Str`
- `command: Str`
- `owner_pid: Int`
- `parent_pid: Int`
- `pid: Int`
- `runtime_seconds: Int`
- `start_time: Str`
- `start_time_ms: Int`
- `status: Str`
- `thread_id: Int`
- `thread_name: Str`
- `uid: Int`
- `user: Str`

### `Signal`

- `name: Str`
- `number: Int`

### `Spawn`

- `argv: Str`
- `command: Str`
- `detach: Bool`
- `ignore_hup: Bool`
- `new_session: Bool`
- `pid: Int`

### `SystemMemory`

- `available: Int`
- `free: Int`
- `swap_free: Int`
- `swap_total: Int`
- `total: Int`

### `SystemOsRelease`

- `id: Str`
- `name: Str`
- `pretty_name: Str`
- `version: Str`
- `version_id: Str`

### `TestCall`

- `args: Record`
- `op: Str`

### `TestContext`

- `file: Path`
- `name: Str`
- `temp_root: Path`

### `TestScriptOutput`

- `status: Int`
- `stderr: Str`
- `stderr_bytes: Bytes`
- `stdout: Str`
- `stdout_bytes: Bytes`
- `success: Bool`

### `Uname`

- `machine: Str`
- `nodename: Str`
- `release: Str`
- `sysname: Str`
- `version: Str`

### `UnixChildEvent`

- `pid: Int`
- `status: Status`

### `UnixGroupId`

- `gid: Int`
- `name: Str`

### `UnixId`

- `egid: Int`
- `euid: Int`
- `gid: Int`
- `groups: List[{gid: Int, name: Str}]`
- `uid: Int`

### `UnixKillAllResult`

- `matched: Int`
- `signaled: Int`

### `UnixLoggedProcessGroup`

- `argv: List[Str]`
- `command: Str`
- `detach: Bool`
- `ignore_hup: Bool`
- `log_pid: Int`
- `new_session: Bool`
- `pid: Int`

### `UnixPid1Event`

- `children: List[{pid: Int, status: Status}]`
- `kind: Str`
- `signal: Str`

### `UnixPid1Shutdown`

- `kill_sent: Int`
- `reaped: List[{pid: Int, status: Status}]`
- `remaining: List[Int]`
- `term_sent: Int`

### `UnixSpawnedChild`

- `argv: List[Str]`
- `command: Str`
- `detach: Bool`
- `ignore_hup: Bool`
- `new_session: Bool`
- `notify_fd: Int`
- `pid: Int`

### `UnixTtyAttrs`

- `cflag: Int`
- `control_chars: List[Int]`
- `crnl: Bool`
- `echo: Bool`
- `iflag: Int`
- `ispeed: Int`
- `lflag: Int`
- `oflag: Int`
- `ospeed: Int`
- `raw: Bool`

### `User`

- `gid: Int`
- `home: Path`
- `name: Str`
- `shell: Str`
- `uid: Int`
