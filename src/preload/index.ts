import { contextBridge, ipcRenderer } from 'electron';
import type { ChatEvent, ChatKind, DeskApi, SendArgs, SettingsPatch, TraderProfile } from '@shared/api';

const api: DeskApi = {
  status: { get: () => ipcRenderer.invoke('status:get') },
  tv: { launch: () => ipcRenderer.invoke('tv:launch') },
  settings: {
    get: () => ipcRenderer.invoke('settings:get'),
    set: (patch: SettingsPatch) => ipcRenderer.invoke('settings:set', patch),
    setSecret: (name, value) => ipcRenderer.invoke('settings:setSecret', name, value),
    clearSecret: (name) => ipcRenderer.invoke('settings:clearSecret', name),
  },
  profile: {
    get: () => ipcRenderer.invoke('profile:get'),
    set: (p: TraderProfile) => ipcRenderer.invoke('profile:set', p),
  },
  chats: {
    list: () => ipcRenderer.invoke('chats:list'),
    create: (kind?: ChatKind) => ipcRenderer.invoke('chats:new', kind),
    open: (key) => ipcRenderer.invoke('chats:open', key),
    rename: (key, title) => ipcRenderer.invoke('chats:rename', key, title),
    delete: (key) => ipcRenderer.invoke('chats:delete', key),
    pin: (key, pinned) => ipcRenderer.invoke('chats:pin', key, pinned),
    setKind: (key, kind) => ipcRenderer.invoke('chats:setKind', key, kind),
  },
  chat: {
    send: (args: SendArgs) => ipcRenderer.send('chat:send', args),
    stop: (chatKey) => ipcRenderer.send('chat:stop', chatKey),
    onEvent: (cb) => {
      const handler = (_e: Electron.IpcRendererEvent, ev: ChatEvent) => cb(ev);
      ipcRenderer.on('chat:event', handler);
      return () => ipcRenderer.removeListener('chat:event', handler);
    },
  },
  open: { external: (url) => ipcRenderer.invoke('open:external', url) },
  app: { version: () => ipcRenderer.invoke('app:version') },
};

contextBridge.exposeInMainWorld('desk', api);
