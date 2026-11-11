# Claude JSONL replay fixtures

`plain.jsonl` and `tools.jsonl` are sanitized functional samples. They preserve
recorded event types, ordering and incremental text boundaries to test Markdown
rendering, interrupted-answer regeneration, split tool input and result matching.

Keep only fields used by the adapter: block type/index, text and empty reasoning
deltas, tool name/input, tool associations, invalidation markers, and result
status/duration/cost. Tool IDs are fixture-local placeholders. Duration and cost
are display regression data, not performance or pricing benchmarks.

Samples contain no session identifiers, original user prompts, reasoning text
or signatures. Session/request metadata, timestamps, usage, duplicate assistant
bodies and result bodies have been removed. Ignored events keep only their type
and original order.

Sanitize new fixtures to the same field scope before committing. Unit tests also
construct independent events for unknown types, subagent messages, malformed
input and failed results.
