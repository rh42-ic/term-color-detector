[English](./README.md) | 简体中文

# term-color-detector

一个快速、零依赖的 CLI 工具，用于检测终端颜色（背景、前景、光标或调色板）并提取其 RGB/亮度（Luma）值。

核心检测逻辑提取自 [Yazi](https://github.com/sxyazi/yazi)，并针对速度和二进制大小进行了全面优化，以便无缝集成到脚本中。

## 特性

- **极速**: 直接通过 `libc` 调用 `/dev/tty` 系统调用，并使用 `termios` 原始终端模式。
- **故障安全**: 内置严格的可配置超时（默认 500ms）。在不支持 OSC 查询或发生挂起的环境中，`term-color-det` 将安全退出并返回默认响应及 `1` 退出码，绝不会导致脚本挂起。

## 安装

你可以直接使用 Cargo 编译。`Cargo.toml` 已预先配置为优化二进制大小（`opt-level = "z"`, `lto = true`, `strip = true`）。

```bash
# 标准 glibc 构建 (Linux) 或 macOS/Windows
cargo build --release

# Musl 构建 (Linux) 以获得极致的可移植性和启动速度
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl

cp target/release/term-color-det ~/.local/bin/
```

### 预编译二进制文件

GitHub Actions 会在每个发布标签（Release tag）自动构建并发布适用于 Linux (gnu/musl)、macOS (x86_64/arm64) 和 Windows (x86_64) 的二进制文件。请查看 [Releases](https://github.com/rh42-ic/term-color-detector/releases) 页面。

## 用法

```bash
term-color-det [目标] [格式] [-t <毫秒>]
```

### 选项

`term-color-det` 采用正交化设计：你需要选择一个**查询目标（Target）**和一种**输出格式（Format）**。

#### 查询目标 (Targets)

*如果省略，默认为背景色 (`-b`)。*

| 标志        | 长标志      | 描述                           | OSC 代码 |
| :---------- | :---------- | :----------------------------- | :------- |
| `-b`        | `--background` | **背景** 颜色 (默认)           | OSC 11   |
| `-f`        | `--foreground` | **前景** 颜色                  | OSC 10   |
| `-c`        | `--cursor`     | **光标** 颜色                  | OSC 12   |
| `-p <n>`    | `--palette`    | **调色板** 索引 `n` 处的颜色   | OSC 4;n  |
| `-o <code>` | `--osc`        | **原始** OSC 查询代码          | 自定义   |

#### 输出格式 (Formats)

*如果省略，默认为配色方案 (`-s`)。*

| 标志 | 长标志     | 成功时的输出      | 回退值 / 超时时输出 |
| :--- | :--------- | :---------------- | :------------------ |
| `-s` | `--scheme` | `dark` 或 `light` | `dark`              |
| `-r` | `--rgb`    | `#RRGGBB` (十六进制) | `#000000`           |
| `-l` | `--luma`   | `0-255` (整数)    | `0`                 |

#### 通用选项

| 标志      | 长标志      | 描述                         | 默认值  |
| :-------- | :---------- | :--------------------------- | :------ |
| `-t <ms>` | `--timeout` | 等待终端响应的时间           | `500ms` |
|           | `--rtt`     | 打印往返时延 (RTT)           |         |
| `-h`      | `--help`    | 显示帮助信息                 |         |

### 示例

**1. Shell 配置 (.bashrc/.zshrc)**
最常见的用例是为你的 CLI 工具动态设置主题。由于 `term-color-det` 在失败时返回非零退出码，你可以轻松地在脚本中提供回退值。

```bash
# 检测终端配色方案（失败或超时时默认设为 'light'）
# 我们使用 200ms 的超时以考虑 SSH 环境下的网络延迟
if THEME=$(term-color-det -s -t 200 2>/dev/null); then
    export TERMINAL_SCHEME="$THEME"
else
    export TERMINAL_SCHEME="light"
fi

# 示例用法：为 'bat' 和 'starship' 设置主题
if [ "$TERMINAL_SCHEME" = "light" ]; then
    export BAT_THEME="GitHub"
    export STARSHIP_CONFIG="$HOME/.config/starship.light.toml"
else
    export BAT_THEME="Catppuccin-Mocha"
    export STARSHIP_CONFIG="$HOME/.config/starship.dark.toml"
fi
```

**2. 获取前景色的亮度 (Luma)**
获取一个 0 - 255 的整数来表示亮度。通过这个数字，可以方便的使用自定义的明暗阈值。

```bash
$ term-color-det -f -l
200
```

**3. 获取调色板索引 4 的 RGB 值**

```bash
$ term-color-det -p 4 -r
#F28FAD
```

**4. 调整超时时间**
对于本地终端，个位数毫秒通常就足够了。然而，在远程 SSH 连接中，终端的响应时间取决于网络的往返时延 (RTT)。如果超时设置得太短，`term-color-det` 会退出，而姗姗来迟的响应将作为难看的原始文本（如 `]11;rgb:0000/0000/0000\`）泄漏到你的终端提示符中。最好不要设置的太小。

```bash
$ term-color-det -b -s -t 1000
dark
```

## 工作原理

1. 保存当前的 `termios` 状态。
2. 进入原始（raw）模式（禁用 `ECHO` 和 `ICANON`）。
3. 根据目标发送相应的 OSC 查询（例如，背景色为 `\x1b]11;?\x07`，前景色为 `\x1b]10;?\x07`）。
4. 使用 `select` 在超时窗口内轮询响应（通常格式为 `\x1b]11;rgb:RRRR/GGGG/BBBB\x07`）。
5. 从响应中解析 RGB 组件。
6. 使用针对整数优化的 BT.709 公式计算亮度（Luma）：`(R*218 + G*732 + B*74 + 512) >> 10`。
7. 检查亮度是否超过 `153` 阈值以确定是 `light` 还是 `dark`（如果请求了 scheme 格式）。
8. 恢复精确的 `termios` 状态并退出。

## 致谢

本项目是针对 [Yazi](https://github.com/sxyazi/yazi) 中终端背景检测逻辑的专门提取和优化。特别感谢 Yazi 团队健壮的实现。

## 许可证

本项目采用 MIT 许可证。详情请参阅 [LICENSE](LICENSE) 文件。
