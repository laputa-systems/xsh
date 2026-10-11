use super::{applet, Running};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Command, Output, Stdio};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, UNIX_EPOCH};
use ptytest::{CommandSpec, ExitStatus as PtyExitStatus, PtyTest, Scenario, Size, TestEnv};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

struct Terminal {
    master: File,
    slave: File,
}

impl Terminal {
    fn new(rows: u16, columns: u16) -> Self {
        let mut master = -1;
        let mut slave = -1;
        let size = libc::winsize {
            ws_row: rows, ws_col: columns,
            ws_xpixel: columns * 8, ws_ypixel: rows * 10,
        };
        assert_eq!(unsafe {
            libc::openpty(&mut master, &mut slave, std::ptr::null_mut(), std::ptr::null(), &size)
        }, 0, "open terminal: {}", std::io::Error::last_os_error());
        let terminal = Self {
            master: unsafe { File::from_raw_fd(master) },
            slave: unsafe { File::from_raw_fd(slave) },
        };
        for file in [&terminal.master, &terminal.slave] {
            assert_eq!(unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) }, 0);
        }
        terminal
    }

    fn stdio(&self) -> Stdio {
        self.slave.try_clone().unwrap().into()
    }
}

// Linux returns EIO after the final slave closes; other Unix hosts return EOF.
// Poll bounds the capture even if an inherited descriptor unexpectedly stays open.
fn terminal_bytes(mut master: File) -> Vec<u8> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut bytes = Vec::new();
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(!remaining.is_zero(), "terminal capture did not close");
        let mut event = libc::pollfd { fd: master.as_raw_fd(), events: libc::POLLIN, revents: 0 };
        let ready = unsafe { libc::poll(&mut event, 1, remaining.as_millis() as i32) };
        if ready < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted { continue; }
            panic!("poll terminal: {error}");
        }
        assert!(ready > 0, "terminal capture timed out");
        let mut buffer = [0; 4096];
        match master.read(&mut buffer) {
            Ok(0) => return bytes,
            Ok(count) => bytes.extend_from_slice(&buffer[..count]),
            Err(error) if error.raw_os_error() == Some(libc::EIO) => return bytes,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => panic!("read terminal: {error}"),
        }
    }
}

fn terminal_run(mut command: Command, streams: [bool; 3], size: (u16, u16), input: &[u8], write_after_spawn: bool) -> Output {
    let mut terminals: Vec<Option<Terminal>> = streams.into_iter()
        .map(|enabled| enabled.then(|| Terminal::new(size.0, size.1))).collect();
    if let Some(terminal) = &terminals[0] { command.stdin(terminal.stdio()); }
    if let Some(terminal) = &terminals[1] { command.stdout(terminal.stdio()); }
    if let Some(terminal) = &terminals[2] { command.stderr(terminal.stdio()); }
    if !write_after_spawn {
        if let Some(terminal) = &mut terminals[0] { terminal.master.write_all(input).unwrap(); }
    }
    let child = Running::spawn(&mut command);
    drop(command);
    // Only the child keeps the slaves alive during capture.
    let mut masters: Vec<Option<File>> = terminals.into_iter().map(|terminal| {
        terminal.map(|Terminal { master, slave }| { drop(slave); master })
    }).collect();
    if let Some(master) = &mut masters[0] {
        if write_after_spawn { master.write_all(input).unwrap(); }
        // Canonical input needs a completed line before its EOF character.
        master.write_all(b"\n\x04").unwrap();
    }
    let mut output = child.finish();
    if let Some(master) = masters[1].take() { output.stdout = terminal_bytes(master); }
    if let Some(master) = masters[2].take() { output.stderr = terminal_bytes(master); }
    output
}

#[derive(Debug, PartialEq, Eq)]
struct TermiosSnapshot {
    input: libc::tcflag_t,
    output: libc::tcflag_t,
    control: libc::tcflag_t,
    local: libc::tcflag_t,
    speeds: (libc::speed_t, libc::speed_t),
    control_chars: Vec<libc::cc_t>,
}

fn termios_snapshot(fd: libc::c_int) -> TermiosSnapshot {
    let mut attrs = unsafe { std::mem::zeroed::<libc::termios>() };
    assert_eq!(unsafe { libc::tcgetattr(fd, &mut attrs) }, 0, "read terminal attributes");
    TermiosSnapshot {
        input: attrs.c_iflag,
        output: attrs.c_oflag,
        control: attrs.c_cflag,
        local: attrs.c_lflag,
        speeds: (unsafe { libc::cfgetispeed(&attrs) }, unsafe { libc::cfgetospeed(&attrs) }),
        control_chars: attrs.c_cc.to_vec(),
    }
}

fn set_terminal_canonical(fd: libc::c_int) {
    let mut attrs = unsafe { std::mem::zeroed::<libc::termios>() };
    assert_eq!(unsafe { libc::tcgetattr(fd, &mut attrs) }, 0, "read terminal attributes");
    attrs.c_lflag |= libc::ICANON;
    assert_eq!(unsafe { libc::tcsetattr(fd, libc::TCSANOW, &attrs) }, 0, "enable canonical input");
}

fn fill_pty_output_queue(terminal: &Terminal) {
    let mut writer = terminal.slave.try_clone().unwrap();
    let flags = unsafe { libc::fcntl(writer.as_raw_fd(), libc::F_GETFL) };
    assert!(flags >= 0, "read terminal descriptor flags");
    assert_eq!(unsafe { libc::fcntl(writer.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) }, 0);
    let buffer = [b'x'; 4096];
    loop {
        match writer.write(&buffer) {
            Ok(0) => break,
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => panic!("fill terminal output queue: {error}"),
        }
    }
    assert_eq!(unsafe { libc::fcntl(writer.as_raw_fd(), libc::F_SETFL, flags) }, 0);
}

fn write_btop_frame(root: &Path, index: usize, counters: [u64; 4], receive: u64, transmit: u64) {
    let proc = root.join(format!("frames/{index}/proc"));
    fs::create_dir_all(proc.join("net")).unwrap();
    let [user, system, idle, _other] = counters;
    let stat = format!(
        "cpu {user} 0 {system} {idle} 0 0 0 0 0 0\ncpu0 {user} 0 {system} {idle} 0 0 0 0 0 0\nbtime 1700000000\nprocesses 1\nprocs_running 1\nprocs_blocked 0\n"
    );
    fs::write(proc.join("stat"), stat).unwrap();
    fs::write(proc.join("uptime"), "25.50 100.00\n").unwrap();
    fs::write(proc.join("loadavg"), "0.10 0.20 0.30 1/100 42\n").unwrap();
    fs::write(
        proc.join("meminfo"),
        "MemTotal: 1048576 kB\nMemFree: 131072 kB\nMemAvailable: 262144 kB\nBuffers: 32768 kB\nCached: 65536 kB\nActive: 524288 kB\nInactive: 131072 kB\nDirty: 128 kB\nWriteback: 0 kB\n",
    ).unwrap();
    fs::write(
        proc.join("net/dev"),
        format!(
            "Inter-| Receive                                                | Transmit\n face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed\neth0: {receive} 100 0 0 0 0 0 0 {transmit} 100 0 0 0 0 0 0\n"
        ),
    ).unwrap();
}

fn write_btop_process(root: &Path, frame: usize, pid: u32, start_ticks: u64, cpu_ticks: u64, rss_pages: u64) {
    let directory = root.join(format!("frames/{frame}/proc/{pid}"));
    fs::create_dir_all(&directory).unwrap();
    fs::write(
        directory.join("stat"),
        format!("{pid} (sample worker) S 1 {pid} {pid} 0 -1 4194560 100 0 0 0 {cpu_ticks} 0 0 0 20 1 1 0 {start_ticks} 16777216 {rss_pages} 0 0 0 0 0 0 0 0 0 0 0 0 0 17 0 0 0 0 0 0 0 0 0 0 0 0 0 0\n"),
    ).unwrap();
    fs::write(directory.join("statm"), format!("65536 {rss_pages} 100 0 0 1 0\n")).unwrap();
    fs::write(directory.join("status"), "Name:\tsample worker\nUid:\t1000\t1000\t1000\t1000\n").unwrap();
}

fn write_btop_bundle(root: &Path, frames: &[([u64; 4], u64, u64, u64)]) {
    for (index, (counters, receive, transmit, _at_ns)) in frames.iter().enumerate() {
        write_btop_frame(root, index, *counters, *receive, *transmit);
    }
    let frame_metadata = frames
        .iter()
        .map(|(_, _, _, at_ns)| format!("{{\"at_ns\":{at_ns}}}"))
        .collect::<Vec<_>>()
        .join(",");
    fs::write(
        root.join("manifest.json"),
        format!(
            "{{\"schema\":3,\"page_size_bytes\":4096,\"clock_ticks_per_second\":100,\"source_id\":\"pty-fixture\",\"display\":{{\"term\":\"xterm-256color\",\"colorterm\":\"\",\"no_color\":false,\"lc_all\":\"C.UTF-8\",\"lc_ctype\":\"\",\"lang\":\"\",\"rows\":24,\"cols\":80}},\"frames\":[{frame_metadata}]}}\n"
        ),
    ).unwrap();
    fs::write(root.join("observations.jsonl"), "").unwrap();
}

fn btop_applet(directory: &Path, bundle: &Path, frame_limit: usize) -> Command {
    let mut command = applet("btop", directory);
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    command
        .env(
            "XSH_MODULE_PATH",
            format!("{}:{}", repository.display(), repository.join("core").display()),
        )
        .args([
            "--replay",
            bundle.to_str().expect("fixture path is UTF-8"),
            "--frames",
            &frame_limit.to_string(),
        ]);
    command
}

fn btop_applet_on_terminals(directory: &Path, bundle: &Path, frame_limit: usize, input: &Terminal, output: &Terminal) -> Command {
    let mut command = btop_applet(directory, bundle, frame_limit);
    command.stdin(input.stdio()).stdout(output.stdio()).stderr(Stdio::piped());
    command
}

fn btop_pty_scenario(directory: &Path, script: &Path, columns: u16, rows: u16, label: &str) -> Scenario {
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let environment = TestEnv::hermetic_ascii().unwrap().env(
        "XSH_MODULE_PATH",
        format!("{}:{}", repository.display(), repository.join("core").display()),
    );
    let command = CommandSpec::new(release_bin!("xsh"))
        .arg(script)
        .current_dir(directory);
    Scenario::new(label)
        .unwrap()
        .command(command)
        .size(Size::new(columns, rows).unwrap())
        .environment(environment)
}

fn run_xsh_output(directory: &Path, name: &str, source: &str) -> Vec<u8> {
    let script = directory.join(name);
    fs::write(&script, source).unwrap();
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut command = Command::new(release_bin!("xsh"));
    command
        .arg(&script)
        .current_dir(directory)
        .env("XSH_MODULE_PATH", format!("{}:{}", repository.join("core").display(), repository.display()))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let result = Running::spawn(&mut command).finish();
    assert!(result.status.success(), "XSH surface fixture failed: {}", String::from_utf8_lossy(&result.stderr));
    assert!(result.stderr.is_empty(), "unexpected XSH surface fixture error: {}", String::from_utf8_lossy(&result.stderr));
    result.stdout
}

fn set_terminal_size(master_fd: libc::c_int, rows: u16, columns: u16) {
    let size = libc::winsize {
        ws_row: rows,
        ws_col: columns,
        ws_xpixel: columns * 8,
        ws_ypixel: rows * 10,
    };
    assert_eq!(unsafe { libc::ioctl(master_fd, libc::TIOCSWINSZ, &size) }, 0, "resize terminal");
}

fn append_terminal_output(master: &mut File, output: &mut Vec<u8>) {
    let mut buffer = [0; 8192];
    loop {
        match master.read(&mut buffer) {
            Ok(0) => return,
            Ok(count) => output.extend_from_slice(&buffer[..count]),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return,
            Err(error) if error.raw_os_error() == Some(libc::EIO) => return,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => panic!("read terminal output: {error}"),
        }
    }
}

fn wait_for_terminal_output(master: &mut File, output: &mut Vec<u8>, timeout: Duration, condition: impl Fn(&[u8]) -> bool) {
    let deadline = Instant::now() + timeout;
    while !condition(output) {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            let start = output.len().saturating_sub(3000);
            panic!("timed out waiting for terminal output; captured tail: {}", String::from_utf8_lossy(&output[start..]));
        }
        let mut event = libc::pollfd { fd: master.as_raw_fd(), events: libc::POLLIN, revents: 0 };
        let ready = unsafe { libc::poll(&mut event, 1, remaining.as_millis().min(100) as i32) };
        if ready < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted { continue; }
            panic!("poll terminal output: {error}");
        }
        if ready > 0 {
            append_terminal_output(master, output);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TerminalColor {
    Ansi(u8),
    Indexed(u8),
    Rgb(u8, u8, u8),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct VirtualTerminal {
    rows: usize,
    columns: usize,
    cells: Vec<Vec<String>>,
    styles: Vec<Vec<Option<TerminalColor>>>,
    cursor_row: usize,
    cursor_column: usize,
    alternate_screen: bool,
    paste_mode: bool,
    synchronized_output: bool,
    cursor_visible: bool,
    style_reset: bool,
    current_color: Option<TerminalColor>,
    last_cluster: Option<(usize, usize, String)>,
}

impl VirtualTerminal {
    fn new(rows: usize, columns: usize) -> Self {
        Self {
            rows,
            columns,
            cells: vec![vec![" ".to_owned(); columns]; rows],
            styles: vec![vec![None; columns]; rows],
            cursor_row: 0,
            cursor_column: 0,
            alternate_screen: false,
            paste_mode: false,
            synchronized_output: false,
            cursor_visible: true,
            style_reset: false,
            current_color: None,
            last_cluster: None,
        }
    }

    fn resize(&mut self, rows: usize, columns: usize) {
        let mut resized = vec![vec![" ".to_owned(); columns]; rows];
        let mut resized_styles = vec![vec![None; columns]; rows];
        for row in 0..rows.min(self.rows) {
            for column in 0..columns.min(self.columns) {
                let cell = &self.cells[row][column];
                if cell.is_empty() {
                    continue;
                }
                let width = cell.as_str().width();
                if column + width <= columns {
                    resized[row][column] = cell.clone();
                    resized_styles[row][column] = self.styles[row][column].clone();
                    for continuation in column + 1..column + width {
                        resized[row][continuation].clear();
                        resized_styles[row][continuation] = self.styles[row][continuation].clone();
                    }
                }
            }
        }
        self.rows = rows;
        self.columns = columns;
        self.cells = resized;
        self.styles = resized_styles;
        self.cursor_row = self.cursor_row.min(rows.saturating_sub(1));
        self.cursor_column = self.cursor_column.min(columns);
        self.last_cluster = None;
    }

    fn feed(&mut self, output: &[u8]) {
        let text = std::str::from_utf8(output).expect("terminal output is UTF-8");
        let bytes = text.as_bytes();
        let mut offset = 0;
        while offset < bytes.len() {
            if bytes[offset] == 0x1b {
                assert_eq!(bytes.get(offset + 1), Some(&b'['), "unsupported terminal escape");
                let final_offset = (offset + 2..bytes.len())
                    .find(|at| (0x40..=0x7e).contains(&bytes[*at]))
                    .expect("complete terminal control sequence");
                let parameters = std::str::from_utf8(&bytes[offset + 2..final_offset])
                    .expect("CSI parameters are ASCII");
                self.control(parameters, bytes[final_offset]);
                offset = final_offset + 1;
            } else {
                let character = text[offset..].chars().next().expect("character boundary");
                self.write_character(character);
                offset += character.len_utf8();
            }
        }
    }

    fn control(&mut self, parameters: &str, final_byte: u8) {
        self.last_cluster = None;
        if let Some(private_mode) = parameters.strip_prefix('?') {
            let mode = private_mode.parse::<u16>().expect("numeric private mode");
            match (mode, final_byte) {
                (1049, b'h') => self.alternate_screen = true,
                (1049, b'l') => self.alternate_screen = false,
                (2004, b'h') => self.paste_mode = true,
                (2004, b'l') => self.paste_mode = false,
                (2026, b'h') => self.synchronized_output = true,
                (2026, b'l') => self.synchronized_output = false,
                (25, b'h') => self.cursor_visible = true,
                (25, b'l') => self.cursor_visible = false,
                _ => panic!("unsupported private terminal mode: {parameters}{:?}", final_byte as char),
            }
            return;
        }

        match final_byte {
            b'J' if parameters == "2" => {
                self.cells.iter_mut().for_each(|row| row.fill(" ".to_owned()));
            }
            b'H' => {
                let coordinates = if parameters.is_empty() { vec![1, 1] } else {
                    parameters.split(';').map(|value| value.parse::<usize>().expect("numeric cursor position")).collect()
                };
                assert!(coordinates.len() == 2, "unsupported cursor position: {parameters}");
                assert!(coordinates[0] > 0 && coordinates[0] <= self.rows, "cursor row outside terminal");
                assert!(coordinates[1] > 0 && coordinates[1] <= self.columns, "cursor column outside terminal");
                self.cursor_row = coordinates[0] - 1;
                self.cursor_column = coordinates[1] - 1;
            }
            b'K' if parameters.is_empty() || parameters == "0" => {
                for column in self.cursor_column..self.columns {
                    self.clear_glyph(self.cursor_row, column);
                }
            }
            b'm' => self.sgr(parameters),
            _ => panic!("unsupported terminal control sequence: {parameters}{:?}", final_byte as char),
        }
    }

    fn sgr(&mut self, parameters: &str) {
        if parameters == "0" {
            self.style_reset = true;
            self.current_color = None;
            return;
        }
        let values = parameters
            .split(';')
            .map(|value| value.parse::<u16>().expect("numeric SGR parameter"))
            .collect::<Vec<_>>();
        let color = match values.as_slice() {
            [38, 5, index] if *index <= 255 => TerminalColor::Indexed(*index as u8),
            [38, 2, red, green, blue] if *red <= 255 && *green <= 255 && *blue <= 255 => {
                TerminalColor::Rgb(*red as u8, *green as u8, *blue as u8)
            }
            [code] if (30..=37).contains(code) => TerminalColor::Ansi((code - 30) as u8),
            [code] if (90..=97).contains(code) => TerminalColor::Ansi((code - 90 + 8) as u8),
            _ => panic!("unsupported terminal color SGR: {parameters}"),
        };
        self.current_color = Some(color);
    }

    fn joins_previous(&self, character: char) -> bool {
        let Some((row, _, cluster)) = &self.last_cluster else { return false };
        if *row != self.cursor_row { return false; }
        let last = cluster.chars().last().expect("nonempty cell cluster");
        let emoji_modifier = (0x1f3fb..=0x1f3ff).contains(&(character as u32))
            && cluster.chars().any(is_emoji_base);
        let regional_pair = (0x1f1e6..=0x1f1ff).contains(&(character as u32))
            && cluster.chars().count() == 1
            && (0x1f1e6..=0x1f1ff).contains(&(last as u32));
        UnicodeWidthChar::width(character) == Some(0)
            || last == '\u{200d}'
            || emoji_modifier
            || regional_pair
    }

    fn write_character(&mut self, character: char) {
        assert!(!character.is_control(), "unfiltered terminal control character: {character:?}");
        if self.joins_previous(character) {
            let (row, column, cluster) = self.last_cluster.take().expect("previous cluster");
            let old_width = cluster.as_str().width();
            let mut extended = cluster;
            extended.push(character);
            let new_width = extended.as_str().width();
            assert!(column + new_width <= self.columns, "wide grapheme would wrap at the terminal edge");
            for at in column..column + old_width.max(new_width) {
                self.clear_glyph(row, at);
            }
            self.cells[row][column] = extended.clone();
            self.styles[row][column] = self.current_color.clone();
            for at in column + 1..column + new_width {
                self.cells[row][at].clear();
                self.styles[row][at] = self.current_color.clone();
            }
            self.cursor_column = column + new_width;
            self.last_cluster = Some((row, column, extended));
            return;
        }

        let width = UnicodeWidthChar::width(character).expect("printable terminal character") as usize;
        assert!(width > 0, "standalone zero-width output must be sanitized");
        assert!(
            self.cursor_row < self.rows && self.cursor_column + width <= self.columns,
            "terminal output would wrap or scroll at ({}, {}), width {width}, char {character:?}, size {}x{}",
            self.cursor_row,
            self.cursor_column,
            self.rows,
            self.columns,
        );
        let row = self.cursor_row;
        let column = self.cursor_column;
        for at in column..column + width {
            self.clear_glyph(row, at);
        }
        let cluster = character.to_string();
        self.cells[row][column] = cluster.clone();
        self.styles[row][column] = self.current_color.clone();
        for at in column + 1..column + width {
            self.cells[row][at].clear();
            self.styles[row][at] = self.current_color.clone();
        }
        self.cursor_column += width;
        self.last_cluster = Some((row, column, cluster));
    }

    fn clear_glyph(&mut self, row: usize, column: usize) {
        if column >= self.columns { return; }
        if self.cells[row][column].is_empty() {
            if column > 0 {
                self.cells[row][column - 1] = " ".to_owned();
                self.styles[row][column - 1] = None;
            }
        } else {
            let width = self.cells[row][column].as_str().width();
            for at in column + 1..column + width.min(self.columns - column) {
                self.cells[row][at] = " ".to_owned();
                self.styles[row][at] = None;
            }
        }
        self.cells[row][column] = " ".to_owned();
        self.styles[row][column] = None;
    }

    fn line(&self, row: usize) -> String {
        self.cells[row].concat()
    }

    fn assert_modes_restored(&self) {
        assert!(!self.alternate_screen, "alternate screen was not closed");
        assert!(!self.paste_mode, "bracketed paste mode was not closed");
        assert!(!self.synchronized_output, "synchronized output was not closed");
        assert!(self.cursor_visible, "cursor was left hidden");
        assert!(self.style_reset, "terminal style was not reset");
    }
}

fn is_emoji_base(character: char) -> bool {
    matches!(character as u32, 0x2300..=0x23ff | 0x2600..=0x27ff | 0x1f000..=0x1faff)
}

fn set_master_nonblocking(master: &File) {
    let flags = unsafe { libc::fcntl(master.as_raw_fd(), libc::F_GETFL) };
    assert!(flags >= 0, "read terminal flags");
    assert_eq!(unsafe { libc::fcntl(master.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) }, 0);
}

fn assert_btop_terminal_modes_close(output: &[u8]) {
    for sequence in [
        b"\x1b[?1049h".as_slice(),
        b"\x1b[?2004h".as_slice(),
        b"\x1b[?2026h".as_slice(),
        b"\x1b[?2026l".as_slice(),
        b"\x1b[?2004l".as_slice(),
        b"\x1b[?1049l".as_slice(),
    ] {
        assert!(output.windows(sequence.len()).any(|window| window == sequence), "missing terminal mode sequence {sequence:?} in {:?}", String::from_utf8_lossy(output));
    }
}

fn synchronized_frames(output: &[u8]) -> usize {
    output.windows(b"\x1b[?2026l".len()).filter(|window| *window == b"\x1b[?2026l").count()
}

fn percentile_ms(samples: &[Duration], percentile: usize) -> f64 {
    let mut nanos = samples.iter().map(Duration::as_nanos).collect::<Vec<_>>();
    nanos.sort_unstable();
    let index = (nanos.len() * percentile).div_ceil(100).saturating_sub(1);
    nanos[index] as f64 / 1_000_000.0
}

#[test]
fn btop_surface_delta_reconstructs_the_same_cells_as_a_fresh_full_frame() {
    let directory = tempfile::tempdir().unwrap();
    let target = "let target = surface.from_lines(8, 2, [\"x界2ABCD\", \"new row\"])?\n";
    let delta_source = format!(
        "use core.lib.btop_surface as surface\nuse lib.gnu\n\
         let before = surface.from_lines(8, 2, [\"界12abcd\", \"old row\"])?\n\
         {target}\
         let first = surface.prepare(surface.initial(), before)\n\
         let committed = surface.complete(first, bytes.from_text(first.bytes).len())\n\
         let delta = surface.prepare(committed, target)\n\
         assert ! delta.full_redraw\n\
         gnu.write_text(first.bytes + delta.bytes)\n"
    );
    let full_source = format!(
        "use core.lib.btop_surface as surface\nuse lib.gnu\n\
         {target}\
         let full = surface.prepare(surface.initial(), target)\n\
         gnu.write_text(full.bytes)\n"
    );
    let delta_output = run_xsh_output(directory.path(), "delta.xsh", &delta_source);
    let full_output = run_xsh_output(directory.path(), "full.xsh", &full_source);
    let mut delta_screen = VirtualTerminal::new(2, 8);
    let mut full_screen = VirtualTerminal::new(2, 8);
    delta_screen.feed(&delta_output);
    full_screen.feed(&full_output);

    assert_eq!(delta_screen.cells, full_screen.cells);
    assert_eq!(delta_screen.line(0), "x界2ABCD");
    assert_eq!(delta_screen.line(1), "new row ");
}

#[test]
fn btop_replay_updates_after_resize_and_quits_with_restored_terminal() {
    let directory = tempfile::tempdir().unwrap();
    let bundle = directory.path().join("bundle");
    fs::create_dir(&bundle).unwrap();
    write_btop_bundle(
        &bundle,
        &[
            ([100, 50, 850, 0], 1_000_000, 500_000, 0),
            ([130, 60, 910, 0], 1_005_120, 501_024, 500_000_000),
            ([160, 70, 970, 0], 1_010_240, 502_048, 5_000_000_000),
        ],
    );
    let terminal = Terminal::new(24, 80);
    set_master_nonblocking(&terminal.master);
    let original_attrs = termios_snapshot(terminal.slave.as_raw_fd());
    let mut command = btop_applet(directory.path(), &bundle, 3);
    command.stdin(terminal.stdio()).stdout(terminal.stdio()).stderr(Stdio::piped());
    let child = Running::spawn(&mut command);
    drop(command);
    let mut output = Vec::new();

    wait_for_terminal_output(&mut { terminal.master.try_clone().unwrap() }, &mut output, Duration::from_secs(10), |bytes| {
        bytes.windows(b"Usage warming".len()).any(|window| window == b"Usage warming")
            && bytes.windows(b"\x1b[?2026l".len()).any(|window| window == b"\x1b[?2026l")
    });
    let initial_output_len = output.len();
    let mut screen = VirtualTerminal::new(24, 80);
    screen.feed(&output);
    assert!(screen.line(1).contains("Usage warming"));
    let active_attrs = termios_snapshot(terminal.slave.as_raw_fd());
    assert_eq!(active_attrs.local & libc::ICANON, 0, "btop must enter raw mode");
    assert_eq!(active_attrs.local & libc::ISIG, 0, "raw Ctrl-C must arrive as a byte");
    assert_eq!(active_attrs.local & libc::ECHO, 0, "raw input must not echo key bytes into the screen");
    set_terminal_size(terminal.master.as_raw_fd(), 30, 100);

    wait_for_terminal_output(&mut { terminal.master.try_clone().unwrap() }, &mut output, Duration::from_secs(10), |bytes| {
        let updates = bytes.windows(b"\x1b[?2026l".len()).filter(|window| *window == b"\x1b[?2026l").count();
        updates >= 3
            && bytes.windows(b"Usage 40.0%".len()).any(|window| window == b"Usage 40.0%")
    });
    terminal.master.try_clone().unwrap().write_all(b"q").unwrap();
    let result = child.finish();
    append_terminal_output(&mut terminal.master.try_clone().unwrap(), &mut output);
    screen.resize(30, 100);
    screen.feed(&output[initial_output_len..]);

    assert!(result.status.success(), "btop exited with {}: {}", result.status, String::from_utf8_lossy(&result.stderr));
    assert!(result.stderr.is_empty(), "unexpected btop error: {}", String::from_utf8_lossy(&result.stderr));
    assert_eq!(screen.rows, 30);
    assert_eq!(screen.columns, 100);
    assert!((0..screen.rows).any(|row| screen.line(row).contains("Usage 40.0%")));
    assert!(screen.line(0).contains("CPU"));
    screen.assert_modes_restored();
    assert_eq!(termios_snapshot(terminal.slave.as_raw_fd()), original_attrs);
    assert_btop_terminal_modes_close(&output);
    assert!(output.windows(b"\x1b[0m".len()).any(|window| window == b"\x1b[0m"));
    assert!(output.windows(b"\x1b[?25h".len()).any(|window| window == b"\x1b[?25h"));
}

#[test]
fn btop_replay_finite_completion_restores_terminal_attributes() {
    let directory = tempfile::tempdir().unwrap();
    let bundle = directory.path().join("bundle");
    fs::create_dir(&bundle).unwrap();
    write_btop_bundle(
        &bundle,
        &[
            ([100, 50, 850, 0], 1_000_000, 500_000, 0),
            ([160, 70, 970, 0], 1_010_240, 502_048, 100_000_000),
        ],
    );
    let terminal = Terminal::new(24, 80);
    set_master_nonblocking(&terminal.master);
    let original_attrs = termios_snapshot(terminal.slave.as_raw_fd());
    let mut command = btop_applet(directory.path(), &bundle, 2);
    command.stdin(terminal.stdio()).stdout(terminal.stdio()).stderr(Stdio::piped());
    let child = Running::spawn(&mut command);
    drop(command);
    let result = child.finish();
    let mut output = Vec::new();
    append_terminal_output(&mut terminal.master.try_clone().unwrap(), &mut output);

    assert!(result.status.success(), "finite replay exited with {}: {}", result.status, String::from_utf8_lossy(&result.stderr));
    assert!(result.stderr.is_empty(), "unexpected btop error: {}", String::from_utf8_lossy(&result.stderr));
    assert_eq!(termios_snapshot(terminal.slave.as_raw_fd()), original_attrs);
    assert_btop_terminal_modes_close(&output);
    let mut screen = VirtualTerminal::new(24, 80);
    screen.feed(&output);
    screen.assert_modes_restored();
    assert!(screen.line(1).contains("Usage 40.0%"));
    assert_eq!(screen.styles[0][0], Some(TerminalColor::Indexed(46)));
    assert_eq!(screen.styles[8][0], Some(TerminalColor::Indexed(39)));
    assert_eq!(screen.styles[17][0], Some(TerminalColor::Indexed(214)));
}

#[test]
fn btop_replay_ctrl_c_byte_exits_and_restores_terminal() {
    let directory = tempfile::tempdir().unwrap();
    let bundle = directory.path().join("bundle");
    fs::create_dir(&bundle).unwrap();
    write_btop_bundle(
        &bundle,
        &[
            ([100, 50, 850, 0], 1_000_000, 500_000, 0),
            ([160, 70, 970, 0], 1_010_240, 502_048, 5_000_000_000),
        ],
    );
    let terminal = Terminal::new(24, 80);
    set_master_nonblocking(&terminal.master);
    let original_attrs = termios_snapshot(terminal.slave.as_raw_fd());
    let mut command = btop_applet(directory.path(), &bundle, 2);
    command.stdin(terminal.stdio()).stdout(terminal.stdio()).stderr(Stdio::piped());
    let child = Running::spawn(&mut command);
    drop(command);
    let mut output = Vec::new();

    wait_for_terminal_output(&mut { terminal.master.try_clone().unwrap() }, &mut output, Duration::from_secs(10), |bytes| {
        bytes.windows(b"Usage warming".len()).any(|window| window == b"Usage warming")
            && bytes.windows(b"\x1b[?2026l".len()).any(|window| window == b"\x1b[?2026l")
    });
    let active_attrs = termios_snapshot(terminal.slave.as_raw_fd());
    assert_eq!(active_attrs.local & libc::ISIG, 0, "raw Ctrl-C must not raise SIGINT");
    assert_eq!(active_attrs.local & libc::ECHO, 0, "raw Ctrl-C must not echo as ^C");
    terminal.master.try_clone().unwrap().write_all(b"\x03").unwrap();
    let result = child.finish();
    append_terminal_output(&mut terminal.master.try_clone().unwrap(), &mut output);
    let mut screen = VirtualTerminal::new(24, 80);
    screen.feed(&output);

    assert!(result.status.success(), "Ctrl-C byte should exit cleanly: {}", result.status);
    assert!(result.stderr.is_empty(), "unexpected btop error: {}", String::from_utf8_lossy(&result.stderr));
    assert_eq!(termios_snapshot(terminal.slave.as_raw_fd()), original_attrs);
    assert_btop_terminal_modes_close(&output);
    screen.assert_modes_restored();
    assert!(output.windows(b"\x1b[0m".len()).any(|window| window == b"\x1b[0m"));
    assert!(output.windows(b"\x1b[?25h".len()).any(|window| window == b"\x1b[?25h"));
}

#[test]
fn btop_replay_stdin_eof_restores_terminal_attributes() {
    let directory = tempfile::tempdir().unwrap();
    let bundle = directory.path().join("bundle");
    fs::create_dir(&bundle).unwrap();
    write_btop_bundle(
        &bundle,
        &[
            ([100, 50, 850, 0], 1_000_000, 500_000, 0),
            ([160, 70, 970, 0], 1_010_240, 502_048, 30_000_000_000),
        ],
    );
    let input = Terminal::new(24, 80);
    let output_terminal = Terminal::new(24, 80);
    set_master_nonblocking(&input.master);
    set_master_nonblocking(&output_terminal.master);
    let original_attrs = termios_snapshot(input.slave.as_raw_fd());
    let mut command = btop_applet_on_terminals(directory.path(), &bundle, 2, &input, &output_terminal);
    let child = Running::spawn(&mut command);
    drop(command);
    let mut output = Vec::new();
    wait_for_terminal_output(&mut { output_terminal.master.try_clone().unwrap() }, &mut output, Duration::from_secs(10), |bytes| {
        bytes.windows(b"Usage warming".len()).any(|window| window == b"Usage warming")
            && bytes.windows(b"\x1b[?2026l".len()).any(|window| window == b"\x1b[?2026l")
    });

    // Canonical VEOF yields a zero-byte read while keeping stdout attached so cleanup is observable.
    set_terminal_canonical(input.slave.as_raw_fd());
    input.master.try_clone().unwrap().write_all(b"\x04").unwrap();
    let result = child.finish();
    append_terminal_output(&mut output_terminal.master.try_clone().unwrap(), &mut output);

    assert!(result.status.success(), "stdin EOF should quit cleanly: {}: {}", result.status, String::from_utf8_lossy(&result.stderr));
    assert!(result.stderr.is_empty(), "unexpected btop error: {}", String::from_utf8_lossy(&result.stderr));
    assert_eq!(termios_snapshot(input.slave.as_raw_fd()), original_attrs);
    assert_btop_terminal_modes_close(&output);
    let mut screen = VirtualTerminal::new(24, 80);
    screen.feed(&output);
    screen.assert_modes_restored();
}

#[test]
fn btop_replay_write_failure_restores_input_terminal_attributes() {
    let directory = tempfile::tempdir().unwrap();
    let bundle = directory.path().join("bundle");
    fs::create_dir(&bundle).unwrap();
    write_btop_bundle(
        &bundle,
        &[
            ([100, 50, 850, 0], 1_000_000, 500_000, 0),
            ([160, 70, 970, 0], 1_010_240, 502_048, 30_000_000_000),
        ],
    );
    let input = Terminal::new(24, 80);
    let output_terminal = Terminal::new(24, 80);
    set_master_nonblocking(&input.master);
    set_master_nonblocking(&output_terminal.master);
    let original_attrs = termios_snapshot(input.slave.as_raw_fd());
    let mut command = btop_applet_on_terminals(directory.path(), &bundle, 2, &input, &output_terminal);
    let child = Running::spawn(&mut command);
    drop(command);
    let mut output = Vec::new();
    wait_for_terminal_output(&mut { output_terminal.master.try_clone().unwrap() }, &mut output, Duration::from_secs(10), |bytes| {
        bytes.windows(b"Usage warming".len()).any(|window| window == b"Usage warming")
            && bytes.windows(b"\x1b[?2026l".len()).any(|window| window == b"\x1b[?2026l")
    });

    drop(output_terminal.master);
    input.master.try_clone().unwrap().write_all(b"r").unwrap();
    let result = child.finish();

    assert!(!result.status.success(), "a disconnected output terminal must fail the write");
    assert!(String::from_utf8_lossy(&result.stderr).contains("write error"), "expected the applet's handled write error: {}", String::from_utf8_lossy(&result.stderr));
    assert_eq!(termios_snapshot(input.slave.as_raw_fd()), original_attrs);
}

#[test]
fn btop_terminal_partial_start_failure_restores_input_terminal_attributes() {
    let directory = tempfile::tempdir().unwrap();
    let input = Terminal::new(24, 80);
    let output_terminal = Terminal::new(24, 80);
    let original_attrs = termios_snapshot(input.slave.as_raw_fd());
    let output = output_terminal.stdio();
    fill_pty_output_queue(&output_terminal);
    let script = directory.path().join("partial-start.xsh");
    fs::write(
        &script,
        "use core.lib.btop_terminal as terminal\nproc start_probe() [env, process, io, error] -> Result[Unit, Error] {\n  let session = terminal.begin()?\n  terminal.finish(session)?\n}\nstart_probe()?\n",
    ).unwrap();
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut command = Command::new(release_bin!("xsh"));
    command
        .arg(&script)
        .current_dir(directory.path())
        .env("XSH_MODULE_PATH", format!("{}:{}", repository.display(), repository.join("core").display()))
        .stdin(input.stdio())
        .stdout(output)
        .stderr(Stdio::piped());
    let input_probe = input.slave.try_clone().unwrap();
    let output_master = output_terminal.master;
    let close_blocked_output = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if termios_snapshot(input_probe.as_raw_fd()).local & libc::ICANON == 0 {
                drop(output_master);
                return true;
            }
            std::thread::yield_now();
        }
        drop(output_master);
        false
    });
    let child = Running::spawn(&mut command);
    drop(command);
    let raw_mode_was_reached = close_blocked_output.join().unwrap();
    let result = child.finish();

    assert!(raw_mode_was_reached, "session setup must reach raw mode before its first output blocks");
    assert!(!result.status.success(), "closed output must fail while initializing the terminal session");
    assert!(String::from_utf8_lossy(&result.stderr).contains("write error"), "expected the session-start write failure: {}", String::from_utf8_lossy(&result.stderr));
    assert_eq!(termios_snapshot(input.slave.as_raw_fd()), original_attrs);
}

#[test]
fn btop_replay_filter_sort_selection_and_resize_use_the_decoded_input() {
    let directory = tempfile::tempdir().unwrap();
    let bundle = directory.path().join("bundle");
    fs::create_dir(&bundle).unwrap();
    write_btop_bundle(
        &bundle,
        &[
            ([100, 50, 850, 0], 1_000_000, 500_000, 0),
            ([160, 70, 970, 0], 1_010_240, 502_048, 60_000_000_000),
        ],
    );
    for (frame, rows) in [
        (0, [(42, 1000, 10, 10), (43, 2000, 20, 20), (44, 3000, 30, 30)]),
        (1, [(42, 1000, 11, 10), (43, 2000, 21, 20), (44, 3000, 31, 30)]),
    ] {
        for (pid, start_ticks, cpu_ticks, rss_pages) in rows {
            write_btop_process(&bundle, frame, pid, start_ticks, cpu_ticks, rss_pages);
        }
    }
    let terminal = Terminal::new(24, 80);
    set_master_nonblocking(&terminal.master);
    let original_attrs = termios_snapshot(terminal.slave.as_raw_fd());
    let mut command = btop_applet(directory.path(), &bundle, 2);
    command.stdin(terminal.stdio()).stdout(terminal.stdio()).stderr(Stdio::piped());
    let child = Running::spawn(&mut command);
    drop(command);
    let mut output = Vec::new();

    wait_for_terminal_output(&mut { terminal.master.try_clone().unwrap() }, &mut output, Duration::from_secs(10), |bytes| {
        bytes.windows(b"PID USER CPU% RSS COMMAND".len()).any(|window| window == b"PID USER CPU% RSS COMMAND")
            && synchronized_frames(bytes) >= 1
    });
    let mut screen = VirtualTerminal::new(24, 80);
    let mut captured = output.len();
    screen.feed(&output);
    assert!((0..screen.rows).any(|row| screen.line(row).contains("> 43")));

    terminal.master.try_clone().unwrap().write_all(b"\x1b[B").unwrap();
    wait_for_terminal_output(&mut { terminal.master.try_clone().unwrap() }, &mut output, Duration::from_secs(2), |bytes| {
        synchronized_frames(bytes) >= 2 && bytes.windows(b"> 42".len()).any(|window| window == b"> 42")
    });
    screen.feed(&output[captured..]);
    captured = output.len();

    terminal.master.try_clone().unwrap().write_all(b"r").unwrap();
    wait_for_terminal_output(&mut { terminal.master.try_clone().unwrap() }, &mut output, Duration::from_secs(2), |bytes| {
        synchronized_frames(bytes) >= 3 && bytes.windows(b"Sort CPU lazy ascending".len()).any(|window| window == b"Sort CPU lazy ascending")
    });
    screen.feed(&output[captured..]);
    captured = output.len();

    terminal.master.try_clone().unwrap().write_all(b"\x1b[C").unwrap();
    wait_for_terminal_output(&mut { terminal.master.try_clone().unwrap() }, &mut output, Duration::from_secs(2), |bytes| {
        synchronized_frames(bytes) >= 4 && bytes.windows(b"Sort PID ascending".len()).any(|window| window == b"Sort PID ascending")
    });
    screen.feed(&output[captured..]);
    captured = output.len();

    terminal.master.try_clone().unwrap().write_all(b"f44\r").unwrap();
    wait_for_terminal_output(&mut { terminal.master.try_clone().unwrap() }, &mut output, Duration::from_secs(2), |bytes| {
        synchronized_frames(bytes) >= 5
            && bytes.windows(b"Filter: 44".len()).any(|window| window == b"Filter: 44")
            && bytes.windows(b"> 44".len()).any(|window| window == b"> 44")
    });
    screen.feed(&output[captured..]);
    captured = output.len();

    terminal.master.try_clone().unwrap().write_all(b"/").unwrap();
    wait_for_terminal_output(&mut { terminal.master.try_clone().unwrap() }, &mut output, Duration::from_secs(2), |bytes| {
        synchronized_frames(bytes) >= 6 && bytes.windows(b"Filter: 44_".len()).any(|window| window == b"Filter: 44_")
    });
    screen.feed(&output[captured..]);
    captured = output.len();

    terminal.master.try_clone().unwrap().write_all(b"x").unwrap();
    wait_for_terminal_output(&mut { terminal.master.try_clone().unwrap() }, &mut output, Duration::from_secs(2), |bytes| {
        synchronized_frames(bytes) >= 7 && bytes.windows(b"Filter: 44x_".len()).any(|window| window == b"Filter: 44x_")
    });
    screen.feed(&output[captured..]);
    captured = output.len();

    terminal.master.try_clone().unwrap().write_all(b"\x1b").unwrap();
    wait_for_terminal_output(&mut { terminal.master.try_clone().unwrap() }, &mut output, Duration::from_secs(2), |bytes| {
        synchronized_frames(bytes) >= 8
            && bytes.windows(b"Filter: 44 | f edit, Del clear".len()).any(|window| window == b"Filter: 44 | f edit, Del clear")
    });
    screen.feed(&output[captured..]);
    captured = output.len();

    terminal.master.try_clone().unwrap().write_all(b"\x1b[3~").unwrap();
    wait_for_terminal_output(&mut { terminal.master.try_clone().unwrap() }, &mut output, Duration::from_secs(2), |bytes| {
        synchronized_frames(bytes) >= 9 && bytes.windows(b"> 44".len()).any(|window| window == b"> 44")
    });
    screen.feed(&output[captured..]);
    captured = output.len();

    set_terminal_size(terminal.master.as_raw_fd(), 30, 100);
    wait_for_terminal_output(&mut { terminal.master.try_clone().unwrap() }, &mut output, Duration::from_secs(2), |bytes| {
        synchronized_frames(bytes) >= 10
    });
    screen.resize(30, 100);
    screen.feed(&output[captured..]);
    captured = output.len();

    terminal.master.try_clone().unwrap().write_all(b"q").unwrap();
    let result = child.finish();
    append_terminal_output(&mut terminal.master.try_clone().unwrap(), &mut output);
    screen.feed(&output[captured..]);

    assert!(result.status.success(), "interactive view exited with {}: {}", result.status, String::from_utf8_lossy(&result.stderr));
    assert!(result.stderr.is_empty(), "unexpected btop error: {}", String::from_utf8_lossy(&result.stderr));
    assert_eq!(termios_snapshot(terminal.slave.as_raw_fd()), original_attrs);
    assert_eq!(screen.rows, 30);
    assert_eq!(screen.columns, 100);
    assert!((0..screen.rows).any(|row| screen.line(row).contains("> 44")));
    assert!((0..screen.rows).any(|row| screen.line(row).contains("43 1000")));
    assert!((0..screen.rows).any(|row| screen.line(row).contains("44 1000")));
    screen.assert_modes_restored();
}

#[test]
fn btop_replay_network_and_cpu_view_keys_change_rendered_cells() {
    let directory = tempfile::tempdir().unwrap();
    let bundle = directory.path().join("bundle");
    fs::create_dir(&bundle).unwrap();
    write_btop_bundle(
        &bundle,
        &[
            ([100, 50, 850, 0], 1_000_000, 500_000, 0),
            ([160, 70, 970, 0], 1_010_240, 502_048, 60_000_000_000),
        ],
    );
    for (frame, loopback_bytes) in [(0, 64), (1, 128)] {
        let proc = bundle.join(format!("frames/{frame}/proc"));
        let net_dev = proc.join("net/dev");
        let mut contents = fs::read_to_string(&net_dev).unwrap();
        contents.push_str(&format!("lo: {loopback_bytes} 1 0 0 0 0 0 0 {loopback_bytes} 1 0 0 0 0 0 0\n"));
        fs::write(net_dev, contents).unwrap();
        for (interface, ifindex) in [("eth0", 2), ("lo", 1)] {
            let path = bundle.join(format!("frames/{frame}/sys/class/net/{interface}"));
            fs::create_dir_all(&path).unwrap();
            fs::write(path.join("ifindex"), format!("{ifindex}\n")).unwrap();
        }
        write_btop_process(&bundle, frame, 42, 1000, if frame == 0 { 10 } else { 11 }, 10);
    }
    let model_dump = directory.path().join("hotkey-model.jsonl");
    let mut headless = btop_applet(directory.path(), &bundle, 2);
    headless.args(["--headless", "--dump-model", model_dump.to_str().unwrap()]);
    let model_result = headless.output().unwrap();
    assert!(model_result.status.success(), "headless fixture capture failed: {}", String::from_utf8_lossy(&model_result.stderr));
    let model_dump = fs::read_to_string(model_dump).unwrap();
    assert!(model_dump.contains("\"name\":\"lo\""), "loopback is missing from replay models: {model_dump}");

    let terminal = Terminal::new(24, 80);
    set_master_nonblocking(&terminal.master);
    let original_attrs = termios_snapshot(terminal.slave.as_raw_fd());
    let mut command = btop_applet(directory.path(), &bundle, 2);
    command.stdin(terminal.stdio()).stdout(terminal.stdio()).stderr(Stdio::piped());
    let mut child = Running::spawn(&mut command);
    drop(command);
    let mut output = Vec::new();
    wait_for_terminal_output(&mut { terminal.master.try_clone().unwrap() }, &mut output, Duration::from_secs(10), |bytes| {
        bytes.windows(b"Interface eth0".len()).any(|window| window == b"Interface eth0")
            && bytes.windows(b"PID USER CPU% RSS COMMAND".len()).any(|window| window == b"PID USER CPU% RSS COMMAND")
    });
    let mut screen = VirtualTerminal::new(24, 80);
    screen.feed(&output);
    let mut captured = output.len();

    let before_per_core = synchronized_frames(&output);
    terminal.master.try_clone().unwrap().write_all(b"c").unwrap();
    wait_for_terminal_output(&mut { terminal.master.try_clone().unwrap() }, &mut output, Duration::from_secs(2), |bytes| {
        synchronized_frames(bytes) > before_per_core
    });
    screen.feed(&output[captured..]);
    assert!((0..screen.rows).any(|row| screen.line(row).contains("CPU/core%")));
    captured = output.len();

    assert!(child.0.try_wait().unwrap().is_none(), "monitor exited before network selection");
    let before_interface_cycle = synchronized_frames(&output);
    terminal.master.try_clone().unwrap().write_all(b"n").unwrap();
    wait_for_terminal_output(&mut { terminal.master.try_clone().unwrap() }, &mut output, Duration::from_secs(2), |bytes| {
        synchronized_frames(bytes) > before_interface_cycle
    });
    assert!(child.0.try_wait().unwrap().is_none(), "monitor exited while handling network selection");
    screen.feed(&output[captured..]);
    assert!(
        (0..screen.rows).any(|row| screen.line(row).contains("Interface lo")),
        "network key did not render loopback: {}; bytes={:?}",
        (0..screen.rows).map(|row| screen.line(row)).collect::<Vec<_>>().join("\n"),
        String::from_utf8_lossy(&output[captured..]),
    );
    captured = output.len();

    let before_total_offset = synchronized_frames(&output);
    terminal.master.try_clone().unwrap().write_all(b"z").unwrap();
    wait_for_terminal_output(&mut { terminal.master.try_clone().unwrap() }, &mut output, Duration::from_secs(2), |bytes| {
        synchronized_frames(bytes) > before_total_offset
    });
    screen.feed(&output[captured..]);
    assert!((0..screen.rows).any(|row| screen.line(row).contains("0 B total")));

    terminal.master.try_clone().unwrap().write_all(b"q").unwrap();
    let result = child.finish();
    append_terminal_output(&mut terminal.master.try_clone().unwrap(), &mut output);
    screen.feed(&output[captured..]);
    assert!(result.status.success(), "hotkey replay exited with {}: {}", result.status, String::from_utf8_lossy(&result.stderr));
    assert!(result.stderr.is_empty(), "unexpected btop error: {}", String::from_utf8_lossy(&result.stderr));
    assert_eq!(termios_snapshot(terminal.slave.as_raw_fd()), original_attrs);
    screen.assert_modes_restored();
}

#[test]
fn btop_input_to_present_latency_is_separate_from_replay_sample_interval() {
    let directory = tempfile::tempdir().unwrap();
    let bundle = directory.path().join("bundle");
    fs::create_dir(&bundle).unwrap();
    write_btop_bundle(
        &bundle,
        &[
            ([100, 50, 850, 0], 1_000_000, 500_000, 0),
            ([160, 70, 970, 0], 1_010_240, 502_048, 60_000_000_000),
        ],
    );
    let terminal = Terminal::new(24, 80);
    set_master_nonblocking(&terminal.master);
    let original_attrs = termios_snapshot(terminal.slave.as_raw_fd());
    let mut command = btop_applet(directory.path(), &bundle, 2);
    command.stdin(terminal.stdio()).stdout(terminal.stdio()).stderr(Stdio::piped());
    let child = Running::spawn(&mut command);
    drop(command);
    let mut output = Vec::new();
    wait_for_terminal_output(&mut { terminal.master.try_clone().unwrap() }, &mut output, Duration::from_secs(10), |bytes| {
        bytes.windows(b"Usage warming".len()).any(|window| window == b"Usage warming")
            && bytes.windows(b"\x1b[?2026l".len()).any(|window| window == b"\x1b[?2026l")
    });

    let mut samples = Vec::new();
    for _ in 0..100 {
        let expected_frames = synchronized_frames(&output) + 1;
        let started = Instant::now();
        terminal.master.try_clone().unwrap().write_all(b"r").unwrap();
        wait_for_terminal_output(&mut { terminal.master.try_clone().unwrap() }, &mut output, Duration::from_secs(2), |bytes| {
            synchronized_frames(bytes) >= expected_frames
        });
        samples.push(started.elapsed());
    }

    terminal.master.try_clone().unwrap().write_all(b"q").unwrap();
    let result = child.finish();
    append_terminal_output(&mut terminal.master.try_clone().unwrap(), &mut output);
    assert!(result.status.success(), "latency probe exited with {}: {}", result.status, String::from_utf8_lossy(&result.stderr));
    assert!(result.stderr.is_empty(), "unexpected btop error: {}", String::from_utf8_lossy(&result.stderr));
    assert_eq!(termios_snapshot(terminal.slave.as_raw_fd()), original_attrs);
    println!(
        "btop input-to-present measurement: runner=Alpine x86_64 musl release size=80x24 samples={} replay_interval_ms=60000 p95_ms={:.3} p99_ms={:.3}",
        samples.len(),
        percentile_ms(&samples, 95),
        percentile_ms(&samples, 99),
    );
}

#[test]
fn btop_terminal_cleanup_runs_after_runtime_error_during_presentation() {
    let directory = tempfile::tempdir().unwrap();
    let script = directory.path().join("render-failure.xsh");
    fs::write(
        &script,
        "use core.lib.btop_terminal as terminal\nproc cleanup_probe() [env, process, io, error] -> Result[Unit, Error] {\n  let session = terminal.begin()?\n  defer { terminal.finish(session)? }\n  terminal.present(\"partial frame\")\n  fail \"forced render failure\"\n}\ncleanup_probe()?\n",
    ).unwrap();
    let mut terminal = PtyTest::spawn(btop_pty_scenario(directory.path(), &script, 80, 24, "btop runtime error cleanup")).unwrap();
    let baseline = terminal.terminal_baseline();
    let status = terminal.wait_for_exit(terminal.deadline(Duration::from_secs(3))).unwrap();
    assert!(matches!(status, PtyExitStatus::Code(code) if code != 0), "the injected XSH failure must remain visible: {status:?}");
    let output = terminal.raw_output();
    assert!(String::from_utf8_lossy(output).contains("partial frame"), "unexpected terminal output: {:?}", String::from_utf8_lossy(output));
    assert!(String::from_utf8_lossy(output).contains("forced render failure"));
    assert_btop_terminal_modes_close(output);
    terminal.assert_terminal_restored(&baseline).unwrap();
    assert_eq!(terminal.finish(terminal.deadline(Duration::from_secs(3))).unwrap(), status);
}

fn tty_case(streams: [bool; 3], size: (u16, u16), stdout: &[u8], stderr: &[u8]) {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("is_a_tty.sh"), r#"for descriptor in '0 stdin' '1 stdout' '2 stderr'; do
    set -- $descriptor
    if test -t "$1"; then
        printf '%s is a tty\n' "$2"
        if test "$1" = 0; then
            printf 'terminal size: '
            stty size
        fi
    else
        printf '%s is not a tty\n' "$2"
    fi
done
printf 'This is an error message.\n' >&2
exit 0
"#).unwrap();
    let mut command = applet("env", directory.path());
    command.args(["sh", "is_a_tty.sh"]);
    let output = terminal_run(command, streams, size, b"", false);
    assert!(output.status.success());
    assert_eq!(output.stdout, stdout);
    assert_eq!(output.stderr, stderr);
}

// origin: uutils test_env::test_simulation_of_terminal_true
#[test]
fn env_terminal_all_streams() {
    tty_case([true; 3], (30, 80), b"stdin is a tty\r\nterminal size: 30 80\r\nstdout is a tty\r\nstderr is a tty\r\n", b"This is an error message.\r\n");
}

// origin: uutils test_env::test_simulation_of_terminal_for_stdin_only
#[test]
fn env_terminal_stdin_only() {
    tty_case([true, false, false], (30, 80), b"stdin is a tty\nterminal size: 30 80\nstdout is not a tty\nstderr is not a tty\n", b"This is an error message.\n");
}

// origin: uutils test_env::test_simulation_of_terminal_for_stdout_only
#[test]
fn env_terminal_stdout_only() {
    tty_case([false, true, false], (30, 80), b"stdin is not a tty\r\nstdout is a tty\r\nstderr is not a tty\r\n", b"This is an error message.\n");
}

// origin: uutils test_env::test_simulation_of_terminal_for_stderr_only
#[test]
fn env_terminal_stderr_only() {
    tty_case([false, false, true], (30, 80), b"stdin is not a tty\nstdout is not a tty\nstderr is a tty\n", b"This is an error message.\r\n");
}

// origin: uutils test_env::test_simulation_of_terminal_size_information
#[test]
fn env_terminal_dimensions() {
    tty_case([true; 3], (10, 40), b"stdin is a tty\r\nterminal size: 10 40\r\nstdout is a tty\r\nstderr is a tty\r\n", b"This is an error message.\r\n");
}

fn cat_terminal(input: &[u8], after_spawn: bool, expected: &[u8]) {
    let directory = tempfile::tempdir().unwrap();
    let mut command = applet("env", directory.path());
    command.args(["cat", "-"]);
    let output = terminal_run(command, [true; 3], (30, 80), input, after_spawn);
    assert!(output.status.success());
    assert_eq!(output.stdout, expected);
    assert!(output.stderr.is_empty());
}

// origin: uutils test_env::test_simulation_of_terminal_pty_sends_eot_automatically
#[test]
fn env_terminal_eot() { cat_terminal(b"", false, b"\r\n"); }

// origin: uutils test_env::test_simulation_of_terminal_pty_pipes_into_data_and_sends_eot_automatically
#[test]
fn env_terminal_forwarded_input_eot() {
    cat_terminal(b"Hello stdin forwarding!", false, b"Hello stdin forwarding!\r\n");
}

// origin: uutils test_env::test_simulation_of_terminal_pty_write_in_data_and_sends_eot_automatically
#[test]
fn env_terminal_written_input_eot() {
    cat_terminal(b"Hello stdin forwarding via write_in!", true, b"Hello stdin forwarding via write_in!\r\n");
}

// origin: uutils test_env::test_ignore_signal_pipe_broken_pipe_regression
#[test]
fn env_sigpipe_after_reader_closes() {
    let directory = tempfile::tempdir().unwrap();
    // Both images need the same producer's EPIPE exit policy. A bounded shell
    // fixture emits the original sequence and exits 1 when a write fails;
    // inherited default SIGPIPE still terminates it before that error path.
    fs::write(directory.path().join("seq"), br#"#!/bin/sh
[ "$#" -eq 2 ] && [ "$1" = 1 ] && [ "$2" = 1000000 ] || exit 2
value=$1
while [ "$value" -le "$2" ]; do
  printf '%s\n' "$value" || exit 1
  value=$((value + 1))
done
"#).unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(directory.path().join("seq"), fs::Permissions::from_mode(0o755)).unwrap();
    let exits: Vec<i32> = [false, true].into_iter().map(|ignore| {
        let mut command = applet("env", directory.path());
        command.env("PATH", format!("{}:/bin:/usr/bin", directory.path().display()));
        if ignore { command.arg("--ignore-signal=PIPE"); }
        command.args(["seq", "1", "1000000"]).stderr(Stdio::null());
        // Rust ignores SIGPIPE in its own process; restore the normal inherited
        // disposition in this child without touching the parallel test runner.
        unsafe { command.pre_exec(|| {
            if libc::signal(libc::SIGPIPE, libc::SIG_DFL) == libc::SIG_ERR {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        }); }
        let mut child = Running::spawn(&mut command);
        assert_eq!(child.prefix(2), b"1\n");
        drop(child.0.stdout.take().unwrap());
        let status = child.finish().status;
        status.code().unwrap_or_else(|| 128 + status.signal().unwrap())
    }).collect();
    assert_ne!(exits[1], 141);
    assert_eq!(exits[0], 141);
    assert!(exits[1] == 0 || exits[1] == 1, "producer exit statuses: {exits:?}");
}

// origin: uutils test_dd::diagnostics::test_plain_message_at_a_terminal_when_asked_for
#[test]
fn dd_plain_terminal_diagnostic() {
    let directory = tempfile::tempdir().unwrap();
    let mut command = applet("dd", directory.path());
    command.arg("bsx=1").env("UUTILS_DIAG", "never");
    let output = terminal_run(command, [false, false, true], (30, 80), b"", false);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap().replace("\r\n", "\n");
    assert!(stderr.contains("dd: unrecognized operand 'bsx=1'"));
    assert!(!stderr.contains('╭'));
}

fn expr_terminal(args: &[&str]) -> Output {
    let directory = tempfile::tempdir().unwrap();
    let mut command = applet("expr", directory.path());
    command.args(args);
    terminal_run(command, [false, false, true], (30, 80), b"", false)
}

// origin: uutils test_expr::diagnostics::test_errors_without_a_position_stay_plain
#[test]
fn expr_terminal_errors_without_position() {
    let output = expr_terminal(&["6", "/", "0"]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(String::from_utf8(output.stderr).unwrap().replace("\r\n", "\n"), "expr: division by zero\n");
    let output = expr_terminal(&[]);
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("missing operand"));
    assert!(stderr.contains("for more information"));
}

// origin: uutils test_expr::diagnostics::test_computed_operand_stays_plain
#[test]
fn expr_terminal_computed_operand() {
    let output = expr_terminal(&["substr", "abc", "1", "2", "+", "1"]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(String::from_utf8(output.stderr).unwrap().replace("\r\n", "\n"), "expr: non-integer argument\n");
}

fn cp_prompt(input: &[u8], success: bool, expected: &[u8]) {
    let directory = tempfile::tempdir().unwrap();
    File::create(directory.path().join("old")).unwrap().set_modified(UNIX_EPOCH).unwrap();
    File::create(directory.path().join("new")).unwrap();
    let capture = File::create(directory.path().join("merged")).unwrap();
    let mut command = applet("cp", directory.path());
    command.args(["-i", "-v", "--update=older", "new", "old"])
        .stdin(Stdio::piped()).stdout(capture.try_clone().unwrap()).stderr(capture);
    let mut child = Running::spawn(&mut command);
    child.0.stdin.take().unwrap().write_all(input).unwrap();
    let output = child.finish();
    assert_eq!(output.status.success(), success);
    assert_eq!(fs::read(directory.path().join("merged")).unwrap(), expected);
}

// origin: uutils test_cp::test_cp_update_older_interactive_prompt_yes
#[test]
fn cp_update_older_prompt_yes() { cp_prompt(b"Y\n", true, b"cp: overwrite 'old'? 'new' -> 'old'\n"); }

// origin: uutils test_cp::test_cp_update_older_interactive_prompt_no
#[test]
fn cp_update_older_prompt_no() { cp_prompt(b"N\n", false, b"cp: overwrite 'old'? "); }
