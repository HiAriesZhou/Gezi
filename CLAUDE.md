# TMD

终端里的 markdown 渲染器，命令 `tmd`。只读，主打流式不闪、中英文对齐、主题。

- 调研结论和范围：docs/research.md
- 任务拆分（按顺序做）：docs/TASKS.md
- 完整架构（v3，含 MVP 之后的部分）：docs/architecture.md。和 TASKS.md 冲突时以 TASKS.md 为准。

## 常用命令

```
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
cargo insta review          # 审查快照变化
python3 scripts/slowcat.py fixtures/long.md | cargo run --release   # 手动看流式效果
```

## 工作方式

- 一次只做 TASKS.md 里的一个任务。做完跑该任务的"完成标准"，全部通过后提交，然后停下来，告诉我做了什么、哪些需要我手动检查。
- 不加任务里没写的功能。觉得需要时先问。
- `tmd-core` 不能依赖任何终端库（crossterm、ratatui 等），只输出 `Vec<Line>`。
- 快照变化要说明原因，不要直接批量接受。
- 遇到和文档描述不符的库行为（版本、接口、事件格式），以实际为准，并在回复里指出来。

## 和我沟通

中文，表意明确、简洁、直接。不用梗、黑话、比喻，不先预告再展开。
