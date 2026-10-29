# tmd 主题

主题是一个 TOML 文件。内置三套：

| 名字 | 说明 |
|---|---|
| `default-dark` | 深色背景，只用 ANSI 16 色，跟着终端配色变 |
| `default-light` | 浅色背景，只用 ANSI 16 色 |
| `neon-tokyo` | 深色背景，真彩色 |

`--theme auto`（默认）按终端背景选 `default-dark` 或 `default-light`：先问终端（最多等 300 ms），问不到看环境变量 `COLORFGBG`，再不行用深色。输出不是终端时直接用深色。

## 自己写主题

放在 `~/.config/tmd/themes/<名字>.toml`（设置了 `XDG_CONFIG_HOME` 时是 `$XDG_CONFIG_HOME/tmd/themes/`），用 `tmd --theme <名字>` 选。名字只能用字母、数字、`-` 和 `_`。和内置主题同名时，用你的文件。

所有字段都可以不写。没写的字段从 `default-dark` 取；文件里写了 `dark = false` 时从 `default-light` 取。写了某个样式，就整个替换它，不和默认样式合并。

最小的例子，只换一级标题的颜色：

```toml
[styles]
h1 = { fg = "#ff007c", bold = true }
```

写错时 tmd 会报出文件名、行号和原因，例如：

```
tmd: theme file ~/.config/tmd/themes/mine.toml: line 3: invalid color "blu": ...
```

## 颜色

| 写法 | 意思 |
|---|---|
| `"ansi:0"` … `"ansi:7"` | 终端的 8 个基本色：黑、红、绿、黄、蓝、品红、青、白 |
| `"bright:0"` … `"bright:7"` | 对应的亮色（`bright:0` 通常是灰色） |
| `"#RRGGBB"` | 真彩色，例如 `"#4DF2FF"` |
| `"default"` | 终端自己的前景色或背景色 |

只用前两种写法的主题会跟着终端配色变；用真彩色的主题颜色固定，但终端要支持真彩色。

## 顶层字段

| 字段 | 类型 | 说明 |
|---|---|---|
| `dark` | 布尔 | 主题是给深色背景用的。决定代码高亮配色（深色 `base16-ocean.dark`，浅色 `InspiredGitHub`），也决定没写的字段从哪套默认主题取。默认 `true` |

## `[styles]`

每个样式是一个表，字段都可以不写：

| 字段 | 类型 | 说明 |
|---|---|---|
| `fg` | 颜色 | 文字颜色，不写是终端默认色 |
| `bg` | 颜色 | 背景色，不写是终端默认背景 |
| `bold` `italic` `underline` `strike` `dim` | 布尔 | 粗体、斜体、下划线、删除线、变暗，不写是 `false` |

| 样式 | 用在哪里 |
|---|---|
| `h1` | 一级标题，也用于它下面的线 |
| `h2` | 二级标题 |
| `h3` | 三级到六级标题 |
| `text` | 正文 |
| `bold` | `**粗体**`。中文字体常没有粗体，建议同时给颜色 |
| `italic` | `*斜体*`。中文字体基本没有斜体，建议同时给颜色 |
| `strike` | `~~删除线~~` |
| `inline_code` | `` `行内代码` `` |
| `code_block` | 代码块。背景色铺满整行；代码颜色来自高亮，没有高亮时用它的 `fg` |
| `link` | 链接文字 |
| `quote_bar` | 引用左边的竖线 |
| `list_marker` | 列表符号和编号 |
| `rule` | 分隔线，以及二级标题下面的线 |
| `table_border` | 表格边框 |
| `table_header` | 表头文字 |
| `muted` | 图片占位 `[图片: …]` |
| `fold` | Claude Code 事件流里折叠成一行的思考和工具调用 |

## `[symbols]`

| 字段 | 类型 | 说明 |
|---|---|---|
| `bullets` | 字符串数组 | 各层无序列表的符号，层数超过时循环使用，至少一个 |
| `task_unchecked` | 字符串 | 未完成的任务框，默认 `☐` |
| `task_checked` | 字符串 | 已完成的任务框，默认 `☑` |
| `quote_bar` | 字符串 | 引用竖线，默认 `│` |
| `rule` | 字符串 | 分隔线重复的符号，默认 `─` |
| `h1_rule` | 字符串 | 一级标题下面的线，默认 `━`；空字符串表示不画 |
| `h2_rule` | 字符串 | 二级标题下面的线，默认 `─`；空字符串表示不画 |

## `[symbols.table]`

表格边框字符。每个都必须正好占一列，否则报错。

| 字段 | 默认 | 位置 |
|---|---|---|
| `top_left` `top_mid` `top_right` | `┌ ┬ ┐` | 顶边 |
| `mid_left` `mid_mid` `mid_right` | `├ ┼ ┤` | 表头下面的分隔 |
| `bottom_left` `bottom_mid` `bottom_right` | `└ ┴ ┘` | 底边 |
| `vertical` | `│` | 竖线 |
| `horizontal` | `─` | 横线 |

## 完整示例

内置主题本身就是完整示例：[`default-dark.toml`](default-dark.toml)、[`default-light.toml`](default-light.toml)、[`neon-tokyo.toml`](neon-tokyo.toml)。
