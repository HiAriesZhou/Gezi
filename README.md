# tmd

终端里的 markdown 渲染器，命令 `tmd`。只读，主打流式不闪、中英文对齐、主题。

面向日常试用的早期版本，优先在 Kaku 中检查阅读和流式输出效果。

## 源码安装

需要 Git 和 Rust 稳定版工具链。将构建产物安装到 `~/.local/bin`；已有同名 `tmd` 会被替换。

```sh
git clone git@github.com:HiAriesZhou/tmd.git
cd tmd
cargo build --release --locked
mkdir -p "$HOME/.local/bin"
install -m 755 target/release/tmd "$HOME/.local/bin/tmd"
```

若该目录尚未在 PATH 中，先执行以下命令；长期使用时可将它加入 shell 配置。

```sh
export PATH="$HOME/.local/bin:$PATH"
tmd --version
```

## 用法

```sh
tmd README.md          # 渲染文件
cat notes.md | tmd     # 从 stdin 读
tmd --help             # 全部参数
```

支持标题、文字强调、列表、引用、链接、代码高亮和中英文表格。标题使用颜色、粗细和分隔线区分层级，字号由终端设置决定。

```sh
tmd --theme neon-tokyo README.md
tmd --width 80 fixtures/acceptance.md
python3 scripts/slowcat.py fixtures/acceptance.md --rate 600 | tmd
```

内置主题为 `default-dark`、`default-light` 和 `neon-tokyo`，默认 `auto` 根据终端背景选择亮暗主题。自定义主题见 [主题说明](themes/README.md)。

## 当前限制

- 主要在 Kaku 中试用；Ghostty、kitty 的显示效果仍待补测。
- `--from claude-json` 已有真实样本回放测试，真实 Claude 增量管道仍待补测。
- 暂无全屏查看器、搜索、图片显示和 Markdown 标题字号变化。
- 流式缩放只重排活动区，已进入终端历史的内容不重绘。

版本变化见 [CHANGELOG](CHANGELOG.md)，验收步骤见 [终端验收](docs/terminal-testing.md)。

维护者合并版本更新后，主分支 CI 全部通过会自动创建 tag 和 GitHub Release，具体流程见 [自动发布](docs/release.md)。

## 开发

```sh
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
```

## 许可证

MIT
