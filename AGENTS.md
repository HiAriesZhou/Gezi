# TMD engineering rules

TMD is a Kaku-first terminal Markdown editor project, invoked as `tmd`. Current
releases provide read-only rendering and navigation. Editing, undo/redo and
saving are planned; distinguish shipped behavior from plans in public copy.

## Entry points

- Installation, usage and limits: `README.md`.
- Module boundaries and behavior contracts: `docs/architecture.md`.
- Reader controls and configuration: `docs/reader.md`; themes: `themes/README.md`.
- Automated checks and real-terminal acceptance: `docs/terminal-testing.md`.
- Release maintenance: `docs/release.md`; version history: `CHANGELOG.md`.
- `CLAUDE.md` references this file instead of maintaining duplicate rules.

## Scope and documentation

- Work on one task at a time. Do not add features or expand scope without authorization.
- Inspect Git status and diffs before editing. Preserve existing changes; never overwrite, reset, clean or stage someone else's work automatically.
- At each milestone, report implementation, verification and outstanding human acceptance. Commit, push, merge and release only within the current authorization.
- Use English for project documentation, repository rules, commit subjects and bodies, branch names, and PR titles and descriptions. Keep intentional multilingual test fixtures intact.
- Keep only public product, usage, architecture, testing and maintenance documentation in the repository. Plans, handoffs, progress notes, reviews, analysis and conversation records belong outside it; do not rename them to keep them under `docs`.
- A short public feature TODO in README is allowed. Detailed implementation plans and progress still belong outside the repository.
- Never put raw conversations, private strategy, account details or credentials in the repository.

## Architecture and behavior

- Keep two crates: `tmd-core` owns parsing, layout, themes, highlighting and stream commitment; `tmd-cli` owns input adapters, ANSI output and terminal control.
- Core must not depend on terminal libraries. It emits structured `Line` / `Frame` values. `Line.size` is logical size; `width()` is the actual column count and `physical_height()` is the occupied row count.
- Large headings are an explicit static-file option. Check terminal capabilities and dimensions before layout, and enlarge only top-level H1. Default `render` and `Streamer::new` always emit ordinary lines. Restore DEC line attributes explicitly; SGR reset is insufficient.
- During one-shot native large-heading output, temporarily enable `NOFLSH` and disable `ECHO`/`ECHONL` so control keys cannot discard half a heading or inject escapes. Preserve canonical mode and ISIG; restore the original termios on exit. Ctrl-C regressions must use actual VINTR, not only a delivered SIGINT.
- Use the existing width module and `width()`, not `width_cjk()`. Wrapping and truncation must not split the character clusters it recognizes.
- Image IO, decoding and Kitty protocol belong in CLI. Core retains references and computes cell layout only. Clip images to the document viewport, bound loading and caches, and delete only session-owned IDs. Never send a global image deletion command. Plain and streaming output retain alt text.
- Commit streaming content at source byte boundaries using only complete lines. Never repaint committed content; resize reflows only the active area. Inter-block blank lines belong to the following block.
- Streaming uses crossterm without raw mode. Restore the cursor and any configured Unicode version on exit.
- The static reader uses the main screen with temporary raw mode and mouse input. Never enter the alternate screen or clear history. Restore termios, cursor and mouse reporting on exit; draw both halves of large headings together. This exception does not change streaming or one-shot input contracts.
- Coordinate stream diagnostics with active-area painting. Input adapters must not write directly to the shared terminal and move its cursor.
- Put Claude tool results on a new line; never rewrite tool lines already in scrollback. Derive event behavior from real samples and ignore unknown events.
- Explain snapshot changes; do not accept them in bulk without analysis. Report discrepancies between library interfaces or event formats and their documentation.

## Verification and communication

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Add replays and real-terminal acceptance required by the task. Passing tests,
acceptable terminal behavior and maintainer acceptance are separate outcomes.

Communicate with the maintainer in concise Chinese. Distinguish source facts,
historical records, documented requirements and proposals awaiting approval.

## Releases

- Increment only the third version component for ordinary features, improvements and fixes; stay on `0.1.x`. Change the second component only for a major change explicitly approved by the maintainer, not for each feature or milestone.
- Each independently delivered feature or fix has its own patch version, in completion order. Pre-release review fixes, tests and documentation belong to that version. Preserve task boundaries when rebuilding history; do not combine independently delivered features or describe test-only fixes as product fixes.
- Obtain maintainer confirmation immediately before final publication, including a PR merge that triggers publication. Withdraw old Releases or tags only with separate confirmation; version cleanup does not authorize it.
- Only macOS is supported. Check and publish jobs both use macOS runners.
- Successful macOS CI for a push to `main` publishes automatically; PR jobs never publish.
- The Cargo workspace version determines the `v<version>` tag. Both crates and the lockfile must agree, and CHANGELOG must have exactly one nonempty `## <version>` section.
- Skip an unchanged version that is already published. Never move an existing tag, overwrite a Release or adopt a draft that existed before the run.
- macOS CI builds and verifies the Apple Silicon package. The publisher uploads only that run's archive and SHA256 file, downloads them to verify their bytes, then rechecks draft ownership and the actual tag before publishing its own draft.
- Release the exact commit that passed CI. After an error, read remote state before retrying a write. See `docs/release.md` for verification and recovery.
