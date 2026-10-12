import { contextBridge, ipcRenderer } from 'electron';
import type { ChatEvent, ChatKind, DeskApi, NewReport, PanelDayState, PickKind, SendArgs, SettingsPatch, TaskEvent, TraderProfile } from '@shared/api';

const api: DeskApi = {
  status: { get: () => ipcRenderer.invoke('status:get') },
  preflight: { run: () => ipcRenderer.invoke('preflight:run') },
  dialog: { pick: (kind: PickKind) => ipcRenderer.invoke('dialog:pick', kind) },
  tv: {
    launch: () => ipcRenderer.invoke('tv:launch'),
    call: (name: string, args: Record<string, unknown>) => ipcRenderer.invoke('tv:call', name, args),
  },
  panel: {
    get: (workflowId: string, date: string) => ipcRenderer.invoke('panel:get', workflowId, date),
    set: (state: PanelDayState) => ipcRenderer.invoke('panel:set', state),
    clear: (workflowId: string, date: string) => ipcRenderer.invoke('panel:clear', workflowId, date),
  },
  settings: {
    get: () => ipcRenderer.invoke('settings:get'),
    set: (patch: SettingsPatch) => ipcRenderer.invoke('settings:set', patch),
    setSecret: (name, value) => ipcRenderer.invoke('settings:setSecret', name, value),
    clearSecret: (name) => ipcRenderer.invoke('settings:clearSecret', name),
  },
  tasks: {
    run: (id) => ipcRenderer.invoke('tasks:run', id),
    stop: (id) => ipcRenderer.send('tasks:stop', id),
    input: (id, text) => ipcRenderer.send('tasks:input', id, text),
    state: (id) => ipcRenderer.invoke('tasks:state', id),
    list: () => ipcRenderer.invoke('tasks:list'),
    command: (id) => ipcRenderer.invoke('tasks:command', id),
    onEvent: (cb) => {
      const handler = (_e: Electron.IpcRendererEvent, ev: TaskEvent) => cb(ev);
      ipcRenderer.on('task:event', handler);
      return () => ipcRenderer.removeListener('task:event', handler);
    },
  },
  runtime: { clear: () => ipcRenderer.invoke('runtime:clear') },
  rules: { get: () => ipcRenderer.invoke('rules:get') },
  profile: {
    get: () => ipcRenderer.invoke('profile:get'),
    set: (p: TraderProfile) => ipcRenderer.invoke('profile:set', p),
  },
  chats: {
    list: () => ipcRenderer.invoke('chats:list'),
    create: (kind?: ChatKind, workflowId?: string | null) => ipcRenderer.invoke('chats:new', kind, workflowId),
    open: (key) => ipcRenderer.invoke('chats:open', key),
    rename: (key, title) => ipcRenderer.invoke('chats:rename', key, title),
    delete: (key) => ipcRenderer.invoke('chats:delete', key),
    pin: (key, pinned) => ipcRenderer.invoke('chats:pin', key, pinned),
    setKind: (key, kind) => ipcRenderer.invoke('chats:setKind', key, kind),
    addReport: (key, report: NewReport) => ipcRenderer.invoke('chats:addReport', key, report),
    deleteReport: (key, reportId) => ipcRenderer.invoke('chats:deleteReport', key, reportId),
  },
  workflows: { list: () => ipcRenderer.invoke('workflows:list') },
  chat: {
    send: (args: SendArgs) => ipcRenderer.send('chat:send', args),
    stop: (chatKey) => ipcRenderer.send('chat:stop', chatKey),
    prewarm: (args) => ipcRenderer.send('chat:prewarm', args),
    onEvent: (cb) => {
      const handler = (_e: Electron.IpcRendererEvent, ev: ChatEvent) => cb(ev);
      ipcRenderer.on('chat:event', handler);
      return () => ipcRenderer.removeListener('chat:event', handler);
    },
  },
  macro: {
    index: () => ipcRenderer.invoke('macro:index'),
    day: (date: string) => ipcRenderer.invoke('macro:day', date),
    openNote: (rel: string) => ipcRenderer.invoke('macro:openNote', rel),
    image: (rel: string) => ipcRenderer.invoke('macro:image', rel),
    humanRead: (args) => ipcRenderer.invoke('macro:humanRead', args),
    dayAnswers: (args) => ipcRenderer.invoke('macro:dayAnswers', args),
    onChanged: (cb) => {
      const handler = (_e: Electron.IpcRendererEvent, dates: string[]) => cb(dates);
      ipcRenderer.on('macro:changed', handler);
      return () => ipcRenderer.removeListener('macro:changed', handler);
    },
  },
  ui: { zoom: (factor: number) => ipcRenderer.invoke('ui:zoom', factor) },
  open: { external: (url) => ipcRenderer.invoke('open:external', url) },
  app: { version: () => ipcRenderer.invoke('app:version') },
};

contextBridge.exposeInMainWorld('desk', api);
