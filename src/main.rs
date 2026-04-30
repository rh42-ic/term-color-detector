use libc::{c_int, termios, tcgetattr, tcsetattr, TCSANOW, ECHO, ICANON, VMIN, VTIME, fd_set, FD_ZERO, FD_SET, select, timeval, read, write, O_RDWR};
use std::env;
use std::ffi::CString;
use std::io::Error;
use std::process;
use std::ptr;

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

struct TtyState {
    fd: c_int,
    original: termios,
}

impl TtyState {
    fn new() -> Result<Self, Error> {
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

fn query_terminal(timeout_ms: u64) -> Result<String, Error> {
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
    let l_int = (r as u32 * 2627 + g as u32 * 6780 + b as u32 * 593) / 10000;
    l_int as u8
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
