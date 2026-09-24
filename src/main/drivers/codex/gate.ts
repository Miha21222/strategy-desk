// What the Codex engine is allowed to do, decided here and nowhere else.
//
// Codex cannot remove a tool from the model the way the Claude engine removes
// `Bash` — it can only sandbox it and ask. So this callback IS the app's deny
// list: the engine asks before anything leaves the sandbox, and an MCP call is
// approved only when it belongs to a server the app itself registered.
//
// Measured 2026-09-23: under `approvalPolicy: "never"` an MCP call is refused
// outright ("MCP tool call requires approval, but approval policy is never"),
// so threads run with `on-request` and every answer comes from here.
import type { PermissionPreset } from '@shared/api';
import type { JsonValue, ServerRequest } from './protocol';

export interface GateContext {
  /** MCP servers the app registered for this thread. */
  servers: Set<string>;
  preset: PermissionPreset;
}

/** Nothing is known about the thread (it is gone, or the id did not match). */
const UNKNOWN: GateContext = { servers: new Set(), preset: 'standard' };

export function decide(req: ServerRequest, ctx: GateContext = UNKNOWN): Record<string, JsonValue> {
  switch (req.method) {
    case 'mcpServer/elicitation/request': {
      const name = String((req.params as { serverName?: string }).serverName ?? '');
      return { action: ctx.servers.has(name) ? 'accept' : 'decline', content: {} };
    }
    case 'item/fileChange/requestApproval':
    case 'applyPatchApproval':
      // Writing files is what the extended preset is for; standard says no.
      return { decision: ctx.preset === 'extended' ? 'accept' : 'decline' };
    case 'item/commandExecution/requestApproval':
    case 'execCommandApproval':
      // The shell may run inside the sandbox, never outside it — in either
      // preset. It is the same line the Claude engine draws by forbidding Bash.
      return { decision: 'decline' };
    case 'item/permissions/requestApproval':
      // Grant nothing extra: the thread keeps the sandbox it was started with.
      return { permissions: {}, scope: 'turn' };
    default:
      return {};
  }
}
