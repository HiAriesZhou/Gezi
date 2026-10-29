# TMD 任务拆分

按顺序做，一个任务一次提交。每个任务做完跑"完成标准"，全部通过再停下来等确认。

T1–T7 是 MVP：做完就能日常用（看文件、接管道、接 Claude Code）。T8 以后是 MVP 之后。

| 任务 | 内容 | 依赖 | 属于 |
|---|---|---|---|
| T1 | 仓库骨架、命令行参数、CI | — | MVP |
| T2 | 核心渲染（除表格、高亮） | T1 | MVP |
| T3 | 宽度计算与表格 | T2 | MVP |
| T4 | 代码高亮 | T2 | MVP |
| T5 | 主题 | T2 | MVP |
| T6 | 流式渲染（纯文本管道） | T3 T4 T5 | MVP |
| T7 | Claude Code 事件流 | T6 | MVP |
| T8 | 全屏查看器 | T6 | 之后 |
| T9 | 截图导出 | T5 | 之后 |
| T10 | README 与发布素材 | T7 | 之后 |

---

## T1 仓库骨架

做：
- cargo workspace，两个 crate：`crates/tmd-core`（库，不依赖任何终端库）、`crates/tmd-cli`（二进制名 `tmd`）。架构文档里的 tmd-term、tmd-view 先不拆，代码放 tmd-cli，等需要时再拆。
- clap 参数：`tmd [FILE]`、`--ansi`（强制输出颜色码文本）、`--theme <NAME>`、`--width <N>`（默认取终端宽度，非终端时 80）、`--from <text|claude-json>`（默认 text）。
- 没给 FILE 就读 stdin。这一步先原样输出。
- GitHub Actions：fmt --check、clippy -D warnings、test，Linux 和 macOS。
- MIT 许可证。

完成标准：
- `cargo run -- README.md` 和 `cat README.md | cargo run` 都输出内容。
- `cargo run -- --help` 显示上面所有参数。
- CI 通过。

## T2 核心渲染

做：
- tmd-core 的数据结构：
  - `Style { fg: Color, bg: Color, bold, italic, underline, strike, dim, link: Option<String> }`
  - `Color = Reset | Ansi(u8) /* 0–15 */ | Rgb(u8,u8,u8)`
  - `Span { text: String, style: Style }`，`Line { spans: Vec<Span>, src: Range<usize> }`。`src` 是这一行所属块的源码字节区间（一个段落换行成多行时，这几行的 `src` 相同），T6 要用。块之间的空行归属前一个块。
  - `fn render(md: &str, width: u16, theme: &Theme) -> Vec<Line>`
- 支持：
  - 标题 1–6 级，不显示 `#`。一级粗体加色；二级粗体加色，下面一行细线；三级以下只变色。
  - 段落按宽度换行（这一步先按字符数，T3 换成显示宽度）。
  - 粗体、斜体、删除线、行内代码（背景色）。
  - 链接：显示文字，样式带下划线；`style.link` 存 URL。
  - 无序、有序、嵌套列表，任务列表（☐ ☑）。
  - 引用（左边竖线，可嵌套）。
  - 分隔线。
  - 代码块：背景色、右上角语言名，先不高亮。
  - 图片显示为 `[图片: alt]`；HTML 原样灰色显示。
  - 块之间空一行。
- `--theme` 在 T5 之前只有一个写死的默认主题。
- tmd-cli 的 ANSI 输出：`Line` 转成带 SGR 颜色码的字符串；stdout 是终端时链接用 OSC 8 包裹。
- `fixtures/` 建以下文件，内容自己写，要覆盖上面每一项：`basic.md`、`lists.md`、`quotes.md`、`code.md`。
- 用 insta 做快照测试：每个 fixture 存两份快照，一份去掉颜色码的纯文本，一份带颜色码。

完成标准：
- `cargo test` 通过，快照已提交。
- `cargo run -- fixtures/basic.md` 在终端里目测：没有 `#`、`**`、`` ` `` 等标记露出来。

## T3 宽度与表格

做：
- 所有宽度用 unicode-width 的 `UnicodeWidthStr::width()`，不要调 `width_cjk()`。
- 换行按显示宽度：英文按单词断，中文可以在任意两个字之间断；一个英文单词比整行还宽时硬断。
- 表格：
  - 列宽 = 该列内容的最大显示宽度。
  - 支持 `:--`、`:-:`、`--:` 三种对齐。
  - 边框用 `┌ ┬ ┐ ├ ┼ ┤ └ ┴ ┘ │ ─`，表头粗体。
  - 总宽超过终端宽度时，反复把当前最宽的列压窄一格，单元格内换行；压到每列只剩 4 格还放不下，就截断并在行尾加 `…`。
- 新 fixture：`cjk-table.md`（中英文混排、emoji、全角标点、破折号省略号、对齐三种都有）、`wrap.md`（中英混排长段落）。

完成标准：
- 测试：渲染 `cjk-table.md` 后，同一张表格的每一行显示宽度相等。
- 快照：`--width 40`、`80`、`120` 三种宽度各一份。
- 在 Kaku/WezTerm 里目测中文表格竖线对齐。

## T4 代码高亮

做：
- syntect，从内置 dump 加载语法集和主题（`SyntaxSet::load_defaults_newlines`），第一次遇到代码块才加载。
- 按行高亮（`HighlightLines`），一行代码输出一个 `Line`，`src` 指向源码里这一行。T6 的"代码块按行定稿"靠这个。
- 颜色转成 `Color::Rgb`。语言未知或没写时不高亮。
- 高亮主题跟随 tmd 主题的亮暗（暗用 base16-ocean.dark，亮用 InspiredGitHub，先写死）。

完成标准：
- `fixtures/code.md` 里加 rust、typescript、python、bash、没写语言的各一段；`cargo run -- fixtures/code.md` 有颜色。
- 快照更新。

## T5 主题

做：
- `Theme` 结构，字段：每种元素一个 `Style`（h1、h2、h3、正文、粗体、斜体、行内代码、代码块背景、链接、引用竖线、列表符号、表格边框、表头、分隔线、折叠行），以及符号（列表点、任务框、表格线字符）。
- TOML 反序列化。颜色写法：`"ansi:4"`、`"bright:4"`、`"#4DF2FF"`。
- 内置主题编进二进制（`include_str!`）：`default-dark`、`default-light`（只用 ANSI 16 色）、`neon-tokyo`（真彩色）。
- 用户主题目录：`~/.config/tmd/themes/<name>.toml`，同名覆盖内置。
- `--theme auto`（默认）：stdout 是终端时用 terminal-colorsaurus 查亮暗，超时 300 ms；查不到看 `COLORFGBG`；再不行用暗色。stdout 不是终端时不查询，直接用暗色。
- `themes/README.md` 写清每个字段。

完成标准：
- `cargo run -- --theme neon-tokyo fixtures/basic.md` 换了样子。
- 主题 TOML 写错时报错信息带文件名和行号。
- 单元测试：`auto` 回退链（把查询结果做成可注入的参数来测）。

## T6 流式渲染（核心）

做：
- 何时进流式：没给 FILE、stdin 不是终端、stdout 是终端。stdout 不是终端时读完再一次性输出颜色码文本。
- tmd-core 加 `Streamer`，纯逻辑，不碰终端：
  - `push(&mut self, chunk: &str) -> Frame`，`finish(&mut self) -> Frame`
  - `Frame { committed: Vec<Line>, active: Vec<Line> }`：`committed` 是这次新定稿的行（以后不再变），`active` 是当前活动区的全部行。
  - 每次 push：追加到缓冲，对整个缓冲调 `render`，得到带 `src` 的行。
  - 计算定稿边界（字节偏移）：用 pulldown-cmark 的 `into_offset_iter()` 找顶层块（Start 深度 +1、End −1，深度 0 的 Start 和 `Rule` 算顶层块）。边界 = 最后一个顶层块的起点，再往前退到最近一个换行之后。最后一个顶层块是代码块时例外：边界 = 代码块里最后一个完整行（以换行结尾）的末尾。
  - 定稿行 = `src.end <= 边界` 且 `src.start >= 上次已定稿偏移` 的行。按偏移判断，不按行号，这样窗口变宽变窄也不会错位。
  - 其余行是活动区。
  - `finish()` 把剩下的全部定稿。
  - 用 `Parser::new_with_broken_link_callback`，让未定义的 `[foo]` 也按链接样式渲染。
- tmd-cli 的终端输出（crossterm，不用 ratatui，不进 raw 模式）：
  - 记住上一帧活动区的行数 n。每一帧：光标上移 n 行 → 清到屏幕底 → 打印新定稿行（自然进入回滚区）→ 打印活动区。
  - 活动区最多显示"终端高度 − 1"行，超出只显示最后那些行。
  - 每帧用同步输出（`BeginSynchronizedUpdate` / `EndSynchronizedUpdate`）包起来；最多每 16 ms 画一帧，期间收到的内容攒着。
  - 开始时隐藏光标；正常结束、Ctrl-C（ctrlc crate）、上游断开时都恢复光标。
  - `TERM_PROGRAM` 是 WezTerm 时，开始发 `ESC ] 1337 ; UnicodeVersion=push BEL` 再发 `=14`，退出时发 `=pop`。
  - 窗口缩放：下一帧按新宽度渲染活动区，已定稿的不动。
- `scripts/slowcat.py`：按指定速度（默认每秒 300 字节）把文件写到 stdout，用来模拟 AI 逐字输出。
- 新 fixture：`long.md`（300 行以上，含长代码块、长表格、嵌套列表）。

完成标准：
- 流式一致性测试（proptest）：对每个 fixture，随机切成 1–50 字节的片段逐个 push，最后 finish；所有 `committed` 拼起来等于对全文一次性 `render` 的结果。fixture 里不要放"引用定义在引用之后"的写法。
- 单元测试：定稿后的行在之后的 push 里不会再出现在 `committed` 里（不重复输出）。
- 手动：`python3 scripts/slowcat.py fixtures/long.md | cargo run --release` 在 Kaku 里看：不闪、表格不错位、长代码块边出边显示、结束后光标正常。Ghostty 再跑一次。

## T7 Claude Code 事件流

做：
- 作者先录两份真实样本，放 `fixtures/claude/`：
  - `plain.jsonl`：`claude -p "用中文介绍一下 Rust 的所有权，带一个表格" --output-format stream-json --verbose --include-partial-messages > fixtures/claude/plain.jsonl`
  - `tools.jsonl`：让它读文件或跑命令的问题，确保里面有 tool_use。
- `--from claude-json`：逐行用 serde_json 解析成 `Value`，按下表处理，未知事件忽略，解析失败的行忽略（加 `--debug` 时打印到 stderr）。

| 事件 | 处理 |
|---|---|
| `stream_event` → `content_block_start`，type=text | 新建一个 Streamer |
| `content_block_delta` → `text_delta` | `event.delta.text` 推给当前 Streamer |
| `content_block_stop`（文本块） | 当前 Streamer `finish()`，全部定稿 |
| `content_block_start`，type=thinking；`thinking_delta` | 累计字数，块结束时输出一行折叠行"▸ 思考 · N 字"（灰色，定稿） |
| `content_block_start`，type=tool_use；`input_json_delta` | 块结束时输出一行"▸ 工具 · 名字 · 参数摘要（前 60 字）"，记下 tool_use id 和时间 |
| `user` 消息里的 `tool_result` | 按 id 找到工具，另起一行"✓ 名字 · 用时 8.4 s"；出错时"✗ 名字 · 出错" |
| `assistant` 消息 | 忽略（文本是整段重发） |
| `result` | 输出一行"完成 · 总用时 · 费用"（字段存在才显示） |

完成标准：
- 回放测试：读 `fixtures/claude/*.jsonl` 逐行喂入，输出（去掉颜色码）和快照一致。
- 手动：真实跑一次 `claude -p "…" --output-format stream-json --verbose --include-partial-messages | cargo run --release -- --from claude-json`，回答逐字出现，思考和工具调用各一行。

---

以下是 MVP 之后，先别做。

## T8 全屏查看器
- 给了 FILE 且 stdout 是终端时：ratatui 备用屏全屏，`j/k` 上下、`空格/b` 翻页、`gg/G` 首尾、`t` 轮换主题、`q` 退出。
- 流式结束后按任意键进入同一个查看器，`q` 回到原来的屏幕，回滚区保持原样。

## T9 截图导出
- `--screenshot out.svg|out.png`：从 `Vec<Line>` 直接生成 SVG（每个 Span 一个 `<tspan>`，等宽字体按格子定位）；PNG 用 resvg，通过 `usvg::Options::fontdb_mut()` 加载 `assets/` 里的中文字体，family 名要和 SVG 里写的一致。

## T10 README 与发布素材
- README 首图用 `tmd --screenshot` 渲染 README 自己。
- 录 60 秒流式对比视频（左 glow，右 tmd）。
- 截图里不放没测过的数字。
