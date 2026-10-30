# 自动发布

## 日常流程

1. 在任务分支更新 `Cargo.toml` 的 `[workspace.package].version`，同步 Cargo.lock 中的项目包版本。
2. 在 CHANGELOG.md 中加入对应的 `## 版本号` 段落及正文，例如 `## 0.0.2`。
3. 提交 PR，审阅后合并到 `main`。
4. 合并后的 `main` CI 在 Linux 和 macOS 上全部通过后，`Publish version` 自动创建 `v0.0.2` tag 和 GitHub Release。

Release 标题为 `tmd v<版本>`，正文只取对应版本的 CHANGELOG 段落。预发布版本如 `0.1.0-rc.1` 会标为 prerelease。普通文档或代码提交没有更新版本时，已发布版本会跳过。

当前发布 GitHub Release 及平台自动提供的源码归档；不上传预编译二进制，也不发布 crates.io、Homebrew 等包。

## 发布边界

- 发布任务依赖整个 `check` 矩阵成功，只接受 `push` 到 `refs/heads/main`。PR、其他分支和 tag 事件不发布。
- 检查任务只有 `contents: read`；独立发布任务临时申请 `contents: write`，checkout 固定到该次 CI 的提交且不持久化凭据。
- 发布通过 GitHub API 将 tag 固定到通过 CI 的提交。创建前发现 `main` 已前进时跳过，等待新提交自己的 CI。
- 发布任务串行运行。同版本已有非草稿 Release 时跳过，不修改现有 Release 或 tag。
- 已有 tag 没有 Release 时，仅允许 tag 正好指向该次 CI 提交时补发。指向其他提交或存在同名草稿时失败，需要维护者检查。
- 写请求即使超时也可能已成功。脚本只回读 Release 和 tag 验证状态，不在同次运行中重复发送创建请求。

## 验证与恢复

```sh
python3 -B -m unittest discover -s scripts -p 'test_release.py'
```

测试使用模拟 GitHub API，不会创建远端 tag 或 Release；CI 的两个系统都会执行。

首次发布前，先审阅工作流的发布权限与版本说明。合并当前自动化改动后，如果 `0.0.1` 仍未发布且主分支 CI 成功，将自动发布 `v0.0.1`。

失败时先查看 Actions 的 `Publish version` 日志及远端 Release、tag。确认目标提交和版本后，可重新运行该次 CI 的失败任务；已完成发布会跳过，未完成且没有冲突的发布会继续。不要删除或移动已经发布的 tag 来重复使用版本号。

实现依据：[依赖任务成功后执行](https://docs.github.com/en/actions/how-tos/write-workflows/choose-what-workflows-do/use-jobs)、[Release API 与 tag 提交选择](https://docs.github.com/en/rest/releases/releases#create-a-release)。
