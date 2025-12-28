import { contextBridge, ipcRenderer } from 'electron';

// Expose protected methods that allow the renderer process to use
// the ipcRenderer without exposing the entire object
contextBridge.exposeInMainWorld('electronAPI', {
  // VPN operations
  connect: (config?: { privacyLevel?: string }) =>
    ipcRenderer.invoke('vpn:connect', config),

  disconnect: () =>
    ipcRenderer.invoke('vpn:disconnect'),

  getStatus: () =>
    ipcRenderer.invoke('vpn:status'),

  setPrivacyLevel: (level: string) =>
    ipcRenderer.invoke('vpn:setPrivacyLevel', level),

  // Window operations
  minimize: () =>
    ipcRenderer.invoke('window:minimize'),

  close: () =>
    ipcRenderer.invoke('window:close'),

  // Event listeners
  onStateChange: (callback: (state: string) => void) => {
    const handler = (_event: Electron.IpcRendererEvent, state: string) => callback(state);
    ipcRenderer.on('vpn:stateChange', handler);
    return () => ipcRenderer.removeListener('vpn:stateChange', handler);
  },

  onStatsUpdate: (callback: (stats: unknown) => void) => {
    const handler = (_event: Electron.IpcRendererEvent, stats: unknown) => callback(stats);
    ipcRenderer.on('vpn:statsUpdate', handler);
    return () => ipcRenderer.removeListener('vpn:statsUpdate', handler);
  },

  onError: (callback: (error: string) => void) => {
    const handler = (_event: Electron.IpcRendererEvent, error: string) => callback(error);
    ipcRenderer.on('vpn:error', handler);
    return () => ipcRenderer.removeListener('vpn:error', handler);
  },
});

// Type definitions for the exposed API
export interface ElectronAPI {
  connect: (config?: { privacyLevel?: string }) => Promise<{ success: boolean; error?: string }>;
  disconnect: () => Promise<{ success: boolean; error?: string }>;
  getStatus: () => Promise<{ success: boolean; status?: unknown; error?: string }>;
  setPrivacyLevel: (level: string) => Promise<{ success: boolean; error?: string }>;
  minimize: () => Promise<void>;
  close: () => Promise<void>;
  onStateChange: (callback: (state: string) => void) => () => void;
  onStatsUpdate: (callback: (stats: unknown) => void) => () => void;
  onError: (callback: (error: string) => void) => () => void;
}

declare global {
  interface Window {
    electronAPI: ElectronAPI;
  }
}
