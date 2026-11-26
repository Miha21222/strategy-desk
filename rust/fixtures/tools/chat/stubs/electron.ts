// Electron is not loaded by the chat-fold capture: the parts of main that
// touch it (store, runtime, resources) are stubbed next to this file.
export const app = { getPath: () => '/nonexistent', isPackaged: false, getAppPath: () => '/nonexistent' };
export const safeStorage = { isEncryptionAvailable: () => false, encryptString: (s: string) => Buffer.from(s), decryptString: (b: Buffer) => b.toString() };
