// A push channel shaped like the Agent SDK's streaming-input prompt.
//
// `query({ prompt })` accepts an async iterable of user messages. Handing it
// one that yields nothing yet makes the CLI boot and connect its MCP servers
// while the user is still typing: measured on this machine, a session that was
// held open answers ~3.5 s sooner than one started at send time, because the
// tvmcp import (fastmcp ≈ 0.7 s of it) no longer sits on the critical path.
import type { SDKUserMessage } from '@anthropic-ai/claude-agent-sdk';

export interface InputChannel {
  /** The prompt handed to `query()`. */
  stream: AsyncGenerator<SDKUserMessage>;
  /** Queue one user turn; safe before anything consumes the stream. */
  push: (text: string) => void;
  /** Let the stream finish, which ends the session after the current turn. */
  end: () => void;
}

export function userMessage(text: string): SDKUserMessage {
  return { type: 'user', message: { role: 'user', content: text }, parent_tool_use_id: null, session_id: '' } as SDKUserMessage;
}

export function createInputChannel(): InputChannel {
  const queue: SDKUserMessage[] = [];
  let wake: (() => void) | null = null;
  let ended = false;

  const nudge = () => {
    const w = wake;
    wake = null;
    w?.();
  };

  async function* stream(): AsyncGenerator<SDKUserMessage> {
    for (;;) {
      while (queue.length) yield queue.shift() as SDKUserMessage;
      if (ended) return;
      await new Promise<void>((resolve) => {
        wake = resolve;
      });
    }
  }

  return {
    stream: stream(),
    push(text: string) {
      if (ended) return;
      queue.push(userMessage(text));
      nudge();
    },
    end() {
      ended = true;
      nudge();
    },
  };
}
