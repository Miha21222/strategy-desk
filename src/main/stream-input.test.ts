// The push channel behind a prewarmed session: it must hold the stream open
// with nothing queued (that is the whole point) and still deliver whatever was
// pushed before anyone started reading.
import { describe, expect, it } from 'vitest';
import { createInputChannel, userMessage } from './stream-input';

const drain = async (gen: AsyncGenerator<unknown>, n: number) => {
  const out: unknown[] = [];
  for (let i = 0; i < n; i++) {
    const r = await gen.next();
    if (r.done) break;
    out.push(r.value);
  }
  return out;
};

const textOf = (m: unknown) => (m as { message: { content: string } }).message.content;

describe('input channel', () => {
  it('shapes a user message the SDK accepts', () => {
    const m = userMessage('привет');
    expect(m.type).toBe('user');
    expect(m.message.role).toBe('user');
    expect(m.message.content).toBe('привет');
  });

  it('delivers what was pushed before the stream was read', async () => {
    const ch = createInputChannel();
    ch.push('one');
    ch.push('two');
    expect((await drain(ch.stream, 2)).map(textOf)).toEqual(['one', 'two']);
  });

  it('waits instead of ending when the queue is empty', async () => {
    const ch = createInputChannel();
    const pending = ch.stream.next();
    const race = await Promise.race([pending.then(() => 'resolved'), new Promise((r) => setTimeout(() => r('still waiting'), 30))]);
    expect(race).toBe('still waiting');
    ch.push('later');
    expect(textOf((await pending).value)).toBe('later');
  });

  it('end() finishes the stream and ignores later pushes', async () => {
    const ch = createInputChannel();
    ch.push('one');
    ch.end();
    ch.push('dropped');
    const seen = await drain(ch.stream, 5);
    expect(seen.map(textOf)).toEqual(['one']);
    expect((await ch.stream.next()).done).toBe(true);
  });
});
