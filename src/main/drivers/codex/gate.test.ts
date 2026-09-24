// The approval gate is the Codex engine's deny list: Codex cannot take a tool
// away from the model, so what the model may actually DO is decided by these
// answers. The request shapes are the ones the live engine sent on 2026-09-23.
import { describe, expect, it } from 'vitest';
import { decide, type GateContext } from './gate';
import type { ServerRequest } from './protocol';

const ctx = (preset: GateContext['preset'], servers: string[] = ['tradingview', 'gt-database']): GateContext => ({ servers: new Set(servers), preset });

const elicitation = (serverName: string): ServerRequest => ({
  // A live request id really can be 0; the driver must answer it like any other.
  id: 0,
  method: 'mcpServer/elicitation/request',
  params: {
    threadId: 'thread-1',
    serverName,
    mode: 'form',
    message: `Allow the ${serverName} MCP server to run tool "ping"?`,
    _meta: { codex_approval_kind: 'mcp_tool_call' },
  },
});

describe('the approval gate', () => {
  it('approves a tool call on a server the app itself registered', () => {
    expect(decide(elicitation('tradingview'), ctx('standard'))).toEqual({ action: 'accept', content: {} });
    expect(decide(elicitation('gt-database'), ctx('standard'))).toEqual({ action: 'accept', content: {} });
  });

  it('declines a server the app never registered', () => {
    // Nothing else should be able to reach the thread, but if it does, the
    // answer is no — the app's own map is the whole allowlist.
    expect(decide(elicitation('some-other-server'), ctx('extended'))).toEqual({ action: 'decline', content: {} });
  });

  it('declines everything when the thread is unknown', () => {
    expect(decide(elicitation('tradingview'))).toEqual({ action: 'decline', content: {} });
  });

  it('never lets the shell out of the sandbox, in either preset', () => {
    const req: ServerRequest = { id: 1, method: 'item/commandExecution/requestApproval', params: { threadId: 'thread-1', command: 'rm -rf .' } };
    expect(decide(req, ctx('standard'))).toEqual({ decision: 'decline' });
    expect(decide(req, ctx('extended'))).toEqual({ decision: 'decline' });
    expect(decide({ ...req, method: 'execCommandApproval' }, ctx('extended'))).toEqual({ decision: 'decline' });
  });

  it('allows writing files only in the extended preset', () => {
    const req: ServerRequest = { id: 2, method: 'item/fileChange/requestApproval', params: { threadId: 'thread-1' } };
    expect(decide(req, ctx('standard'))).toEqual({ decision: 'decline' });
    expect(decide(req, ctx('extended'))).toEqual({ decision: 'accept' });
    expect(decide({ ...req, method: 'applyPatchApproval' }, ctx('standard'))).toEqual({ decision: 'decline' });
  });

  it('grants no extra permissions: the thread keeps the sandbox it started with', () => {
    const req: ServerRequest = { id: 3, method: 'item/permissions/requestApproval', params: { threadId: 'thread-1', permissions: { network: { allowAll: true } } } };
    expect(decide(req, ctx('extended'))).toEqual({ permissions: {}, scope: 'turn' });
  });

  it('answers an unknown request with an empty result instead of hanging it', () => {
    expect(decide({ id: 4, method: 'something/new', params: {} }, ctx('standard'))).toEqual({});
  });
});
