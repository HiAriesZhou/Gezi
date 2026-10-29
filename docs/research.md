# TMD 调研结论

## 背景
- 一句话描述：终端里的 markdown 渲染器（Terminal MarkDown，命令 `tmd`），只读，主打流式不闪、中英文对齐、主题好看。
- 结论：先做 MVP 自用，用起来再决定要不要继续。理由：
  1. AI 命令行工具每天输出 markdown，终端里看源码或渲染闪烁、表格错位是真实问题（Claude Code、Codex、Gemini CLI 都有相关 issue）。
  2. 同类工具（glow、Streamdown、mdcat）各缺一块：glow 不支持流式，Streamdown 不处理中文宽度。"流式 + 中英文对齐 + 主题"这个组合没人做。
  3. 目的是作品集和社区声誉，不求商业化。一个能录屏、能截图的工具就够。
- 调研日期：2026-10-23 至 2026-10-27（评估、方案、两轮审查）

## 目标用户与核心场景
- 用户：在终端里用 Claude Code / Codex 等 AI 工具的开发者，尤其中文用户；第一个用户是作者本人（Kaku/WezTerm）。
- 场景一：看文件。`tmd README.md`，排好版输出到终端。
- 场景二：接纯文本管道。`cat notes.md | tmd`，边收边渲染。
- 场景三：接 Claude Code。`claude -p "…" --output-format stream-json --verbose --include-partial-messages | tmd --from claude-json`，逐字渲染回答，思考和工具调用折叠成一行。

## 第一版范围（MVP）
### 做
- 渲染：标题（不显示 `#`）、段落、粗体斜体删除线、行内代码、链接、列表（含嵌套、任务列表）、引用、分隔线、代码块（带高亮）、表格（中英文对齐）。
- 宽度：中文 2 格、emoji 2 格、组合字符 0 格、ambiguous 字符 1 格；按显示宽度换行。
- 主题：内置 3 套（default 亮/暗、neon-tokyo），TOML 自定义，`auto` 按终端亮暗选。
- 流式：纯文本管道和 Claude Code 事件流，已定稿的进终端回滚区不再动，只重绘最后一块。
- stdout 不是终端时输出带颜色码的纯文本（可接 `less -R`）。

### 不做（MVP 之后再说）
- 全屏查看器（j/k 滚动）、`--screenshot` 截图导出、目录、搜索、折叠。
- 编辑、多语言绑定、mermaid、公式、图片协议、字号协议、剪贴板输入、`--diff`、`--bench`。

## 已定的决定
| 决定 | 理由 | 考虑过但放弃的方案 |
|---|---|---|
| Rust | 启动快、单文件分发、终端生态成熟 | Go（glamour 已在那边）、TypeScript |
| pulldown-cmark，每次对整个缓冲重解析 | 没有增量接口；自己写分块器会切错嵌套列表、引用续行、setext 标题；文本只有 KB 级，整体重解析开销可忽略 | 自写分块器；comrak |
| 按"已定稿字节偏移"定稿，只定稿最后一个顶层块之前、以换行结尾的内容；代码块按行定稿 | 已定稿的块后续不会再变；代码块可能很长，等围栏闭合会撑爆活动区（Codex 同样按行提交） | 按块数定稿；等代码块闭合 |
| MVP 流式输出直接用 crossterm 写终端，不用 ratatui | 只需要"打印定稿行 + 重绘活动区"，手写更好调试；ratatui 的 inline viewport 定高、需开 scrolling-regions 特性 | ratatui inline viewport（若手写版闪烁再换） |
| 流式模式不进 raw 模式 | Ctrl-C 由 shell 发给整个管道，上游 claude 和 tmd 一起停；只需捕获信号恢复光标 | raw 模式读 /dev/tty（要自己关 stdin 停上游） |
| 宽度只调 unicode-width 的 `width()` | ambiguous 字符按 1 格，和 WezTerm、kitty、Ghostty 默认一致 | `width_cjk()`（破折号、省略号会算 2 格，表格歪） |
| 主题颜色默认用 ANSI 16 色索引 | 不查询也跟着终端配色变；两色查询派生不出整套颜色 | 查询前景背景色后派生 |
| Claude Code 工具结果另起一行显示完成和用时 | 工具行已经进了回滚区，改不了 | 回头改那一行 |
| 链接引用定义出现在引用之后时，已定稿的文字不变成链接 | 定稿后不能改；用 `broken_link_callback` 让它至少有链接样式 | 延迟定稿到流结束 |

## 技术约束
- 技术栈：Rust stable；pulldown-cmark 0.13、crossterm 0.29、unicode-width 0.2、syntect 5.3、terminal-colorsaurus 1.0、serde/serde_json/toml、clap 4、insta（快照测试）、proptest（流式测试）、ctrlc。
- 外部依赖：Claude Code 的 stream-json 事件格式（必须带 `--verbose`）；事件格式可能随版本变，以录下来的真实样本为准。
- 终端：WezTerm/Kaku 主测，Ghostty、kitty 各跑一次。tmux、Zellij、Windows 不在 MVP 测试范围。
- 成本：0。

## 未解决的问题
- crates.io 上 `tmd` 这个名字是否已被占用：未核实。二进制名用 `tmd`，crate 名被占就用 `tmd-cli`。
- 终端不支持真彩色时，代码高亮颜色怎么退化：MVP 先不处理。
- 流式过程中窗口缩放：只重排活动区，已定稿的不动；实际效果要实测。
- 手写重绘在长输出下是否闪：用同步输出（DEC 2026）包裹每帧并限制刷新频率，仍闪再换 ratatui。
- Claude Code stream-json 的完整事件列表：架构文档里的表来自官方文档，以作者录的真实样本为准，遇到未知事件忽略。

## 验收标准
- `cargo fmt --check`、`cargo clippy -- -D warnings`、`cargo test` 全部通过，GitHub Actions 绿。
- 快照测试：`fixtures/*.md` 在宽度 40/80/120 下的输出与快照一致。
- 表格测试：同一张表格每一行的显示宽度相等（含中英文混排）。
- 流式一致性测试（最重要）：每个 fixture 随机切成 1–50 字节的片段逐段喂入，最后拼出来的输出和一次性渲染完全相同。
- Claude Code 回放测试：`fixtures/claude/*.jsonl` 回放输出与快照一致。
- 手动：`python3 scripts/slowcat.py fixtures/long.md | tmd` 在 Kaku 里不闪、表格不错位；真实跑一次 `claude -p … | tmd --from claude-json`。

## 参考资料
- 竞品：glow https://github.com/charmbracelet/glow ；Streamdown https://github.com/day50-dev/Streamdown ；mdfried https://github.com/benjajaja/mdfried ；edamame https://github.com/mijowi/edamame
- 流式做法参考：Codex markdown_stream.rs https://github.com/openai/codex/blob/main/codex-rs/tui/src/markdown_stream.rs ；Crush streaming_markdown.go https://git.secluded.site/crush/blob/main/internal/ui/chat/streaming_markdown.go
- Claude Code 流式输出：https://code.claude.com/docs/en/headless ；https://code.claude.com/docs/en/agent-sdk/streaming-output
- 宽度：https://docs.rs/unicode-width ；https://wezterm.org/config/lua/config/unicode_version.html
- 完整架构：docs/architecture.md（v3）
