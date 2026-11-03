# TMD 任务拆分

按顺序做，一个任务一次提交。每个任务做完跑"完成标准"，全部通过再停下来等确认。

T1 至 T7（含 T3b）是 MVP：做完就能日常用（看文件、接管道、接 Claude Code）。T8 以后是 MVP 之后。

| 任务 | 内容 | 依赖 | 属于 |
|---|---|---|---|
| T1 | 仓库骨架、命令行参数、CI | 无 | MVP |
| T2 | 核心渲染（除表格、高亮） | T1 | MVP |
| T3 | 宽度计算与表格 | T2 | MVP |
| T3b | HTML 去标签显示 | T2 | MVP |
| T4 | 代码高亮 | T2 | MVP |
| T5 | 主题 | T2 | MVP |
| T6 | 流式渲染（纯文本管道） | T3 T4 T5 | MVP |
| T7 | Claude Code 事件流 | T6 | MVP |
| T8 | 静态文件原地滚动阅读 | T7 | 之后 |
| T9 | 全屏查看器 | T6 | 之后 |
| T10 | 截图导出 | T5 | 之后 |
| T11 | README 与发布素材 | T7 | 之后 |

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
  - 标题 1 至 6 级，不显示 `#`。一级粗体加色；二级粗体加色，下面一行细线；三级以下只变色。
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
- 软换行两边的字符都是全角（显示宽度 2，含中文标点）时，不插空格，直接连起来；其他情况照常插一个空格。（T2 审查后加入）
- 表格：
  - 列宽 = 该列内容的最大显示宽度。
  - 支持 `:--`、`:-:`、`--:` 三种对齐。
  - 边框用 `┌ ┬ ┐ ├ ┼ ┤ └ ┴ ┘ │ ─`，表头粗体。
  - 总宽超过终端宽度时，反复把当前最宽的列压窄一格，单元格内换行；压到每列只剩 4 格还放不下，就截断并在行尾加 `…`。
- 新 fixture：`cjk-table.md`（中英文混排、emoji、全角标点、破折号省略号、对齐三种都有）、`wrap.md`（中英混排长段落，含中文之间、中英之间的软换行）。

完成标准：
- 测试：渲染 `cjk-table.md` 后，同一张表格的每一行显示宽度相等。
- 快照：`--width 40`、`80`、`120` 三种宽度各一份。
- 在 Kaku/WezTerm 里目测中文表格竖线对齐。
- 测试：`中文\n中文` 渲染成一行且中间没有空格；`中文\nEnglish` 中间有一个空格。

## T3b HTML 去标签显示

T2 审查后加入，取代 T2 里"HTML 原样灰色显示"。原因：HTML 块里的正文也变灰、和标签混在一起；README 开头常见的居中 logo、徽章原样显示是一大片标签。

做：
- 行内 HTML 和 HTML 块都去掉标签，标签里的文字按所在位置的正文样式显示。
- `<img alt="…">` 显示为 `[图片: …]`，没有 alt 时 `[图片]`，样式同 markdown 图片（`muted`）。
- `<br>` 换行。HTML 块里，块级标签（`p`、`div`、`details`、`summary`、`h1` 至 `h6`、`li`、`tr`）的开始和结束处换行，连续空行合并，首尾空行去掉。
- `<!-- … -->` 注释不显示；其他认不出的标签直接隐藏。
- 常见实体解码：`&amp;` `&lt;` `&gt;` `&quot;` `&#39;` `&nbsp;`。
- `<details>` 直接展开显示，不做折叠。
- 不引入 HTML 解析库，用简单的标签扫描；未闭合的 `<` 按普通文字显示。
- 去标签后 HTML 块什么都不剩时，这个块不输出（也不留空行）。
- 新 fixture：`html.md`（README 式开头：居中 logo 和徽章、`<details>`、`<kbd>`、注释、`<br>`、未闭合标签）。

完成标准：
- 单元测试：`<img>` 的 alt、`<br>`、注释、实体、未闭合标签、只有注释的 HTML 块。
- 快照更新：`basic.md` 和 `html.md` 的输出里不出现 `<`、`>` 标签。
- `cargo run -- fixtures/html.md` 目测像正文。

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
- `themes/README.md` 写清每个字段（含 T2 加的 `muted`：图片占位用）。
- 粗体、斜体除了字重和斜体属性，在内置主题里也配颜色：中文字体常没有粗体、斜体，只靠属性看不出来。（T2 审查后加入）
- 标题层级：不改字号（不做字号协议），靠颜色、粗细、符号和线让一级、二级、三级及以下能一眼区分，例如一级下面用粗线 `━`。具体样式截图给作者定。（T2 审查后加入）

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
- 流式一致性测试（proptest）：对每个 fixture，随机切成 1 至 50 字节的片段逐个 push，最后 finish；所有 `committed` 拼起来等于对全文一次性 `render` 的结果。fixture 里不要放"引用定义在引用之后"的写法。
- 单元测试：定稿后的行在之后的 push 里不会再出现在 `committed` 里（不重复输出）。
- 手动：`python3 scripts/slowcat.py fixtures/long.md | cargo run --release` 在 Kaku 里看：不闪、表格不错位、长代码块边出边显示、结束后光标正常。Ghostty 再跑一次。

实现中对上面规则的修改（流式一致性测试发现，以实际为准）：
- 块之间的空行改为归属**后一个块**（和后一个块的第一行同一个 `src`），取代 T2 的"归属前一个块"。原因：空行是否存在取决于后一个块有没有内容（只有标签的 HTML 块暂时不渲染），归属前一个块时会在流式中丢掉这一行。
- 定稿边界只按**完整的行**计算：最后一行没收完时可能被暂时解析成新块（表格后的 `| 中`），收完后又并回上一个块。
- 最后一个块是代码块、还没有完整代码行时，边界取代码块起点（标签行和空块占位行不提前定稿）。
- Unicode 版本序列对 `TERM_PROGRAM=Kaku`（WezTerm 的分支）也发送。
- 代码高亮按块缓存逐行状态，流式每帧不再重算已高亮的行（long.md 重渲染从 12.7 ms 降到 1.0 ms）。

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

2026-10-30 接续补充：空思考正文显示“▸ 思考 · 内容不可见”；回答作废时先定稿当前文本，再显示“▸ 回答中断，重新生成”。result 的 `is_error: true` 显示“失败”。Ghostty/kitty 补测移至 MVP 最终验收，不阻塞 T7。Claude 账户验证未解除前，真实环境检查按用户指定使用 Codex 文本管道；Claude JSONL 特有行为用已录真实样本回放验证，真实 Claude 管道仍标为待补测。

---

以下是 MVP 之后，只推进用户明确授权的任务。

## 0.0.2 静态大标题

2026-11-02 用户确认的 MVP 后扩展，独立于 T8：

- `--large-headings` 默认关闭；仅静态 Markdown 文件、stdout 为终端、直接 Kaku 生效。
- 顶层 ATX/Setext H1 使用双宽双高；按半宽换行，分隔线仍为普通全宽行。H2 至 H6、嵌套标题、表格和代码不变。
- 非终端、`--ansi`、Claude JSONL、管道输入、未知终端、tmux/screen、窗口小于 4 列或 3 行、指定宽度超出窗口时，在布局前回退到普通字号。
- core 只增加逻辑字号和物理行数；`render`、`Streamer::new` 的默认输出不变，不改流式行数计算。
- 静态路径须处理正常结束、Ctrl-C 和部分写入失败；显式恢复 DEC 行属性，未完成的上下半行须清除。终端断开后的恢复只能尽力执行。
- 自动检查覆盖布局、回退、每个字节边界的写入失败和取消、真实 CLI 的 PTY 生命周期；现有快照不应改变。
- Kaku 实测须检查中英文/emoji、换行、底部滚屏、正常结束和中断后提示符、复制结果；用截图记录字号效果。DEC 上下半行写入相同文字，复制可能重复，不能将 PTY 通过当作复制或显示验收。
- 验收样本和步骤见 `fixtures/large-headings.md`、`docs/terminal-testing.md`。当前实现及验证进度见 `docs/HANDOFF.md`。

## T8 静态文件原地滚动阅读

2026-11-03 用户确认的独立任务，不依赖全屏查看器：

- 静态 Markdown 文件且 stdin/stdout 都是终端时，默认零配置原地滚动阅读；全文布局保存在内存，从开头绘制，不切备用屏，不清除终端历史。
- 根据当前光标行把此前占用的可见内容推入历史，顶部显示清除控制字符后的 tmd 调用信息；正文从下一行开始，底部明确提示 q/Esc 返回命令行。
- 滚轮、上下键、j/k 连续滚动，Home/End 到首尾，PageUp/PageDown 或空格/b 翻页。无需按键放出后续内容。短文档和空文档自动返回。
- 阅读器临时管理键盘及鼠标事件；这是静态阅读专用输入模式，流式路径仍不进入 raw 模式。正常结束、键盘 Ctrl-C、INT/TERM/HUP/QUIT 和输出错误须恢复终端。
- 窗口变化重新布局，并回到此前顶部段落的开头；大标题按物理行数计算，不能切开上下半行。窗口变小时仍须可以退出。
- 仅一个配置入口：`tmd/config.toml` 的 `[reader].mode`，默认 `inline`、可选 `native`；`--reader inline|native` 优先于配置。配置根目录与主题一致，优先 XDG_CONFIG_HOME，其次 ~/.config。
- 可选 Kaku 适配器见 `contrib/kaku-reader.lua`：版本化私有临时文件回执确认安装及历史容量后，才输出全文并请求原生回到开头。不支持、超时、版本不匹配或容量不足时，在全文输出前回退 inline。不自动改用户终端配置。
- 管道输入、重定向、--ansi、Claude JSONL、不支持的终端和初始过小窗口保持原输出路径；不加入搜索、目录、主题切换或流式结束后进入阅读器。
- 自动验收：配置解析、行布局边界、真实 CLI 的首屏/滚轮/首尾/缩放/退出/信号恢复及原生握手/回退。PTY 结果不代替真实终端显示、复制与原生适配器验收。
- 使用和配置步骤见 `docs/reader.md`，终端验收见 `docs/terminal-testing.md`。

## T9 全屏查看器
- 给了 FILE 且 stdout 是终端时：ratatui 备用屏全屏，`j/k` 上下、`空格/b` 翻页、`gg/G` 首尾、`t` 轮换主题、`q` 退出。
- 流式结束后按任意键进入同一个查看器，`q` 回到原来的屏幕，回滚区保持原样。

## T10 截图导出
- `--screenshot out.svg|out.png`：从 `Vec<Line>` 直接生成 SVG（每个 Span 一个 `<tspan>`，等宽字体按格子定位）；PNG 用 resvg，通过 `usvg::Options::fontdb_mut()` 加载 `assets/` 里的中文字体，family 名要和 SVG 里写的一致。

## T11 README 与发布素材
- README 首图用 `tmd --screenshot` 渲染 README 自己。
- 录 60 秒流式对比视频（左 glow，右 tmd）。
- 截图里不放没测过的数字。
