// Zero-dependency terminal handling: raw mode, ANSI codes, input parsing.
#![allow(dead_code)]

use std::io::{self, Read, Write};

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
    let ret =
        unsafe { libc_ioctl(1, TIOCGWINSZ, &mut ws as *mut Winsize as *mut u8) };
    if ret == 0 && ws.ws_col > 0 && ws.ws_row > 0 {
        Some((ws.ws_col, ws.ws_row))
    } else {
        None
    }
}

#[cfg(not(unix))]
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
    #[cfg(target_os = "linux")]
    const NCCS: usize = 32;
    #[cfg(target_os = "macos")]
    const NCCS: usize = 20;

    #[repr(C)]
    #[derive(Clone)]
    pub struct Termios {
        pub c_iflag: u64,
        pub c_oflag: u64,
        pub c_cflag: u64,
        pub c_lflag: u64,
        #[cfg(target_os = "linux")]
        pub c_line: u8,
        pub c_cc: [u8; NCCS],
        #[cfg(target_os = "macos")]
        pub c_ispeed: u64,
        #[cfg(target_os = "macos")]
        pub c_ospeed: u64,
    }

    // Linux flags
    #[cfg(target_os = "linux")]
    mod flags {
        pub const ECHO: u64 = 0o10;
        pub const ICANON: u64 = 0o2;
        pub const ISIG: u64 = 0o1;
        pub const IEXTEN: u64 = 0o100000;
        pub const TCGETS: u64 = 0x5401;
        pub const TCSETS: u64 = 0x5402;
    }

    // macOS flags
    #[cfg(target_os = "macos")]
    mod flags {
        pub const ECHO: u64 = 0x00000008;
        pub const ICANON: u64 = 0x00000100;
        pub const ISIG: u64 = 0x00000080;
        pub const IEXTEN: u64 = 0x00000400;
        pub const TCGETS: u64 = 0x40487413; // TIOCGETA
        pub const TCSETS: u64 = 0x80487414; // TIOCSETA
    }

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
            let ret = unsafe { libc_ioctl(0, flags::TCGETS, &mut orig as *mut Termios) };
            if ret != 0 {
                return Err(io::Error::last_os_error());
            }
            let saved = orig.clone();
            orig.c_lflag &= !(flags::ECHO | flags::ICANON | flags::ISIG | flags::IEXTEN);
            let ret = unsafe { libc_ioctl(0, flags::TCSETS, &orig as *const Termios) };
            if ret != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(RawMode { original: saved })
        }
    }

    impl Drop for RawMode {
        fn drop(&mut self) {
            unsafe {
                libc_ioctl(0, flags::TCSETS, &self.original as *const Termios);
            }
        }
    }
}

#[cfg(unix)]
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
    let mut buf = [0u8; 4];
    let n = io::stdin().lock().read(&mut buf)?;
    if n == 0 {
        return Ok(Key::Unknown);
    }
    Ok(match buf[0] {
        27 if n >= 3 && buf[1] == b'[' => match buf[2] {
            b'A' => Key::Up,
            b'B' => Key::Down,
            _ => Key::Unknown,
        },
        27 => Key::Escape,
        b' ' => Key::Space,
        13 | 10 => Key::Enter,
        c => Key::Char(c as char),
    })
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
