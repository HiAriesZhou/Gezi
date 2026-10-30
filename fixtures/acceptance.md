# TMD 日常使用验收

本文件用于在 Kaku 中检查当前已安装的 tmd。暂不检查其他终端及 Claude 事件流兼容性。

当前预期：文字清楚、层级可辨、表格对齐、代码可读、换行合理；流式刷新时没有明显闪烁或重复内容，结束后终端可以正常输入。

字号说明：当前所有内容使用终端设置的字号。Markdown 标题通过颜色、粗细和分隔线区分，不单独放大字体。三级至六级标题可以共用样式。

## 01 标题层级

以下各级标题的字号应相同。重点比较一级、二级与三级以下的视觉区别。

# 一级标题 H1：项目概览

正文对照：这是一级标题下面的普通段落。Normal body text, 0123456789。

## 二级标题 H2：实现范围

正文对照：这是二级标题下面的普通段落。Normal body text, 0123456789。

### 三级标题 H3：渲染细节

正文对照：这是三级标题下面的普通段落。Normal body text, 0123456789。

#### 四级标题 H4：补充说明

##### 五级标题 H5：补充说明

###### 六级标题 H6：补充说明

## 02 行内样式与链接

中文对照：普通文字，**粗体文字**，*斜体文字*，***粗斜体文字***，~~删除文字~~，`行内代码`。

English: normal text, **bold text**, *italic text*, ***bold italic***, ~~deleted text~~, `inline_code()`.

嵌套样式：**粗体中的 `代码`**，*斜体中的 **粗体***，带标点的代码 `name = "中文🙂"`。

链接：[示例网站](https://example.com)，自动链接：<https://example.com/docs>。链接文字应可读；是否能点击由终端决定。

中文字体如果没有独立的粗体或斜体字形，重点检查配色是否仍能区分这些样式。

## 03 段落、换行与长单词

这是一个中英文混排的长段落，用来观察窗口变窄后的换行。TMD renders Markdown in a terminal. 中文字符、English words、数字 1234567890 和标点（括号）、逗号，应该保持自然间距，不应丢字、重复字，或者把后续内容覆盖掉。将窗口从宽改窄，再从窄改宽，比较阅读体验。

这一行是软换行的前半句，
这一行在同一段内，应接续显示。

这一行末尾使用 Markdown 强制换行。\
这一句应该从新的一行开始。

连续无空格英文：ABCDEFGHIJKLMNOPQRSTUVWXYZ_abcdefghijklmnopqrstuvwxyz_0123456789_Supercalifragilisticexpialidocious。

连续中文：这是一段没有空格的中文长句用于观察窄窗口中的自动换行效果请检查每个字是否完整显示并确认不会遮盖后面的段落。

## 04 中英文、数字与 emoji 表格

检查边框和竖线对齐，右侧数字列靠右；切换到 40、80、120 列比较结果。

| 项目 | 内容 | 数量 |
|:-----|:-----|-----:|
| 中文 | 所有权与借用 | 1 |
| English | ownership / borrow | 23 |
| 中英混排 | Rust 语言 2026 | 456 |
| 常用表情 | 🙂 🚀 ❤️ | 7890 |
| 组合表情 | 👨‍👩‍👧 👍🏽 | 12 |
| 组合字符 | café / café | 34 |
| 空值 | | 0 |

下面三列分别是左对齐、居中、右对齐。

| 左对齐 | 居中对齐 | 右对齐 |
|:-------|:--------:|-------:|
| a | b | c |
| 中文 | 中 English 文 | 123 |
| **粗体** | `代码` | 4567 |

长单元格应按可用空间换行，整张表不应出现边框错位。

| 类型 | 较长的说明 | 状态 |
|:-----|:-----------|:-----|
| 中文 | 本单元格故意写得较长，用于检查列宽调整、单元格换行和相邻行边框。 | 正常 |
| English | A longer sentence should wrap inside its own table cell without shifting the neighboring column. | ready |
| 混排 | 文件名 acceptance.md，路径 /tmp/example，附带中文说明与 emoji 🙂。 | 待观察 |

## 05 列表与任务列表

- 第一项：普通文字。
- 第二项：包含 **粗体**、`代码` 和中英文。
  - 二级列表 A：检查缩进。
  - 二级列表 B：这一项写得较长，用来观察续行是否仍然对齐到列表正文，而不是退回最左边。English words should stay readable here.
    - 三级列表：中文与 emoji 🙂。

1. 第一步：打开文件。
2. 第二步：观察换行。
   1. 子步骤：缩小窗口。
   2. 子步骤：扩大窗口。
3. 第三步：检查后面的段落没有被错误缩进。

- [x] 已完成任务：能看出勾选状态。
- [ ] 未完成任务：与上一项有区别。
- [ ] 较长任务：检查任务列表的续行缩进是否正确，并确认勾选框没有挤占中文文字的位置。

## 06 引用与分隔线

> 这是一段引用。引用标记与普通正文应有区别。
>
> 引用的第二段包含 **强调** 和 `inline code`，在窄窗口里换行后，引用边界应保持清楚。
>
> > 这是嵌套引用，不应与外层文字重叠。

下面是分隔线，之后应恢复普通正文样式。

---

普通正文恢复：没有引用缩进，也没有代码背景。

## 07 代码高亮与长行

Rust：观察关键字、字符串、注释与普通标识符的颜色区别。

```rust
fn greet(name: &str) -> String {
    // 中文注释与 emoji 🙂
    format!("你好，{name}！")
}
let message = greet("Rust");
println!("{message}");
```

TypeScript：

```typescript
type Result = { ok: boolean; message: string };
const result: Result = { ok: true, message: "中文与 English" };
console.log(result.message);
```

Python：

```python
def greet(name: str) -> str:
    return f"你好，{name} 🙂"

print(greet("Python"))
```

Shell：仅用于显示，不需要执行。

```bash
printf '%s\n' 'hello terminal'
for item in alpha beta gamma; do
    printf 'item=%s\n' "$item"
done
```

长代码行：观察当前宽度下的处理，确认没有遮盖后面的内容。该行同时包含较长字符串、中文和 emoji。

```rust
let long_message = "0123456789 abcdefghijklmnopqrstuvwxyz ABCDEFGHIJKLMNOPQRSTUVWXYZ 这是一段用于检查代码长行显示的中文字符串 🙂 🚀";
println!("下一行仍应正常显示");
```

没有指定语言的代码块，内容与空行应保留：

```
第一行：plain text

第三行：中文 English 0123456789
```

## 08 HTML 与图片占位

行内标签：<kbd>Ctrl</kbd> + <kbd>C</kbd>。标签不应原样显示，文字应保留。

<div>HTML 块中的中文与 English 应作为正文显示。</div>

<!-- HIDDEN_TMD_COMMENT：这一整条注释不应显示。 -->

图片仅显示文字占位，不加载图片：![示例图片说明](acceptance-demo.png)。

## 09 长代码块与流式刷新

下面的连续编号便于发现重复、丢行和顺序错误。流式播放时可缩放窗口，并观察新代码行是否持续出现。已进入终端历史的内容不要求随着窗口宽度重新排版。

```text
STREAM 01 | 中文 English 🙂 | alpha
STREAM 02 | 中文 English 🙂 | beta
STREAM 03 | 中文 English 🙂 | gamma
STREAM 04 | 中文 English 🙂 | delta
STREAM 05 | 中文 English 🙂 | epsilon
STREAM 06 | 中文 English 🙂 | zeta
STREAM 07 | 中文 English 🙂 | eta
STREAM 08 | 中文 English 🙂 | theta
STREAM 09 | 中文 English 🙂 | iota
STREAM 10 | 中文 English 🙂 | kappa
STREAM 11 | 中文 English 🙂 | lambda
STREAM 12 | 中文 English 🙂 | mu
STREAM 13 | 中文 English 🙂 | nu
STREAM 14 | 中文 English 🙂 | xi
STREAM 15 | 中文 English 🙂 | omicron
STREAM 16 | 中文 English 🙂 | pi
STREAM 17 | 中文 English 🙂 | rho
STREAM 18 | 中文 English 🙂 | sigma
STREAM 19 | 中文 English 🙂 | tau
STREAM 20 | 中文 English 🙂 | upsilon
STREAM 21 | 中文 English 🙂 | phi
STREAM 22 | 中文 English 🙂 | chi
STREAM 23 | 中文 English 🙂 | psi
STREAM 24 | 中文 English 🙂 | omega
STREAM 25 | 中文 English 🙂 | alpha
STREAM 26 | 中文 English 🙂 | beta
STREAM 27 | 中文 English 🙂 | gamma
STREAM 28 | 中文 English 🙂 | delta
STREAM 29 | 中文 English 🙂 | epsilon
STREAM 30 | 中文 English 🙂 | zeta
STREAM 31 | 中文 English 🙂 | eta
STREAM 32 | 中文 English 🙂 | theta
STREAM 33 | 中文 English 🙂 | iota
STREAM 34 | 中文 English 🙂 | kappa
STREAM 35 | 中文 English 🙂 | lambda
STREAM 36 | 中文 English 🙂 | mu
STREAM 37 | 中文 English 🙂 | nu
STREAM 38 | 中文 English 🙂 | xi
STREAM 39 | 中文 English 🙂 | omicron
STREAM 40 | 中文 English 🙂 | pi
```

## 10 结束位置

如果能看到下面这行，说明内容已到达文件末尾。代码背景、粗体和斜体不应影响之后的 shell 提示符。

TMD_ACCEPTANCE_END：中文、English 与 emoji 🙂 均已显示完毕。
