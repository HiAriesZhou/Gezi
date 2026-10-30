# TMD 交接

更新日期：2026-10-30。接续依据为仓库文档、源码、测试、Git 状态及本次交接确认。

## 当前结论

T7 已实现 Claude JSONL 事件适配、终端流式和非终端批量输出，并补齐样本回放及异常输入测试。本地格式、Clippy、140 个 Rust 测试、两种格式的 PTY 场景和三个 Python 回归通过；本阶段 PR 审查与 CI 结果以最终交付回执为准。此前纯文本修复和 T6 协议检查分别通过 PR #9、#10 合并。Kaku 已完成 T6 交互检查。用户已确认将 Ghostty/kitty 补测移至 MVP 最终验收，不再阻塞 T7。

T7 交付包含两份已脱敏的真实样本、`claude.rs` 适配器、`--debug` 和两份确定性快照。原 `t7-claude` 工作区的五个未提交文件保留；阶段交付使用从 main 创建的独立分支，不覆盖原工作。

本次交接确认：

- 没有其他会话在修改本工作区，可以在保留现有改动的前提下接续。
- 接手时 T6 的真实终端验收尚未完成；后续已完成 Kaku 检查，其余终端仍须补做。
- T7 采用回答作废提示：保留已经进入终端历史的内容，结束当前活动文本，另起一行显示「▸ 回答中断，重新生成」，再显示后续回答。
- 交接审计阶段未构建、运行测试或联网核对远端状态。
- 后续执行已授权：先恢复纯文本路径、补 T6 验收，再推进 T7；按逻辑变化分步提交，各阶段开 PR，经 Waza:check 审查，问题按 Waza:hunt 分析修复，CI 通过后合并。T8 至 T10、打 tag、包发布和部署不在范围内。

## 项目目标和架构

只读终端 Markdown 渲染器，二进制名 `tmd`，包名 `tmd-cli`。MVP 覆盖读取文件、纯文本管道、Claude Code JSONL 管道，重点是流式输出、中英文对齐、主题。

当前是 Rust 2024 workspace，只有两个 crate：

- `crates/tmd-core`：pulldown-cmark 解析、HTML 去标签、显示宽度、换行、表格、syntect 高亮、TOML 主题及 `Streamer` 定稿，输出 `Line` / `Frame`，无终端依赖。
- `crates/tmd-cli`：clap 参数、主题选择、ANSI/OSC 8 输出、stdin 和文件读取、终端活动区重绘及退出恢复。

`CLAUDE.md` 的「只输出 `Vec<Line>`」是旧入口的简写：当前 `render` 返回 `Vec<Line>`，`Streamer` 返回由已定稿行和活动行组成的 `Frame`；两者都不依赖终端。保留这些已有接口。

`docs/TASKS.md` 决定当前实现范围；`docs/architecture.md` 包含四 crate、ratatui、raw 模式、查看器和截图等旧方案或未来设计，不应直接照搬。T8 至 T10 暂不开展。

## 接手时的 Git 基线

- 当前分支：`t7-claude`，HEAD 为 `636ea31`，提交内容是两份 Claude Code 真实事件样本。
- 本地 `main` 和 `origin/main` 均指向 `18c9f06`；当前分支相对本地 `main` 多一个提交。
- `t7-claude` 未配置 upstream。这里的远端引用只是本地缓存，尚未重新核对。
- 审计时有以下四个未提交文件；没有暂存改动。这些是应接续的既有工作，不得当成待清理内容。

| 文件 | 已有修改 | 接续注意 |
|---|---|---|
| `Cargo.lock` | 增加 serde_json 及其依赖记录 | 与 manifest 一起保留 |
| `crates/tmd-cli/Cargo.toml` | 增加 `serde_json = "1"` | 事件解析尚未使用 |
| `crates/tmd-cli/src/stream.rs` | 新增 `Source` trait，`run` 改收创建输入源的闭包 | `main.rs` 的调用仍未适配 |
| `crates/tmd-core/src/lib.rs` | 导出 `text_width`、`truncate` | 可复用到折叠行布局 |

当前本地 `t7-claude` 还包含交接文档提交 `cea3203`，并有五个未提交文件：上表四个文件，加上已经适配输入源工厂的 `crates/tmd-cli/src/main.rs`。其中 `main.rs`、`stream.rs` 的纯文本修复已经通过 PR #9 进入 main；JSON 依赖和宽度导出仍是 T7 准备工作。接续时区分已交付的相同内容与尚未提交的 T7 内容，保留它们，不重复应用纯文本修复或按旧的四文件计数误判漂移。

## 已有实现和近期完成事项

以下表示源码和测试存在，不表示本次重新验收通过。

| 阶段 | 当前证据 |
|---|---|
| T1 至 T3b | 文件/stdin 入口；标题、段落、列表、引用、表格、中文宽度、HTML 去标签及行为测试 |
| T4 至 T5 | 高亮、三套主题、用户主题覆盖、auto 回退、主题校验及测试 |
| T6 | 字节偏移定稿、代码块按行定稿、活动区限高重绘、16 ms 刷新间隔、UTF-8 分段解码、退出恢复 |
| 渲染回归 | 8 个 Markdown fixture；40/80/120 宽度纯文本快照加 80 宽度 ANSI 快照，共 32 份 |
| 流式回归 | proptest 比较分段输出和全文渲染；另有不重复定稿、代码行、空行、边界回归及保留的失败种子 |
| T7 样本 | `fixtures/claude/plain.jsonl`（426 行）、`tools.jsonl`（50 行）；均可按行解析为 JSON，随 T7 交付 |

近期提交修复了流式空行和定稿边界、上游空闲时忙等、Ctrl-C 恢复终端、一行高终端的活动区问题，并将高亮缓存上限改为按行数计算。

CI 定义在 `.github/workflows/ci.yml`，Linux/macOS 均执行 fmt、Clippy、Rust 测试、CLI 构建、PTY 检查和三个 Python 回归。PR #9 和 #10 的最终 CI 均成功；历史会话回执不替代当前版本的验证。

## 当前未完成任务和阻塞

### 先接完已有重构

接手时 `stream::run` 的第二个参数已变为 `FnOnce(u16) -> Box<dyn Source>`，但 `main.rs` 仍传入 `Theme`。编译器已复现 E0277；现已用创建 `Streamer` 的闭包适配调用。该修复通过 PR #9 单独合并到 main（`a9fa035`），没有把尚未使用的 JSON 依赖和宽度导出捆入纯文本修复。

### T7 交付与待验收项

- 已实现文本、思考、工具调用、工具结果、result 和回答作废提示；忽略 assistant 全量重发、未知事件和子 agent 事件。
- JSONL 可按任意 UTF-8 输入块切分，支持 EOF 无换行、非法行恢复、按 id 配对工具结果和固定时钟回放。
- `--debug` 只在 stderr 报告坏行行号及错误类别，不输出原始记录。折叠行移除控制字符并按显示宽度截断。
- 两份新快照保留样本中的中断回答及重新生成内容；工具耗时固定为 0.0 s，不能视为真实调用耗时。独立断言覆盖去重、作废、工具关联及失败。
- Claude 2.1.314 已登录，但真实请求被上游账户验证拦截，未产生增量文本；`is_error: true` 显示“失败”，不会显示“完成”。验证账户后仍须补做真实 Claude 管道验收。
- 用户指定改用 Codex。Codex 0.166.0 的真实文本管道已在 Kaku 显示中文表格和 Rust 代码块，管道退出 0。其 JSONL 是另一套协议，本阶段没有新增 `--from codex-json`；完整消息输出不能证明逐字增量显示。

依赖补充下载已获系统权限确认并完成。用户暂停其他 Kaku 操作后，Kaku 0.27.0 已完成正常长文档流式、活动期间缩放、代码块逐行显示、Ctrl-C、后续输入和 80 列中英文/emoji 表格检查。

用户已允许临时安装 Ghostty 1.4.0 和 kitty 0.49.2，两份官方应用均通过代码签名校验。但电脑控制工具在用户确认后仍拒绝访问这两个应用，原因是工具安全准入限制。用户已确认将这两项移至 MVP 最终验收。届时需实际开放应用访问或由作者手动验收；不得绕过拒绝，也不得将伪终端结果冒充视觉验收。

Claude 账户验证属于外部限制；不在仓库记录验证链接、组织标识或原始错误会话。

## 关键设计决定

- 每次对缓冲全文重解析，不自写 Markdown 分块器。
- 定稿按源码字节边界推进；仅完整行参与边界判定。块间空行归属后一个块，代码块不必等待围栏结束才定稿代码行。
- 已定稿内容进入 scrollback 后不再修改；窗口缩放只重排活动区。
- MVP 直接使用 crossterm，并用同步输出包裹重绘；不进 raw 模式，让 Ctrl-C 传给整个管道。
- 中文宽度和截断复用已有字符簇处理；不使用 `width_cjk()`。
- 高亮复用每行状态；超长行会关闭块高亮，缓存限制为总行数，避免每帧重复高亮或缓存反复淘汰。
- 后置引用定义不能追溯改变已定稿内容，这是已接受的流式限制；普通流式一致性用例不以此为保证。
- 工具结果按 id 关联，另起一行显示完成或出错，不回改历史工具行。
- 回答作废标记 `abandoned_blocks` 位于 stream_event 外层，与 `event` 并列，不能从 `event` 内查找。

## 事件样本与待定事项

样本提交说明记录录制版本为 Claude Code 2.1.312，并说明做过路径、用量事件和初始化清单脱敏。今后新增样本仍需单独检查，不可直接提交原始录制。

实际已观察到：

- `plain.jsonl` 有两段 message，第一段的 `message_stop` 带顶层 `abandoned_blocks`，后面重新回答。
- `tools.jsonl` 有工具调用、分段 JSON 参数和对应工具结果。
- 两份样本均有 thinking 增量，但其正文为空；不能据此计算实际思考字数。
- result 包含 `duration_ms`、`total_cost_usd` 等字段；展示时以字段存在与真实类型为准。

当前采用的展示方案：

- 空思考正文显示「▸ 思考 · 内容不可见」，避免将未知字数或 token 估计写成事实。用户已授权按 handoff 继续实施，不再为这个可逆的展示选择中断执行。

实施者在编码时需落实的技术方案，不要求用户预先选择内部接口：

- 已注入单调时钟；生产使用启动后的 elapsed，测试控制固定时刻。工具耗时为本地观察到的开始至结果到达时间。
- 非 null 的 `parent_tool_use_id` 事件不参与父级块索引处理；顶层 user 的 tool_result 仍按工具 id 关联。现有样本没有子 agent 事件，过滤行为只由构造用例验证，仍须真实样本补测。

会话记录只用于恢复上述决定和验证线索，不是提交、合并或发布的授权，也不作为原文复制进仓库。

## 接续顺序和验收

1. 重新读取当前 Git 状态，对照上面的“接手时基线”和“当前本地状态”，保留五个未提交文件及本地文档提交。纯文本修复与 T6 协议检查已交付，不重复实施；发现新的未知修改时再核实来源。
2. Ghostty/kitty 留待 MVP 最终验收；Kaku 已完成的 T6 检查不必无故重跑。此前阻塞 T7 的验收顺序已由用户调整。
3. T7 已复用 `Source` 转换为 `Frame`，未改造终端绘制方式。非终端或 `--ansi` 同样解释事件。先完成本阶段审查及 CI，按授权合并。
4. 后续新增事件类型先录制、脱敏并审查样本，再补独立语义断言及回放快照。不得以批量接受快照代替正确性判断。
5. 执行自动检查，确认纯文本渲染、已有快照和流式一致性未退化。
6. Claude 账户验证解除后补做真实管道；最终补齐 Ghostty/kitty，分别报告自动检查、终端效果和作者验收状态。

### 自动验证命令

以下是开发阶段的验证入口。纯文本修复已在原工作区和隔离分支通过这些检查，PR #9 的 Linux/macOS CI 也均通过。若缺新依赖，先报告缺项，不自动安装。

`scripts/check_stream_pty.py` 已覆盖正常结束、80→60 列缩放、Ctrl-C 返回 130、光标和 Unicode 状态恢复、终端属性不变及 shell 继续运行。`scripts/test_stream_pty.py` 覆盖父子启动顺序和 C locale；两项回归均跑过修复前失败、修复后通过。检查说明见 `docs/terminal-testing.md`。这是协议与生命周期证据，不代表真实终端视觉验收完成。

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
```

T7 回放随 `cargo test` 执行，覆盖 `fixtures/claude/*.jsonl`；CLI 集成测试覆盖重定向输出、`--ansi` 及 `--debug` 的 stderr 隔离。

### 人工验收命令

```sh
python3 scripts/slowcat.py fixtures/long.md | cargo run --release
```

Kaku 主测，Ghostty 补测：长输出不闪、表格对齐、长代码块逐行输出、窗口缩放仅改变活动区、Ctrl-C 和正常结束均恢复光标。更广的 MVP 终端检查按 `docs/research.md` 执行，Windows/tmux/Zellij 不在 MVP 测试范围。

先用已录样本检查 T7，不产生新的模型调用：

```sh
cargo run --release -- --from claude-json < fixtures/claude/plain.jsonl
cargo run --release -- --from claude-json < fixtures/claude/tools.jsonl
```

在 Claude 账户可用后，再验证逐字文本、思考和工具折叠行、工具结果、结束行及终端恢复：

```sh
claude -p "用中文介绍一下 Rust 的所有权，带一个表格" --output-format stream-json --verbose --include-partial-messages | cargo run --release -- --from claude-json
```

用户指定的 Codex 验收使用现有纯文本入口，无需 JSON 格式参数：

```sh
codex exec --sandbox read-only --ephemeral "用中文介绍 Rust 所有权，带表格和代码块" < /dev/null | cargo run --release
```

Codex 输出完成的 Markdown 消息后由 tmd 渲染，不能代替 Claude JSONL 增量协议的真实验收。剩余真实终端及 Claude 管道验收完成前不宣称 MVP 完成；不提前进入 T8 至 T10。
