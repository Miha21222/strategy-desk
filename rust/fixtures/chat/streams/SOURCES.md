# Recorded and generated CLI streams

| File | From |
| --- | --- |
| `text_turn.jsonl`, `tool_turn.jsonl`, `interrupted.jsonl`, `auth_error.jsonl`, `resume_missing.jsonl` | copies of `rust/crates/desk-app/src/claude/testdata/` (recorded from claude 2.1.332, anonymised by desk-app) |
| `fake_tools.jsonl` | `fake-claude --events 80 --seed 1 --tool-every 1` |
| `fake_garbage.jsonl` | `fake-claude --events 40 --seed 2 --garbage-every 5` |
| `fake_die.jsonl` | `fake-claude --events 40 --seed 3 --die-after 15` (ends in half a line; the real process exits with code 3) |
| `empty.jsonl` | nothing: a process that dies before its first line |

The `cwd` field of fake-claude's init line is replaced by `C:\Users\T\Documents\Strategy Desk`.
Regenerate with `bash rust/fixtures/tools/capture-chat.sh`.
