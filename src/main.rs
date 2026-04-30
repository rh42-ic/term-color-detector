use std::env;
use std::process;

#[derive(Debug, PartialEq)]
enum OutputMode {
    DarkLight,
    Rgb,
    Luma,
}

#[derive(Debug)]
struct Config {
    mode: OutputMode,
    timeout_ms: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mode: OutputMode::DarkLight,
            timeout_ms: 50,
        }
    }
}

fn parse_args() -> Config {
    let mut config = Config::default();
    let mut args = env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-d" => config.mode = OutputMode::DarkLight,
            "-r" => config.mode = OutputMode::Rgb,
            "-l" => config.mode = OutputMode::Luma,
            "-t" => {
                if let Some(val) = args.next() {
                    if let Ok(ms) = val.parse::<u64>() {
                        config.timeout_ms = ms;
                    } else {
                        eprintln!("Invalid timeout value");
                        process::exit(1);
                    }
                } else {
                    eprintln!("Missing timeout value");
                    process::exit(1);
                }
            }
            "-h" | "--help" => {
                println!("Usage: term-bg [-d|-r|-l] [-t <ms>]");
                println!("  -d  Output 'dark' or 'light' (default)");
                println!("  -r  Output RGB hex (e.g., #RRGGBB)");
                println!("  -l  Output luma value (0-255)");
                println!("  -t  Timeout in milliseconds (default 50)");
                process::exit(0);
            }
            _ => {
                eprintln!("Unknown argument: {}", arg);
                process::exit(1);
            }
        }
    }
    config
}

#[cfg(unix)]
mod tty {
    use libc::{c_int, fd_set, read, select, tcgetattr, tcsetattr, termios, timeval, write, ECHO, FD_SET, FD_ZERO, ICANON, O_RDWR, TCSANOW, VMIN, VTIME};
    use std::ffi::CString;
    use std::io::Error;
    use std::ptr;

    pub struct TtyState {
        fd: c_int,
        original: termios,
    }

    impl TtyState {
        pub fn new() -> Result<Self, Error> {
            unsafe {
                let path = CString::new("/dev/tty").unwrap();
                let fd = libc::open(path.as_ptr(), O_RDWR);
                if fd < 0 {
                    return Err(Error::last_os_error());
                }

                let mut original: termios = std::mem::zeroed();
                if tcgetattr(fd, &mut original) != 0 {
                    libc::close(fd);
                    return Err(Error::last_os_error());
                }

                let mut raw = original;
                raw.c_lflag &= !(ECHO | ICANON);
                raw.c_cc[VMIN] = 0;
                raw.c_cc[VTIME] = 0;

                if tcsetattr(fd, TCSANOW, &raw) != 0 {
                    libc::close(fd);
                    return Err(Error::last_os_error());
                }

                Ok(Self { fd, original })
            }
        }
    }

    impl Drop for TtyState {
        fn drop(&mut self) {
            unsafe {
                tcsetattr(self.fd, TCSANOW, &self.original);
                libc::close(self.fd);
            }
        }
    }

    pub fn query_terminal(timeout_ms: u64) -> Result<String, Error> {
        let tty = TtyState::new()?;
        let query = b"\x1b]11;?\x07";

        unsafe {
            if write(tty.fd, query.as_ptr() as *const libc::c_void, query.len()) < 0 {
                return Err(Error::last_os_error());
            }

            let mut read_fds: fd_set = std::mem::zeroed();
            FD_ZERO(&mut read_fds);
            FD_SET(tty.fd, &mut read_fds);

            let mut timeout = timeval {
                tv_sec: (timeout_ms / 1000) as libc::time_t,
                tv_usec: ((timeout_ms % 1000) * 1000) as libc::suseconds_t,
            };

            let ret = select(
                tty.fd + 1,
                &mut read_fds,
                ptr::null_mut(),
                ptr::null_mut(),
                &mut timeout,
            );

            if ret <= 0 {
                // Timeout or error
                return Err(Error::from_raw_os_error(libc::ETIMEDOUT));
            }

            let mut buf = [0u8; 64];
            let n = read(tty.fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len());
            if n < 0 {
                return Err(Error::last_os_error());
            }

            Ok(String::from_utf8_lossy(&buf[..n as usize]).into_owned())
        }
    }
}

#[cfg(windows)]
mod tty {
    use std::io::Error;
    use std::ptr;
    use windows_sys::Win32::{
        Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE, WAIT_FAILED, WAIT_OBJECT_0},
        Storage::FileSystem::{CreateFileA, ReadFile, WriteFile, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING},
        System::Console::{GetConsoleMode, SetConsoleMode, ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT},
        System::Threading::WaitForSingleObject,
    };

    pub struct TtyState {
        in_handle: HANDLE,
        out_handle: HANDLE,
        original_mode: u32,
    }

    impl TtyState {
        pub fn new() -> Result<Self, Error> {
            unsafe {
                let in_handle = CreateFileA(
                    b"CONIN$\0".as_ptr(),
                    GENERIC_READ | GENERIC_WRITE,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    ptr::null_mut(),
                    OPEN_EXISTING,
                    0,
                    ptr::null_mut(),
                );
                
                let out_handle = CreateFileA(
                    b"CONOUT$\0".as_ptr(),
                    GENERIC_READ | GENERIC_WRITE,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    ptr::null_mut(),
                    OPEN_EXISTING,
                    0,
                    ptr::null_mut(),
                );

                if in_handle == INVALID_HANDLE_VALUE || out_handle == INVALID_HANDLE_VALUE {
                    if in_handle != INVALID_HANDLE_VALUE { CloseHandle(in_handle); }
                    if out_handle != INVALID_HANDLE_VALUE { CloseHandle(out_handle); }
                    return Err(Error::last_os_error());
                }

                let mut original_mode = 0;
                if GetConsoleMode(in_handle, &mut original_mode) == 0 {
                    CloseHandle(in_handle);
                    CloseHandle(out_handle);
                    return Err(Error::last_os_error());
                }

                let raw_mode = original_mode & !(ENABLE_ECHO_INPUT | ENABLE_LINE_INPUT);
                if SetConsoleMode(in_handle, raw_mode) == 0 {
                    CloseHandle(in_handle);
                    CloseHandle(out_handle);
                    return Err(Error::last_os_error());
                }

                Ok(Self {
                    in_handle,
                    out_handle,
                    original_mode,
                })
            }
        }
    }

    impl Drop for TtyState {
        fn drop(&mut self) {
            unsafe {
                SetConsoleMode(self.in_handle, self.original_mode);
                CloseHandle(self.in_handle);
                CloseHandle(self.out_handle);
            }
        }
    }

    pub fn query_terminal(timeout_ms: u64) -> Result<String, Error> {
        let tty = TtyState::new()?;
        let query = b"\x1b]11;?\x07";

        unsafe {
            let mut written = 0;
            if WriteFile(tty.out_handle, query.as_ptr() as _, query.len() as u32, &mut written, ptr::null_mut()) == 0 {
                return Err(Error::last_os_error());
            }

            match WaitForSingleObject(tty.in_handle, timeout_ms as u32) {
                WAIT_OBJECT_0 => {}
                WAIT_FAILED => return Err(Error::last_os_error()),
                _ => return Err(Error::from_raw_os_error(110)), // ETIMEDOUT equivalent
            }

            let mut buf = [0u8; 64];
            let mut read_bytes = 0;
            if ReadFile(tty.in_handle, buf.as_mut_ptr() as _, buf.len() as u32, &mut read_bytes, ptr::null_mut()) == 0 {
                return Err(Error::last_os_error());
            }

            Ok(String::from_utf8_lossy(&buf[..read_bytes as usize]).into_owned())
        }
    }
}

use tty::query_terminal;

fn parse_rgb(resp: &str) -> Option<(u8, u8, u8)> {
    // Look for "]11;rgb:"
    let start_idx = resp.find("]11;rgb:")?;
    let rgb_str = &resp[start_idx + 8..];
    
    let parts: Vec<&str> = rgb_str.split('/').collect();
    if parts.len() < 3 {
        return None;
    }

    // Parse the first 2 characters of each component as hex
    let r_str = parts[0].get(0..2).unwrap_or(parts[0]);
    let g_str = parts[1].get(0..2).unwrap_or(parts[1]);
    let b_str = parts[2].split('\x07').next()?.split('\x1b').next()?;
    let b_str = b_str.get(0..2).unwrap_or(b_str);

    let r = u8::from_str_radix(r_str, 16).ok()?;
    let g = u8::from_str_radix(g_str, 16).ok()?;
    let b = u8::from_str_radix(b_str, 16).ok()?;

    Some((r, g, b))
}

fn calculate_luma(r: u8, g: u8, b: u8) -> u8 {
    let l_int = r as u32 * 218 + g as u32 * 732 + b as u32 * 74 + 512;
    (l_int >> 10) as u8
}

fn print_failure(config: &Config) {
    match config.mode {
        OutputMode::DarkLight => print!("dark"),
        OutputMode::Rgb => print!("0"),
        OutputMode::Luma => print!("0"),
    }
    process::exit(1);
}

fn main() {
    let config = parse_args();
    
    let resp = match query_terminal(config.timeout_ms) {
        Ok(r) => r,
        Err(_) => {
            print_failure(&config);
            unreachable!();
        }
    };

    let (r, g, b) = match parse_rgb(&resp) {
        Some(rgb) => rgb,
        None => {
            print_failure(&config);
            unreachable!();
        }
    };

    match config.mode {
        OutputMode::DarkLight => {
            let luma = calculate_luma(r, g, b);
            if luma > 153 {
                print!("light");
            } else {
                print!("dark");
            }
        }
        OutputMode::Rgb => {
            print!("#{:-02X}{:02X}{:02X}", r, g, b);
        }
        OutputMode::Luma => {
            print!("{}", calculate_luma(r, g, b));
        }
    }
    process::exit(0);
}
