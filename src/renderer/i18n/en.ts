// English stub: partial overrides on top of ru.ts. Anything missing falls
// back to Russian at lookup time.
import type { Dict } from './ru';

// Widen literal strings so English text type-checks against the RU shape.
type Widen<T> = { [K in keyof T]?: T[K] extends string ? string : Widen<T[K]> };

export const en: Widen<Dict> = {
  tabs: { copilot: 'Copilot', workflows: 'Workflows', settings: 'Settings', help: 'Help' },
  sidebar: {
    connections: 'Connections',
    tvRunning: 'TradingView running',
    cdpPort: 'CDP port',
    tvmcp: 'tvmcp server',
    claude: 'Claude',
    launchTv: 'Launch TradingView',
    quick: 'Quick actions',
    chats: 'Chats',
    newChat: 'New chat',
  },
  composer: { send: 'Send', stop: 'Stop', modeSaver: 'Saver', modeFull: 'Full' },
};
