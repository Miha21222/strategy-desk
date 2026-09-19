// Actions a card may trigger. Provided by app.tsx so cards stay free of IPC
// wiring and are renderable in isolation (tests, drawer, smoke).
import { createContext } from 'preact';
import type { ChatKind } from '@shared/api';
import type { StressReport } from '@shared/blocks';

export interface CardActions {
  /** Send text as if the user typed it into the current chat. */
  send(prompt: string): void;
  /** Set the current chat's kind (+ title when given). */
  saveChat(kind: ChatKind, name?: string): Promise<void>;
  /** Append a line to profile.style. */
  appendProfileStyle(text: string): Promise<void>;
  /** Open the right-side report drawer on a stress report. */
  openReport(report: StressReport): void;
  /** True while the chat is streaming (chips disabled). */
  busy: boolean;
}

const noop: CardActions = {
  send: () => undefined,
  saveChat: async () => undefined,
  appendProfileStyle: async () => undefined,
  openReport: () => undefined,
  busy: false,
};

export const CardActionsContext = createContext<CardActions>(noop);
