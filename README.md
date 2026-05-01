# term-color-detector

A fast, zero-dependency CLI tool to detect terminal colors (background, foreground, cursor, or palette) or extract their RGB/Luma values.

The core detection logic is extracted from [Yazi](https://github.com/sxyazi/yazi), fully optimized for speed and binary size to be seamlessly integrated into scripts.

## Features

- **Extreme Speed**: Bypasses heavy TUI libraries or async runtimes. Uses direct `/dev/tty` syscalls via `libc` and raw terminal mode `termios`.
- **Zero-Cost Math**: Computes the Luma value using the BT.709 standard formula (`Y ≈ 0.2126 R + 0.7152 G + 0.0722 B`) exclusively through integer bit-shifting for maximum performance.
- **Fail-Safe**: Includes a strict configurable timeout (default 50ms). In environments where OSC queries are unsupported or hanging, `tcdet` will safely exit with a default response and a `1` exit code, never hanging your scripts.

## Installation

You can compile it directly with Cargo. The `Cargo.toml` is already pre-configured to optimize for binary size (`opt-level = "z"`, `lto = true`, `strip = true`).

```bash
# Standard glibc build (Linux) or macOS/Windows
cargo build --release

# Musl build (Linux) for ultimate portability and startup speed
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl

cp target/release/tcdet ~/.local/bin/
```

### Pre-built Binaries
GitHub Actions automatically builds and publishes binaries for Linux (gnu/musl), macOS (x86_64/arm64), and Windows (x86_64) on every release tag. Check the [Releases](https://github.com/rh42-ic/term-color-detector/releases) page.

## Usage

```bash
tcdet [TARGET] [FORMAT] [-t <ms>]
```

### Options

`tcdet` uses an orthogonal flag design: you pick **one target** to query and **one format** for the output.

#### Targets (What to query)
If no target is specified, `-b` (Background) is used.

| Flag | Long Flag | Description | OSC Code |
|------|-----------|-------------|----------|
| `-b` | `--bg` | **[Default]** Background color | OSC 11 |
| `-f` | `--fg` | Foreground color | OSC 10 |
| `-c` | `--cursor`| Cursor color | OSC 12 |
| `-p` | `--palette <idx>` | Palette color at index | OSC 4 |
| `-o` | `--osc <code>`| Raw OSC query code | Custom |

#### Formats (How to output)
If no format is specified, `-s` (Scheme) is used.

| Flag | Long Flag | Description | Success Output | Timeout / Failure | Exit Code |
|------|-----------|-------------|----------------|-------------------|-----------|
| `-s` | `--scheme` | **[Default]** Dark/Light mode | `dark` or `light` | `dark` | 0 (Success) / 1 (Failure) |
| `-r` | `--rgb` | RGB Hex format | e.g., `#1E1E2E` | `#000000` | 0 (Success) / 1 (Failure) |
| `-l` | `--luma` | Luma value | Integer `0-255` | `0` | 0 (Success) / 1 (Failure) |

#### General Options

| Flag | Long Flag | Description | Default |
|------|-----------|-------------|---------|
| `-t` | `--timeout <ms>`| Timeout for the terminal response | `500` ms |
| `-h` | `--help` | Show help message | |

### Examples

**1. Basic Theme Detection (Background)**
```bash
THEME=$(tcdet)
if [ "$THEME" = "light" ]; then
    echo "Terminal is light!"
else
    echo "Terminal is dark!"
fi
```

**2. Getting Foreground RGB**
```bash
$ tcdet -f -r
#D9E0EE
```

**3. Getting Cursor Luma**
```bash
$ tcdet -c -l
200
```

**4. Getting Palette Color 4 in RGB**
```bash
$ tcdet -p 4 -r
#F28FAD
```

**5. Adjusting the Timeout**
For local terminals, single-digit milliseconds are usually enough. However, over a remote SSH connection, the terminal's response time depends on the network's Round-Trip Time (RTT). If the timeout is set too short, `tcdet` will exit and the late-arriving response will leak into your terminal prompt as ugly raw text (like `]11;rgb:0000/0000/0000\`).

```bash
$ tcdet -b -s -t 1000
dark
```

## How It Works

1. Saves current `termios` state.
2. Enters raw mode (`ECHO` and `ICANON` disabled).
3. Sends the appropriate OSC query based on the target (e.g., `\x1b]11;?\x07` for background, `\x1b]10;?\x07` for foreground).
4. Uses `select` to poll for the response (e.g., `\x1b]11;rgb:RRRR/GGGG/BBBB\x07`) within the timeout window.
5. Parses the RGB components from the response.
6. Calculates the Luma using the integer-optimized BT.709 formula: `(R*218 + G*732 + B*74 + 512) >> 10`.
7. Checks if the Luma crosses the `153` threshold to determine `light` or `dark` (if scheme format is requested).
8. Restores the exact `termios` state and exits.

## Credits

This project is a specialized extraction and optimization of the terminal background detection logic found in [Yazi](https://github.com/sxyazi/yazi). Special thanks to the Yazi team for their robust implementation.

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
