# tmd

终端里的 markdown 渲染器，命令 `tmd`。只读，主打流式不闪、中英文对齐、主题。

开发中，还不能用。

## 用法

```
tmd README.md          # 渲染文件
cat notes.md | tmd     # 从 stdin 读
tmd --help             # 全部参数
```

## 开发

```
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
```

## 许可证

MIT
