# 一级标题 H1：中文 English

正文保持常规字号。分别开启和关闭 `--large-headings`，观察上方标题宽高及下方分隔线。

Setext 一级标题
==============

## 二级标题 H2

### 三级标题 H3

#### 四级标题 H4

##### 五级标题 H5

###### 六级标题 H6

# 很长的一级标题用于检查半宽换行 Long heading wraps at half the available columns without overlapping the following body

标题之后的正文不应变大。试用 `--width 20`，标题和正文应各自换行。

# 字符簇：中 é 👩‍💻 ☀️ 文

# **粗体** *斜体* `code` [链接](https://example.com)

> # 引用中的 H1 保持常规字号
>
> 引用正文。

- # 列表中的 H1 保持常规字号
- 普通列表项

```markdown
# 代码中的 H1 保持原文及常规字号
```

| 内容 | 预期 |
|---|---|
| 中文 English | 普通字号且对齐 |
| 👩‍💻 emoji | 不挤占后列 |

# 末尾标题检查滚屏

输出结束后，提示符和下一条命令应为普通字号。选中一个放大标题复制到纯文本编辑区，记录是否出现重复。
