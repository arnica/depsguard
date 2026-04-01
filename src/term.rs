// Zero-dependency terminal handling: raw mode, ANSI codes, input parsing.
#![allow(dead_code)]

use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};

// ── Color support ────────────────────────────────────────────────────

static COLORS_ENABLED: AtomicBool = AtomicBool::new(true);

/// Disable all ANSI color/style output.
pub fn disable_colors() {
    COLORS_ENABLED.store(false, Ordering::Relaxed);
}

/// Check if colors should be used based on environment.
/// Respects: --no-color flag, NO_COLOR env var, non-TTY stdout, TERM=dumb.
pub fn should_use_colors() -> bool {
    // NO_COLOR convention (https://no-color.org/)
    if std::env::var("NO_COLOR").is_ok() {
        return false;
    }
    // TERM=dumb
    if std::env::var("TERM").map(|t| t == "dumb").unwrap_or(false) {
        return false;
    }
    // Check if stdout is a TTY
    #[cfg(unix)]
    {
        unsafe extern "C" {
            fn isatty(fd: i32) -> i32;
        }
        // SAFETY: isatty(1) is a standard POSIX call on fd 1 (stdout), always safe.
        if unsafe { isatty(1) } == 0 {
            return false;
        }
    }
    #[cfg(windows)]
    {
        unsafe extern "system" {
            fn GetStdHandle(nStdHandle: u32) -> *mut std::ffi::c_void;
            fn GetConsoleMode(h: *mut std::ffi::c_void, mode: *mut u32) -> i32;
        }
        // SAFETY: GetStdHandle with STD_OUTPUT_HANDLE is always safe; GetConsoleMode
        // reads into a valid &mut u32 pointer. Both are standard Win32 console APIs.
        let handle = unsafe { GetStdHandle(0xFFFF_FFF5) };
        let mut mode = 0u32;
        if unsafe { GetConsoleMode(handle, &mut mode) } == 0 {
            return false;
        }
    }
    true
}

pub fn colors_enabled() -> bool {
    COLORS_ENABLED.load(Ordering::Relaxed)
}

// ── ANSI helpers ──────────────────────────────────────────────────────

pub const RESET: &str = "\x1b[0m";
pub const BOLD: &str = "\x1b[1m";
pub const DIM: &str = "\x1b[2m";
pub const GREEN: &str = "\x1b[32m";
pub const RED: &str = "\x1b[31m";
pub const YELLOW: &str = "\x1b[33m";
pub const CYAN: &str = "\x1b[36m";
pub const MAGENTA: &str = "\x1b[35m";
pub const WHITE: &str = "\x1b[97m";
pub const BG_GREEN: &str = "\x1b[42m";
pub const BG_RED: &str = "\x1b[41m";

/// A writer wrapper that strips ANSI escape sequences when colors are disabled.
pub struct ColorWriter<W: Write> {
    inner: W,
}

impl<W: Write> ColorWriter<W> {
    pub fn new(inner: W) -> Self {
        Self { inner }
    }

    pub fn into_inner(self) -> W {
        self.inner
    }
}

impl<W: Write> Write for ColorWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if colors_enabled() {
            return self.inner.write(buf);
        }
        // Strip ANSI color/style sequences (\x1b[...m) but keep cursor control
        // CSI sequences end with a byte in 0x40..0x7E (@A-Z[\]^_`a-z{|}~)
        let mut i = 0;
        let len = buf.len();
        while i < len {
            if buf[i] == 0x1b && i + 1 < len && buf[i + 1] == b'[' {
                // Find the terminating byte
                let start = i;
                i += 2;
                while i < len && !(0x40..=0x7E).contains(&buf[i]) {
                    i += 1;
                }
                if i < len {
                    let terminator = buf[i];
                    i += 1;
                    // Only strip color/style (ends with 'm'), pass through others
                    if terminator != b'm' {
                        self.inner.write_all(&buf[start..i])?;
                    }
                }
            } else {
                self.inner.write_all(&buf[i..i + 1])?;
                i += 1;
            }
        }
        Ok(len) // report all bytes as consumed
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

pub fn clear_screen(w: &mut impl Write) -> io::Result<()> {
    write!(w, "\x1b[2J\x1b[H")
}

pub fn hide_cursor(w: &mut impl Write) -> io::Result<()> {
    write!(w, "\x1b[?25l")
}

pub fn show_cursor(w: &mut impl Write) -> io::Result<()> {
    write!(w, "\x1b[?25h")
}

pub fn move_to(w: &mut impl Write, row: u16, col: u16) -> io::Result<()> {
    write!(w, "\x1b[{};{}H", row, col)
}

// ── Terminal size ─────────────────────────────────────────────────────

#[cfg(unix)]
pub fn terminal_size() -> Option<(u16, u16)> {
    #[repr(C)]
    struct Winsize {
        ws_row: u16,
        ws_col: u16,
        _ws_xpixel: u16,
        _ws_ypixel: u16,
    }
    let mut ws = Winsize {
        ws_row: 0,
        ws_col: 0,
        _ws_xpixel: 0,
        _ws_ypixel: 0,
    };
    // TIOCGWINSZ = 0x5413 on Linux, 0x40087468 on macOS
    #[cfg(target_os = "linux")]
    const TIOCGWINSZ: u64 = 0x5413;
    #[cfg(target_os = "macos")]
    const TIOCGWINSZ: u64 = 0x40087468;
    let ret = unsafe { libc_ioctl(1, TIOCGWINSZ, &mut ws as *mut Winsize as *mut u8) };
    if ret == 0 && ws.ws_col > 0 && ws.ws_row > 0 {
        Some((ws.ws_col, ws.ws_row))
    } else {
        None
    }
}

#[cfg(windows)]
pub fn terminal_size() -> Option<(u16, u16)> {
    #[repr(C)]
    struct Coord {
        x: i16,
        y: i16,
    }
    #[repr(C)]
    struct SmallRect {
        left: i16,
        top: i16,
        right: i16,
        bottom: i16,
    }
    #[repr(C)]
    struct ConsoleScreenBufferInfo {
        dw_size: Coord,
        dw_cursor_position: Coord,
        w_attributes: u16,
        sr_window: SmallRect,
        dw_maximum_window_size: Coord,
    }
    unsafe extern "system" {
        fn GetStdHandle(nStdHandle: u32) -> *mut std::ffi::c_void;
        fn GetConsoleScreenBufferInfo(
            h: *mut std::ffi::c_void,
            info: *mut ConsoleScreenBufferInfo,
        ) -> i32;
    }
    let mut info = unsafe { std::mem::zeroed::<ConsoleScreenBufferInfo>() };
    let handle = unsafe { GetStdHandle(0xFFFF_FFF5) }; // STD_OUTPUT_HANDLE
    let ret = unsafe { GetConsoleScreenBufferInfo(handle, &mut info) };
    if ret != 0 {
        let cols = (info.sr_window.right - info.sr_window.left + 1) as u16;
        let rows = (info.sr_window.bottom - info.sr_window.top + 1) as u16;
        Some((cols, rows))
    } else {
        None
    }
}

#[cfg(not(any(unix, windows)))]
pub fn terminal_size() -> Option<(u16, u16)> {
    None
}

#[cfg(unix)]
unsafe extern "C" {
    #[link_name = "ioctl"]
    fn libc_ioctl(fd: i32, request: u64, ...) -> i32;
}

// ── Raw mode ──────────────────────────────────────────────────────────

#[cfg(unix)]
mod raw {
    use std::io;

    // Minimal termios FFI — no libc crate needed.
    // Field types must match the OS ABI exactly to avoid UB.

    // Linux: tcflag_t = u32, cc_t = u8, speed_t = u32, NCCS = 32
    #[cfg(target_os = "linux")]
    mod platform {
        pub const NCCS: usize = 32;
        pub type TcFlag = u32;

        #[repr(C)]
        #[derive(Clone)]
        pub struct Termios {
            pub c_iflag: TcFlag,
            pub c_oflag: TcFlag,
            pub c_cflag: TcFlag,
            pub c_lflag: TcFlag,
            pub c_line: u8,
            pub c_cc: [u8; NCCS],
            pub c_ispeed: TcFlag,
            pub c_ospeed: TcFlag,
        }

        pub const ECHO: TcFlag = 0o10;
        pub const ICANON: TcFlag = 0o2;
        pub const ISIG: TcFlag = 0o1;
        pub const IEXTEN: TcFlag = 0o100000;
        pub const TCGETS: u64 = 0x5401;
        pub const TCSETS: u64 = 0x5402;
    }

    // macOS: tcflag_t = u64 (unsigned long on 64-bit), NCCS = 20
    #[cfg(target_os = "macos")]
    mod platform {
        pub const NCCS: usize = 20;
        pub type TcFlag = u64;

        #[repr(C)]
        #[derive(Clone)]
        pub struct Termios {
            pub c_iflag: TcFlag,
            pub c_oflag: TcFlag,
            pub c_cflag: TcFlag,
            pub c_lflag: TcFlag,
            pub c_cc: [u8; NCCS],
            pub c_ispeed: TcFlag,
            pub c_ospeed: TcFlag,
        }

        pub const ECHO: TcFlag = 0x00000008;
        pub const ICANON: TcFlag = 0x00000100;
        pub const ISIG: TcFlag = 0x00000080;
        pub const IEXTEN: TcFlag = 0x00000400;
        pub const TCGETS: u64 = 0x40487413; // TIOCGETA
        pub const TCSETS: u64 = 0x80487414; // TIOCSETA
    }

    use platform::Termios;

    unsafe extern "C" {
        #[link_name = "ioctl"]
        fn libc_ioctl(fd: i32, request: u64, ...) -> i32;
    }

    pub struct RawMode {
        original: Termios,
    }

    impl RawMode {
        pub fn enable() -> io::Result<Self> {
            let mut orig = unsafe { std::mem::zeroed::<Termios>() };
            let ret = unsafe { libc_ioctl(0, platform::TCGETS, &mut orig as *mut Termios) };
            if ret != 0 {
                return Err(io::Error::last_os_error());
            }
            let saved = orig.clone();
            orig.c_lflag &=
                !(platform::ECHO | platform::ICANON | platform::ISIG | platform::IEXTEN);
            let ret = unsafe { libc_ioctl(0, platform::TCSETS, &orig as *const Termios) };
            if ret != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(RawMode { original: saved })
        }
    }

    impl Drop for RawMode {
        fn drop(&mut self) {
            unsafe {
                libc_ioctl(0, platform::TCSETS, &self.original as *const Termios);
            }
        }
    }
}

#[cfg(unix)]
pub use raw::RawMode;

// ── Windows raw mode ─────────────────────────────────────────────────

#[cfg(windows)]
mod raw {
    use std::io;

    // Windows Console API FFI
    type Handle = *mut std::ffi::c_void;
    type Dword = u32;
    const STD_INPUT_HANDLE: Dword = 0xFFFF_FFF6; // (DWORD)-10
    const ENABLE_ECHO_INPUT: Dword = 0x0004;
    const ENABLE_LINE_INPUT: Dword = 0x0002;
    const ENABLE_PROCESSED_INPUT: Dword = 0x0001;
    const ENABLE_VIRTUAL_TERMINAL_INPUT: Dword = 0x0200;

    // Enable ANSI escape sequences on Windows output
    const STD_OUTPUT_HANDLE: Dword = 0xFFFF_FFF5; // (DWORD)-11
    const ENABLE_VIRTUAL_TERMINAL_PROCESSING: Dword = 0x0004;

    unsafe extern "system" {
        fn GetStdHandle(nStdHandle: Dword) -> Handle;
        fn GetConsoleMode(hConsoleHandle: Handle, lpMode: *mut Dword) -> i32;
        fn SetConsoleMode(hConsoleHandle: Handle, dwMode: Dword) -> i32;
    }

    pub struct RawMode {
        input_handle: Handle,
        original_input_mode: Dword,
        output_handle: Handle,
        original_output_mode: Dword,
    }

    impl RawMode {
        pub fn enable() -> io::Result<Self> {
            unsafe {
                let input_handle = GetStdHandle(STD_INPUT_HANDLE);
                let output_handle = GetStdHandle(STD_OUTPUT_HANDLE);

                let mut original_input_mode: Dword = 0;
                if GetConsoleMode(input_handle, &mut original_input_mode) == 0 {
                    return Err(io::Error::last_os_error());
                }

                let mut original_output_mode: Dword = 0;
                let _ = GetConsoleMode(output_handle, &mut original_output_mode);

                // Disable echo and line input, enable virtual terminal input
                let new_input = (original_input_mode
                    & !(ENABLE_ECHO_INPUT | ENABLE_LINE_INPUT | ENABLE_PROCESSED_INPUT))
                    | ENABLE_VIRTUAL_TERMINAL_INPUT;
                if SetConsoleMode(input_handle, new_input) == 0 {
                    return Err(io::Error::last_os_error());
                }

                // Enable ANSI escape processing on output
                let new_output = original_output_mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING;
                let _ = SetConsoleMode(output_handle, new_output);

                Ok(RawMode {
                    input_handle,
                    original_input_mode,
                    output_handle,
                    original_output_mode,
                })
            }
        }
    }

    impl Drop for RawMode {
        fn drop(&mut self) {
            unsafe {
                SetConsoleMode(self.input_handle, self.original_input_mode);
                SetConsoleMode(self.output_handle, self.original_output_mode);
            }
        }
    }
}

#[cfg(windows)]
pub use raw::RawMode;

// ── Key input ─────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
pub enum Key {
    Up,
    Down,
    Space,
    Enter,
    Escape,
    Char(char),
    Unknown,
}

pub fn read_key() -> io::Result<Key> {
    let stdin = io::stdin();
    let mut stdin = stdin.lock();

    let mut first = [0u8; 1];
    if stdin.read(&mut first)? == 0 {
        return Ok(Key::Unknown);
    }

    if first[0] != 27 {
        return Ok(match first[0] {
            b' ' => Key::Space,
            13 | 10 => Key::Enter,
            c => Key::Char(c as char),
        });
    }

    // ESC received — read next bytes to disambiguate escape sequences.
    let mut seq = [0u8; 2];
    let n = stdin.read(&mut seq)?;
    if n == 0 {
        return Ok(Key::Escape);
    }

    if seq[0] == b'[' {
        // Got "ESC [", may need one more byte for the sequence letter.
        let letter = if n >= 2 {
            seq[1]
        } else {
            let mut last = [0u8; 1];
            if stdin.read(&mut last)? == 0 {
                return Ok(Key::Escape);
            }
            last[0]
        };
        return Ok(match letter {
            b'A' => Key::Up,
            b'B' => Key::Down,
            _ => Key::Unknown,
        });
    }

    Ok(Key::Escape)
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ansi_clear_screen() {
        let mut buf = Vec::new();
        clear_screen(&mut buf).unwrap();
        assert_eq!(buf, b"\x1b[2J\x1b[H");
    }

    #[test]
    fn ansi_hide_show_cursor() {
        let mut buf = Vec::new();
        hide_cursor(&mut buf).unwrap();
        assert_eq!(buf, b"\x1b[?25l");
        buf.clear();
        show_cursor(&mut buf).unwrap();
        assert_eq!(buf, b"\x1b[?25h");
    }

    #[test]
    fn ansi_move_to() {
        let mut buf = Vec::new();
        move_to(&mut buf, 3, 5).unwrap();
        assert_eq!(buf, b"\x1b[3;5H");
    }

    #[test]
    fn terminal_size_returns_something() {
        // May be None in CI, just ensure no panic
        let _ = terminal_size();
    }
}
