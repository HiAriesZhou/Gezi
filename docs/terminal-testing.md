# 终端验收

## 自动检查

先运行格式、Clippy 和 Rust 测试，再构建待验收版本：

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release --locked
python3 scripts/check_stream_pty.py
python3 scripts/test_stream_pty.py --binary target/release/tmd
```

PTY 检查只使用 Python 标准库和 `/bin/bash`，适用于当前 MVP 的 macOS/Linux；不安装依赖，不读取用户 shell 配置，不保存 shell 历史。可用 `--binary <路径>` 指定构建产物。

它发送带中文和 emoji 的分段 UTF-8 代码块，检查：

- stdout 是终端时产生多个同步输出帧。
- 从 80 列缩到 60 列后，输出确实使用新的宽度。
- 正常结束返回 0；Ctrl-C 结束返回 130，shell 能继续执行命令。
- 光标、设置过的 Unicode 版本和终端属性恢复。

输出每个场景的 JSON 检查结果；失败返回非零。探测用 `TERM_PROGRAM=Kaku` 覆盖协议分支，不表示运行在真实 Kaku 中。

CI 在 Linux/macOS 上构建 debug 二进制并执行同一探测。缩放和中断以已经观察到的输出为触发条件，避免依赖机器启动速度。启动回归测试故意让父进程晚运行，验证脚本仍在 shell 改变输入模式前保存终端属性基线。

## 真实终端检查

PTY 通过不能证明无闪烁、字体宽度和终端兼容性。在 Kaku 主测，并按 `docs/TASKS.md` 和 `docs/research.md` 在 Ghostty、kitty 补测：

```sh
python3 scripts/slowcat.py fixtures/long.md | target/release/tmd
target/release/tmd --width 80 fixtures/cjk-table.md
```

检查长输出不闪、中文/emoji 表格对齐、代码块逐行出现、活动区不超过终端高度，以及正常结束后光标可用。流式期间缩放窗口，确认只重排活动区；再运行一次并按 Ctrl-C，确认上游结束、光标和输入正常。

记录实际使用的终端、执行版本和结果；未运行的终端明确标为待验收。自动检查、视觉验收和作者接受分别报告。
