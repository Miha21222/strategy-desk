// The renderer sends the toggle state with every turn; this checks the other
// end of that wire: what `chat:send` hands to the agent, and that junk from a
// stale renderer cannot silently select the expensive mode.
import { describe, expect, it, vi, beforeEach } from 'vitest';

const handlers = new Map<string, (...args: unknown[]) => unknown>();
const listeners = new Map<string, (...args: unknown[]) => unknown>();

vi.mock('electron', () => ({
  app: { getVersion: () => '0.0.0-test', getAppPath: () => process.cwd(), getPath: () => process.cwd(), isPackaged: false },
  safeStorage: { isEncryptionAvailable: () => false },
  dialog: { showOpenDialog: vi.fn() },
  shell: { openExternal: vi.fn() },
  ipcMain: {
    handle: (name: string, fn: (...args: unknown[]) => unknown) => handlers.set(name, fn),
    on: (name: string, fn: (...args: unknown[]) => unknown) => listeners.set(name, fn),
  },
}));

const startChat = vi.fn(async (_args: { chatKey: string; text: string; mode: string }, _send: unknown) => undefined);
const prewarmFor = vi.fn(async () => undefined);
vi.mock('./agent', () => ({ startChat, stopChat: vi.fn(), prewarmFor, dropWarm: vi.fn() }));

const { registerIpc } = await import('./ipc');
registerIpc(() => null);

beforeEach(() => {
  startChat.mockClear();
  prewarmFor.mockClear();
});

const send = (args: unknown) => listeners.get('chat:send')?.({}, args);
const prewarm = (args: unknown) => listeners.get('chat:prewarm')?.({}, args);

describe('chat:send carries the mode', () => {
  it('passes saver through', () => {
    send({ chatKey: 'c1', text: 'привет', mode: 'saver' });
    expect(startChat).toHaveBeenCalledWith({ chatKey: 'c1', text: 'привет', mode: 'saver' }, expect.any(Function));
  });

  it('passes full through', () => {
    send({ chatKey: 'c1', text: 'привет', mode: 'full' });
    expect(startChat).toHaveBeenCalledWith(expect.objectContaining({ mode: 'full' }), expect.any(Function));
  });

  it('treats anything else as full rather than guessing', () => {
    send({ chatKey: 'c1', text: 'привет', mode: 'cheap' });
    expect(startChat).toHaveBeenCalledWith(expect.objectContaining({ mode: 'full' }), expect.any(Function));
  });

  it('ignores a malformed payload instead of starting a turn', () => {
    send({ chatKey: 5, text: 'привет', mode: 'saver' });
    send(null);
    expect(startChat).not.toHaveBeenCalled();
  });
});

describe('chat:prewarm carries the same mode', () => {
  it('prepares the session for the mode the composer is in', () => {
    prewarm({ chatKey: 'c1', mode: 'saver' });
    expect(prewarmFor).toHaveBeenCalledWith('c1', 'saver');
  });

  it('accepts "no chat open" and defaults the mode', () => {
    prewarm({ chatKey: null });
    expect(prewarmFor).toHaveBeenCalledWith(null, 'full');
  });
});
