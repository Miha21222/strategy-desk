// JSON-RPC client for `codex app-server`.
//
// Framing is one JSON object per line — verified against codex-cli 0.154.0 on
// 2026-09-23, together with everything else this file assumes:
//
//   * `initialize` answers with the resolved `codexHome`, which is how the
//     driver proves it is NOT running out of the user's own ~/.codex;
//   * the server asks the client for approvals as ordinary JSON-RPC requests,
//     and **a request id may be `0`** — a falsy id that a naive `if (msg.id)`
//     turns into a hang, because the server then waits forever for an answer
//     nobody sent;
//   * an MCP tool call under `approvalPolicy: "on-request"` arrives as
//     `mcpServer/elicitation/request` and must be answered with
//     `{action: "accept" | "decline", content: {}}`.
import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';

export type JsonValue = unknown;

export interface ServerRequest {
  id: number | string;
  method: string;
  params: Record<string, JsonValue>;
}

export interface Notification {
  method: string;
  params: Record<string, JsonValue>;
}

export interface AppServerOptions {
  command: string;
  args?: string[];
  /** The private CODEX_HOME this process must use. */
  codexHome: string;
  cwd?: string;
  onNotification: (n: Notification) => void;
  /** Answer a server-initiated request; the returned value is the JSON-RPC result. */
  onServerRequest: (r: ServerRequest) => Promise<Record<string, JsonValue>>;
  onStderr?: (data: string) => void;
  onExit?: (code: number | null) => void;
}

export class AppServerError extends Error {
  constructor(
    message: string,
    public readonly code?: number,
  ) {
    super(message);
  }
}

export class AppServer {
  private child: ChildProcessWithoutNullStreams;
  private buf = '';
  private nextId = 1;
  private pending = new Map<number, { resolve: (v: Record<string, JsonValue>) => void; reject: (e: unknown) => void }>();
  private exited = false;

  constructor(private readonly opts: AppServerOptions) {
    const env = { ...process.env, CODEX_HOME: opts.codexHome };
    this.child = spawn(opts.command, opts.args ?? ['app-server', '--stdio'], {
      cwd: opts.cwd,
      env,
      stdio: ['pipe', 'pipe', 'pipe'],
      // `codex` on Windows is a .cmd shim from npm; without a shell it is not spawnable.
      shell: process.platform === 'win32',
    }) as ChildProcessWithoutNullStreams;

    this.child.stdout.setEncoding('utf8');
    this.child.stdout.on('data', (d: string) => this.onData(d));
    this.child.stderr.setEncoding('utf8');
    this.child.stderr.on('data', (d: string) => opts.onStderr?.(d));
    this.child.on('exit', (code) => {
      this.exited = true;
      const err = new AppServerError(`app-server exited (${code ?? 'signal'})`);
      for (const [, p] of this.pending) p.reject(err);
      this.pending.clear();
      opts.onExit?.(code);
    });
    this.child.on('error', (e) => opts.onStderr?.(String((e as Error).message ?? e)));
  }

  get alive(): boolean {
    return !this.exited;
  }

  private onData(chunk: string): void {
    this.buf += chunk;
    for (;;) {
      const i = this.buf.indexOf('\n');
      if (i < 0) break;
      const line = this.buf.slice(0, i).trim();
      this.buf = this.buf.slice(i + 1);
      if (!line) continue;
      let msg: Record<string, JsonValue>;
      try {
        msg = JSON.parse(line) as Record<string, JsonValue>;
      } catch {
        // Not our protocol (a stray print): keep it in the stderr tail rather than crash.
        this.opts.onStderr?.(line);
        continue;
      }
      this.dispatch(msg);
    }
  }

  private dispatch(msg: Record<string, JsonValue>): void {
    const id = msg.id as number | string | undefined;
    const method = msg.method as string | undefined;
    // A server request: it HAS an id and a method, and that id may be 0.
    if (id !== undefined && id !== null && method) {
      void this.answer({ id, method, params: (msg.params as Record<string, JsonValue>) ?? {} });
      return;
    }
    if (method) {
      this.opts.onNotification({ method, params: (msg.params as Record<string, JsonValue>) ?? {} });
      return;
    }
    if (typeof id === 'number' && this.pending.has(id)) {
      const p = this.pending.get(id);
      this.pending.delete(id);
      if (msg.error) {
        const e = msg.error as { message?: string; code?: number };
        p?.reject(new AppServerError(e?.message ?? 'app-server error', e?.code));
      } else {
        p?.resolve((msg.result as Record<string, JsonValue>) ?? {});
      }
    }
  }

  private async answer(req: ServerRequest): Promise<void> {
    let result: Record<string, JsonValue> = {};
    try {
      result = await this.opts.onServerRequest(req);
    } catch (e) {
      this.opts.onStderr?.(`approval handler failed: ${String((e as Error)?.message ?? e)}`);
    }
    this.write({ jsonrpc: '2.0', id: req.id, result });
  }

  private write(obj: Record<string, JsonValue>): void {
    if (this.exited) return;
    this.child.stdin.write(`${JSON.stringify(obj)}\n`);
  }

  request(method: string, params: Record<string, JsonValue>): Promise<Record<string, JsonValue>> {
    if (this.exited) return Promise.reject(new AppServerError('app-server is not running'));
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.write({ jsonrpc: '2.0', id, method, params });
    });
  }

  /** Fire-and-forget notification (the client rarely needs one). */
  notify(method: string, params: Record<string, JsonValue>): void {
    this.write({ jsonrpc: '2.0', method, params });
  }

  kill(): void {
    if (this.exited) return;
    try {
      this.child.stdin.end();
    } catch {
      /* already gone */
    }
    this.child.kill();
  }
}

export function startAppServer(opts: AppServerOptions): AppServer {
  return new AppServer(opts);
}
