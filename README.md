简体中文 [README.zh-CN.md](./README.zh-CN.md) | English

# term-color-detector

A fast, zero-dependency CLI tool to detect terminal colors (background, foreground, cursor, or palette) or extract their RGB/Luma values.

The core detection logic is extracted from [Yazi](https://github.com/sxyazi/yazi), fully optimized for speed and binary size to be seamlessly integrated into scripts.

## Features

- **Speed**: Uses direct `/dev/tty` syscalls via `libc` and raw terminal mode `termios`.
- **Fail-Safe**: Includes a strict configurable timeout (default 500ms). In environments where OSC queries are unsupported or hanging, `tcdet` will safely exit with a default response and a `1` exit code, never hanging your scripts.

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

`tcdet` uses an orthogonal design: you pick one **Target** to query and one **Format** for the output.

#### Targets (What to query)

_If omitted, defaults to Background (`-b`)._

| Flag        | Long Flag   | Description                    | OSC Code |
| :---------- | :---------- | :----------------------------- | :------- |
| `-b`        | `--bg`      | **Background** color (Default) | OSC 11   |
| `-f`        | `--fg`      | **Foreground** color           | OSC 10   |
| `-c`        | `--cursor`  | **Cursor** color               | OSC 12   |
| `-p <n>`    | `--palette` | **Palette** color at index `n` | OSC 4;n  |
| `-o <code>` | `--osc`     | **Raw** OSC query code         | Custom   |

#### Formats (How to output)

_If omitted, defaults to Scheme (`-s`)._

| Flag | Long Flag  | Output on Success | Fallback / Timeout |
| :--- | :--------- | :---------------- | :----------------- |
| `-s` | `--scheme` | `dark` or `light` | `dark`             |
| `-r` | `--rgb`    | `#RRGGBB` (Hex)   | `#000000`          |
| `-l` | `--luma`   | `0-255` (Integer) | `0`                |

#### General

| Flag      | Long Flag   | Description                     | Default |
| :-------- | :---------- | :------------------------------ | :------ |
| `-t <ms>` | `--timeout` | Wait time for terminal response | `500ms` |
| `-h`      | `--help`    | Display help message            |         |

### Examples

**1. Shell Configuration (.bashrc/.zshrc)**
The most common use case is dynamically setting themes for your CLI tools. Since `tcdet` returns a non-zero exit code on failure, you can easily provide a fallback value within your script.

```bash
# Detect terminal color scheme (default to 'light' on failure or timeout)
# We use a 200ms timeout to account for network latency in SSH
if THEME=$(tcdet -s -t 200 2>/dev/null); then
    export TERMINAL_SCHEME="$THEME"
else
    export TERMINAL_SCHEME="light"
fi

# Example usage: Set theme for 'bat' and 'starship'
if [ "$TERMINAL_SCHEME" = "light" ]; then
    export BAT_THEME="GitHub"
    export STARSHIP_CONFIG="$HOME/.config/starship.light.toml"
else
    export BAT_THEME="Catppuccin-Mocha"
    export STARSHIP_CONFIG="$HOME/.config/starship.dark.toml"
fi
```

**2. Getting Foreground Luma**
Get an integer from 0 - 255 representing brightness. With this number, you can easily use custom light/dark thresholds.

```bash
$ tcdet -f -l
200
```

**3. Getting Palette Color 4 in RGB**

```bash
$ tcdet -p 4 -r
#F28FAD
```

**4. Adjusting the Timeout**
For local terminals, single-digit milliseconds are usually enough. However, over a remote SSH connection, the terminal's response time depends on the network's Round-Trip Time (RTT). If the timeout is set too short, `tcdet` will exit and the late-arriving response will leak into your terminal prompt as ugly raw text (like `]11;rgb:0000/0000/0000\`). It's best not to set it too small.

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
