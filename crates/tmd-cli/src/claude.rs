//! Claude Code JSONL adapter. Only incremental parent-message events render;
//! assistant messages repeat the same content and must not render again.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde_json::Value;
use tmd_core::{Frame, Line, Span, Streamer, Theme, clusters, text_width, truncate};

use crate::stream::Source;

enum Block {
    Text(Box<Streamer>),
    Thinking(usize),
    Tool {
        id: String,
        name: String,
        input: String,
        started: Duration,
    },
}

struct Tool {
    name: String,
    started: Duration,
}

pub struct ClaudeStream {
    pending: String,
    line: usize,
    current: Option<(u64, Block)>,
    active: Vec<Line>,
    tools: HashMap<String, Tool>,
    width: u16,
    theme: Theme,
    debug: bool,
    diagnostics: Vec<String>,
    now: Box<dyn Fn() -> Duration>,
}

impl ClaudeStream {
    pub fn new(width: u16, theme: Theme, debug: bool) -> Self {
        let start = Instant::now();
        Self::with_clock(width, theme, debug, move || start.elapsed())
    }

    fn with_clock(
        width: u16,
        theme: Theme,
        debug: bool,
        now: impl Fn() -> Duration + 'static,
    ) -> Self {
        Self {
            pending: String::new(),
            line: 0,
            current: None,
            active: Vec::new(),
            tools: HashMap::new(),
            width: width.max(1),
            theme,
            debug,
            diagnostics: Vec::new(),
            now: Box::new(now),
        }
    }

    fn folded(&self, text: &str) -> Line {
        let clean: String = text
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect();
        let width = usize::from(self.width);
        let text = if text_width(&clean) > width {
            format!("{}…", truncate(&clean, width.saturating_sub(1)))
        } else {
            clean
        };
        Line {
            spans: vec![Span::new(text, self.theme.fold.clone())],
            src: 0..0,
            size: Default::default(),
        }
    }

    fn close_block(&mut self, out: &mut Vec<Line>) {
        let Some((_, block)) = self.current.take() else {
            return;
        };
        self.active.clear();
        match block {
            Block::Text(mut text) => out.extend(text.finish().committed),
            Block::Thinking(count) => {
                out.push(self.folded(&if count == 0 {
                    "▸ 思考 · 内容不可见".into()
                } else {
                    format!("▸ 思考 · {count} 字")
                }));
            }
            Block::Tool {
                id,
                name,
                input,
                started,
            } => {
                let summary: String = clusters(&input).take(60).collect();
                let summary = if summary.is_empty() {
                    String::new()
                } else {
                    format!(" · {summary}")
                };
                out.push(self.folded(&format!("▸ 工具 · {name}{summary}")));
                if !id.is_empty() {
                    self.tools.insert(id, Tool { name, started });
                }
            }
        }
    }

    fn parse_line(&mut self, line: &str, out: &mut Vec<Line>) {
        self.line += 1;
        if line.trim().is_empty() {
            return;
        }
        match serde_json::from_str::<Value>(line) {
            Ok(value) => self.event(&value, out),
            Err(err) if self.debug => {
                // Never log the original record (it can contain private text).
                self.diagnostics.push(format!(
                    "tmd: JSONL line {}: invalid JSON ({:?}, column {})",
                    self.line,
                    err.classify(),
                    err.column()
                ));
            }
            Err(_) => {}
        }
    }

    fn event(&mut self, value: &Value, out: &mut Vec<Line>) {
        // Child-agent messages share block indexes with their parent. Their
        // parent tool result is still handled from the top-level user message.
        if value
            .get("parent_tool_use_id")
            .is_some_and(|id| !id.is_null())
        {
            return;
        }
        match value["type"].as_str() {
            Some("stream_event") => {
                let event = &value["event"];
                match event["type"].as_str() {
                    Some("message_start") => self.close_block(out),
                    Some("content_block_start") => {
                        let Some(index) = event["index"].as_u64() else {
                            return;
                        };
                        let content = &event["content_block"];
                        if !matches!(
                            content["type"].as_str(),
                            Some("text" | "thinking" | "tool_use")
                        ) {
                            return;
                        }
                        self.close_block(out);
                        let block = match content["type"].as_str() {
                            Some("text") => {
                                let mut text = Streamer::new(self.width, self.theme.clone());
                                let frame = text.push(content["text"].as_str().unwrap_or(""));
                                out.extend(frame.committed);
                                self.active = frame.active;
                                Block::Text(Box::new(text))
                            }
                            Some("thinking") => Block::Thinking(
                                content["thinking"].as_str().unwrap_or("").chars().count(),
                            ),
                            Some("tool_use") => Block::Tool {
                                id: content["id"].as_str().unwrap_or("").into(),
                                name: content["name"].as_str().unwrap_or("?").into(),
                                input: content
                                    .get("input")
                                    .filter(|v| v.as_object().is_some_and(|o| !o.is_empty()))
                                    .map(Value::to_string)
                                    .unwrap_or_default(),
                                started: (self.now)(),
                            },
                            _ => return,
                        };
                        self.current = Some((index, block));
                    }
                    Some("content_block_delta") => {
                        let Some((index, block)) = &mut self.current else {
                            return;
                        };
                        if event["index"].as_u64() != Some(*index) {
                            return;
                        }
                        let delta = &event["delta"];
                        match (block, delta["type"].as_str()) {
                            (Block::Text(text), Some("text_delta")) => {
                                let frame = text.push(delta["text"].as_str().unwrap_or(""));
                                out.extend(frame.committed);
                                self.active = frame.active;
                            }
                            (Block::Thinking(count), Some("thinking_delta")) => {
                                *count += delta["thinking"].as_str().unwrap_or("").chars().count()
                            }
                            (Block::Tool { input, .. }, Some("input_json_delta")) => {
                                input.push_str(delta["partial_json"].as_str().unwrap_or(""))
                            }
                            _ => {}
                        }
                    }
                    Some("content_block_stop")
                        if self
                            .current
                            .as_ref()
                            .is_some_and(|(i, _)| event["index"].as_u64() == Some(*i)) =>
                    {
                        self.close_block(out)
                    }
                    Some("message_stop") => {
                        self.close_block(out);
                        if value.get("abandoned_blocks").is_some_and(|v| !v.is_null()) {
                            out.push(self.folded("▸ 回答中断，重新生成"));
                        }
                    }
                    _ => {}
                }
            }
            Some("user") => {
                if let Some(content) = value["message"]["content"].as_array() {
                    for item in content {
                        if item["type"] != "tool_result" {
                            continue;
                        }
                        let Some(id) = item["tool_use_id"].as_str() else {
                            continue;
                        };
                        if let Some(tool) = self.tools.remove(id) {
                            self.close_block(out);
                            let text = if item["is_error"].as_bool() == Some(true) {
                                format!("✗ {} · 出错", tool.name)
                            } else {
                                let elapsed =
                                    (self.now)().saturating_sub(tool.started).as_secs_f64();
                                format!("✓ {} · 用时 {elapsed:.1} s", tool.name)
                            };
                            out.push(self.folded(&text));
                        }
                    }
                }
            }
            Some("result") => {
                self.close_block(out);
                let mut text = String::from(if value["is_error"].as_bool() == Some(true) {
                    "失败"
                } else {
                    "完成"
                });
                if let Some(ms) = value["duration_ms"]
                    .as_f64()
                    .filter(|v| v.is_finite() && *v >= 0.0)
                {
                    text.push_str(&format!(" · 总用时 {:.1} s", ms / 1000.0));
                }
                if let Some(cost) = value["total_cost_usd"]
                    .as_f64()
                    .filter(|v| v.is_finite() && *v >= 0.0)
                {
                    text.push_str(&format!(" · ${cost:.4}"));
                }
                out.push(self.folded(&text));
            }
            _ => {}
        }
    }

    fn frame(&self, committed: Vec<Line>) -> Frame {
        Frame {
            committed,
            active: self.active.clone(),
        }
    }
}

impl Source for ClaudeStream {
    fn take_diagnostics(&mut self) -> Vec<String> {
        std::mem::take(&mut self.diagnostics)
    }

    fn push(&mut self, chunk: &str) -> Frame {
        self.pending.push_str(chunk);
        let mut out = Vec::new();
        if let Some(end) = self.pending.rfind('\n') {
            let complete: String = self.pending.drain(..=end).collect();
            for line in complete.lines() {
                self.parse_line(line, &mut out);
            }
        }
        self.frame(out)
    }

    fn finish(&mut self) -> Frame {
        let mut out = Vec::new();
        let pending = std::mem::take(&mut self.pending);
        if !pending.is_empty() {
            self.parse_line(&pending, &mut out);
        }
        self.close_block(&mut out);
        self.tools.clear();
        Frame {
            committed: out,
            active: Vec::new(),
        }
    }

    fn set_width(&mut self, width: u16) {
        self.width = width.max(1);
        if let Some((_, Block::Text(text))) = &mut self.current {
            text.set_width(self.width);
            self.active = text.push("").active;
        }
    }
}

#[cfg(test)]
mod tests;
