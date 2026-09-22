// The task reducer. Both sides run it — the main process to answer
// `tasks:state`, the renderer to fold the event stream — so a divergence here
// would show as a progress view that disagrees with the process it describes.
import { describe, expect, it } from 'vitest';
import { applyTaskEvent, emptyTask, isTaskId, MAX_TASK_LINES, splitLines, TASK_IDS } from './tasks';

describe('the task id list', () => {
  it('accepts only its own ids, so the renderer cannot name a command', () => {
    for (const id of TASK_IDS) expect(isTaskId(id)).toBe(true);
    for (const junk of ['', 'rm -rf /', 'provision', 'uv sync', 42, null, {}]) expect(isTaskId(junk)).toBe(false);
  });
});

describe('splitting process output', () => {
  it('drops blank lines and carriage returns, and caps a very long one', () => {
    expect(splitLines('a\r\n\r\nb\n')).toEqual(['a', 'b']);
    expect(splitLines('x'.repeat(900))[0]).toHaveLength(400);
  });

  it('survives nothing at all', () => {
    expect(splitLines('')).toEqual([]);
  });
});

describe('folding events', () => {
  const id = 'provision-runtime' as const;

  it('start clears the previous run, so a retry does not show the old failure', () => {
    let s = applyTaskEvent(emptyTask(id), { id, kind: 'end', at: 1, ok: false, error: 'boom' });
    s = applyTaskEvent(s, { id, kind: 'line', text: 'old' });
    s = applyTaskEvent(s, { id, kind: 'start', at: 2, command: 'uv sync' });
    expect(s).toMatchObject({ running: true, ok: null, error: '', lines: [], startedAt: 2 });
  });

  it('keeps the tail once the output outgrows the cap', () => {
    let s = applyTaskEvent(emptyTask(id), { id, kind: 'start', at: 1, command: 'uv sync' });
    for (let i = 0; i < MAX_TASK_LINES + 20; i += 1) s = applyTaskEvent(s, { id, kind: 'line', text: `line ${i}` });
    expect(s.lines).toHaveLength(MAX_TASK_LINES);
    // The tail is what diagnoses a failure; the head is noise.
    expect(s.lines.at(-1)).toBe(`line ${MAX_TASK_LINES + 19}`);
  });

  it('end stops the run and records the verdict', () => {
    let s = applyTaskEvent(emptyTask(id), { id, kind: 'start', at: 1, command: 'uv sync' });
    s = applyTaskEvent(s, { id, kind: 'end', at: 9, ok: false, error: 'exit code 2' });
    expect(s).toMatchObject({ running: false, ok: false, error: 'exit code 2', finishedAt: 9 });
  });

  it('ignores an event belonging to another task', () => {
    const s = applyTaskEvent(emptyTask(id), { id: 'install-chromium', kind: 'line', text: 'nope' });
    expect(s.lines).toEqual([]);
  });
});
