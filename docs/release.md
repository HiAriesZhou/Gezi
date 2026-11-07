# 自动发布

## 日常流程

当前统一使用 `0.1.x`。普通功能、体验改进和修复只递增第三段，例如 `0.1.1` → `0.1.2`；只有维护者明确确认的大改动才升级第二段。功能或里程碑完成不等于需要升级第二段。

以完整、可验证的任务作为版本边界：独立交付的功能或修复分别占一个小版本，同一任务发布前的审修、测试及文档更新归入该版本。按任务完成顺序递增，不按提交数量分配版本，也不将多个历史功能压成一个版本。

补发历史版本时，从对应任务结束时的源码准备候选，核对源码、锁文件、实际程序版本和发布说明；不得只给当前完整功能代码换上一个较早版本号。尚未完成验证和发布的历史候选，不标记为已发布。

1. 在任务分支更新 `Cargo.toml` 的 `[workspace.package].version`，同步 Cargo.lock 中的项目包版本。
2. 在 CHANGELOG.md 中加入对应的 `## 版本号` 段落及正文。
3. 提交 PR，审阅并取得维护者的发布确认后合并到 `main`。
4. 合并后的 `main` CI 在 macOS 上通过后，`Publish version` 上传并校验 Apple Silicon 包和 SHA256，将 `v<版本>` tag 固定到通过检查的提交，正式发布 GitHub Release。

Release 标题为 `tmd v<版本>`，正文只取对应版本的 CHANGELOG 段落。预发布版本如 `0.1.0-rc.1` 会标为 prerelease。普通文档或代码提交没有更新版本时，已发布版本会跳过。

Release 包含 Apple Silicon macOS `.tar.gz`、对应 `.sha256`，以及平台自动提供的源码归档；暂不发布 Intel Mac 二进制、crates.io 或 Homebrew 包。

## 构建 Apple Silicon 候选包

在 Apple Silicon macOS 的干净、已提交检出中运行：

```sh
cargo build --release --locked
python3 -B scripts/package_release.py --binary target/release/tmd --out-dir target/release-package
python3 -B scripts/check_package.py --package-dir target/release-package
```

工具检查可执行文件的架构和版本，生成 `tmd-v<版本>-aarch64-apple-darwin.tar.gz` 及对应 `.sha256`。包内仅包含可执行文件、许可证、[二进制使用说明](binary-package.md)和版本/提交/平台元数据；无需 Rust 或 Lua。相同输入使用固定归档元数据，输出已存在时停止，须选择新的空输出目录。

检查工具解压归档中的实际二进制，在临时配置及仅含系统命令的 PATH 下运行版本、帮助、中文渲染和阅读/流式 PTY 检查。测试依赖开发环境中的 Python，可执行文件本身无需 Python、Rust 或 Lua。

macOS CI 在当前提交上执行相同构建与检查，将包保存为本次 Actions 运行的产物。发布任务仅从同次运行下载，校验包内提交与 `github.sha` 一致。重跑检查任务会替换该次运行的 Actions 构建产物；包输出位于 runner 临时目录，不复用 Rust 缓存中的旧归档。

## 发布边界

- 历史 Release 或 tag 的撤回须单独取得维护者确认；版本整理不移动或复用已有 tag，不改写代码提交历史。
- 检查和发布任务均使用 macOS runner。发布任务依赖 `check` 成功，只接受 `push` 到 `refs/heads/main`。PR、其他分支和 tag 事件不发布。
- 检查任务只有 `contents: read`；独立发布任务临时申请 `contents: write`，checkout 固定到该次 CI 的提交且不持久化凭据。
- 发布通过 GitHub API 将 tag 固定到通过 CI 的提交。创建前发现 `main` 已前进时跳过，等待新提交自己的 CI。
- 发布任务串行运行。同版本已有非草稿 Release 时跳过，不修改现有 Release 或 tag。
- 已有 tag 没有 Release 时，仅允许 tag 正好指向该次 CI 提交时补发。指向其他提交或存在同名草稿时失败，需要维护者检查。
- 发布只处理本次运行创建的草稿。归档和校验文件上传后，分别下载核对实际字节，再检查 main 提交和草稿归属，最后将草稿正式发布；运行前已有的草稿始终停止。
- 写请求即使超时也可能已成功。脚本先回读远端，凭本次运行的归属标记确认草稿；无法确认时停止，不重复发送创建、上传或发布请求。正式 Release 正文只保留 CHANGELOG 内容。

## 验证与恢复

```sh
python3 -B -m unittest discover -s scripts -p 'test_*release.py'
```

测试使用模拟 GitHub API，不会创建远端 tag 或 Release；macOS CI 会执行。

失败时先查看 Actions 的 `Publish version` 日志及远端 Release、tag 和资产。已完成发布时重跑会跳过；若留下草稿，须由维护者检查并处理，后续运行不会自动接管。确认没有冲突后可重跑失败任务。不要删除或移动已经发布的 tag 来重复使用版本号。

实现依据：[依赖任务成功后执行](https://docs.github.com/en/actions/how-tos/write-workflows/choose-what-workflows-do/use-jobs)、[Release API 与 tag 提交选择](https://docs.github.com/en/rest/releases/releases#create-a-release)。
