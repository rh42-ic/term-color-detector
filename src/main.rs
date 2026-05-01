use std::env;
use std::process;

#[derive(Debug, PartialEq)]
enum OutputFormat {
    Scheme,
    Rgb,
    Luma,
}

#[derive(Debug)]
struct Config {
    format: OutputFormat,
    osc_code: String,
    timeout_ms: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            format: OutputFormat::Scheme,
            osc_code: "11".to_string(),
            timeout_ms: 500,
        }
    }
}

fn parse_args() -> Config {
    let mut config = Config::default();
    let mut args = env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            // Format Arguments
            "-s" | "--scheme" => config.format = OutputFormat::Scheme,
            "-r" | "--rgb" => config.format = OutputFormat::Rgb,
            "-l" | "--luma" => config.format = OutputFormat::Luma,
            
            // Legacy format fallback for compatibility
            "-d" => config.format = OutputFormat::Scheme,
            
            // Target Arguments
            "-b" | "--bg" => config.osc_code = "11".to_string(),
            "-f" | "--fg" => config.osc_code = "10".to_string(),
            "-c" | "--cursor" => config.osc_code = "12".to_string(),
            "-p" | "--palette" => {
                if let Some(val) = args.next() {
                    config.osc_code = format!("4;{}", val);
                } else {
                    eprintln!("Missing palette index");
                    process::exit(1);
                }
            }
            "-o" | "--osc" => {
                if let Some(val) = args.next() {
                    config.osc_code = val.replace(",", ";").replace(":", ";");
                } else {
                    eprintln!("Missing OSC code");
                    process::exit(1);
                }
            }
            
            // General Arguments
            "-t" | "--timeout" => {
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
                println!("Usage: tcdet [TARGET] [FORMAT] [OPTIONS]");
                println!("Targets (Mutually Exclusive):");
                println!("  -b, --bg          Query background color (OSC 11) [Default]");
                println!("  -f, --fg          Query foreground color (OSC 10)");
                println!("  -c, --cursor      Query cursor color (OSC 12)");
                println!("  -p, --palette <N> Query ANSI palette color (OSC 4;N)");
                println!("  -o, --osc <CODE>  Query raw OSC code");
                println!("Formats (Mutually Exclusive):");
                println!("  -s, --scheme      Output 'dark' or 'light' [Default]");
                println!("  -r, --rgb         Output RGB hex (e.g., #RRGGBB)");
                println!("  -l, --luma        Output luma value (0-255)");
                println!("Options:");
                println!("  -t, --timeout <ms> Timeout in milliseconds (default 500)");
                println!("  -h, --help        Print help");
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

    pub fn query_terminal(osc_code: &str, timeout_ms: u64) -> Result<String, Error> {
        let tty = TtyState::new()?;
        let query_str = format!("\x1b]{};?\x07", osc_code);
        let query = query_str.as_bytes();

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

    pub fn query_terminal(osc_code: &str, timeout_ms: u64) -> Result<String, Error> {
        let tty = TtyState::new()?;
        let query_str = format!("\x1b]{};?\x07", osc_code);
        let query = query_str.as_bytes();

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

fn parse_rgb(osc_code: &str, resp: &str) -> Option<(u8, u8, u8)> {
    let search_pattern = format!("]{};rgb:", osc_code);
    let start_idx = resp.find(&search_pattern)?;
    let rgb_str = &resp[start_idx + search_pattern.len()..];
    
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
    match config.format {
        OutputFormat::Scheme => print!("dark"),
        OutputFormat::Rgb => print!("#000000"),
        OutputFormat::Luma => print!("0"),
    }
    process::exit(1);
}

fn main() {
    let config = parse_args();
    
    let resp = match query_terminal(&config.osc_code, config.timeout_ms) {
        Ok(r) => r,
        Err(_) => {
            print_failure(&config);
            unreachable!();
        }
    };

    let (r, g, b) = match parse_rgb(&config.osc_code, &resp) {
        Some(rgb) => rgb,
        None => {
            print_failure(&config);
            unreachable!();
        }
    };

    match config.format {
        OutputFormat::Scheme => {
            let luma = calculate_luma(r, g, b);
            if luma > 153 {
                print!("light");
            } else {
                print!("dark");
            }
        }
        OutputFormat::Rgb => {
            print!("#{:-02X}{:02X}{:02X}", r, g, b);
        }
        OutputFormat::Luma => {
            print!("{}", calculate_luma(r, g, b));
        }
    }
    process::exit(0);
}
