// The shape of workflows/<id>/workflow.yaml. Kept out of src/main/workflows.ts
// (which imports electron) so the schema can be unit-tested against the files
// that actually ship.
import { z } from 'zod';
import { CHAT_KINDS } from './api';
import { PanelSchema } from './panel';

export const WorkflowPromptSchema = z.object({
  id: z.string().min(1),
  label: z.string().min(1),
  prompt: z.string().min(1),
});

export const WorkflowFileSchema = z.object({
  id: z.string().regex(/^[a-z0-9-]+$/),
  title: z.string().min(1),
  description: z.string().default(''),
  promptBlock: z.string().min(1),
  shelves: z.array(z.enum(CHAT_KINDS as unknown as [string, ...string[]])).default([]),
  quickPrompts: z.array(WorkflowPromptSchema).default([]),
  reports: z.array(WorkflowPromptSchema).default([]),
  /** Optional day panel: the app drives it deterministically (see panel.ts). */
  panel: PanelSchema.optional(),
});

export type WorkflowFile = z.infer<typeof WorkflowFileSchema>;

/** One readable line per validation problem, for the loader's console warning. */
export function describeIssues(error: z.ZodError): string {
  return error.issues.map((i) => `${i.path.join('.') || '(root)'}: ${i.message}`).join('; ');
}
