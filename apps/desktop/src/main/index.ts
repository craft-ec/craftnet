import { app, BrowserWindow, ipcMain, Tray, Menu, nativeImage } from 'electron';
import * as path from 'path';
import { DaemonManager } from './daemon';
import { IPCClient } from './ipc';

let mainWindow: BrowserWindow | null = null;
let tray: Tray | null = null;
let daemonManager: DaemonManager | null = null;
let ipcClient: IPCClient | null = null;

const isDev = process.env.NODE_ENV === 'development';

async function createWindow(): Promise<void> {
  mainWindow = new BrowserWindow({
    width: 400,
    height: 600,
    minWidth: 350,
    minHeight: 500,
    frame: false,
    titleBarStyle: 'hiddenInset',
    transparent: true,
    vibrancy: 'under-window',
    webPreferences: {
      preload: path.join(__dirname, '../preload/index.js'),
      nodeIntegration: false,
      contextIsolation: true,
    },
  });

  if (isDev) {
    mainWindow.loadURL('http://localhost:5173');
    mainWindow.webContents.openDevTools({ mode: 'detach' });
  } else {
    mainWindow.loadFile(path.join(__dirname, '../renderer/index.html'));
  }

  mainWindow.on('close', (e) => {
    if (!app.isQuitting) {
      e.preventDefault();
      mainWindow?.hide();
    }
  });

  mainWindow.on('closed', () => {
    mainWindow = null;
  });
}

function createTray(): void {
  const iconPath = path.join(__dirname, '../../assets/tray-icon.png');
  const icon = nativeImage.createFromPath(iconPath);

  tray = new Tray(icon.isEmpty() ? nativeImage.createEmpty() : icon);

  const contextMenu = Menu.buildFromTemplate([
    { label: 'Show TunnelCraft', click: () => mainWindow?.show() },
    { type: 'separator' },
    { label: 'Connect', click: () => ipcClient?.connect() },
    { label: 'Disconnect', click: () => ipcClient?.disconnect() },
    { type: 'separator' },
    { label: 'Quit', click: () => {
      app.isQuitting = true;
      app.quit();
    }},
  ]);

  tray.setToolTip('TunnelCraft');
  tray.setContextMenu(contextMenu);

  tray.on('click', () => {
    mainWindow?.show();
  });
}

async function startDaemon(): Promise<void> {
  daemonManager = new DaemonManager();
  await daemonManager.start();

  ipcClient = new IPCClient();
  await ipcClient.connect();
}

// IPC handlers from renderer
function setupIpcHandlers(): void {
  ipcMain.handle('vpn:connect', async (_event, config) => {
    try {
      await ipcClient?.connect(config);
      return { success: true };
    } catch (error) {
      return { success: false, error: (error as Error).message };
    }
  });

  ipcMain.handle('vpn:disconnect', async () => {
    try {
      await ipcClient?.disconnect();
      return { success: true };
    } catch (error) {
      return { success: false, error: (error as Error).message };
    }
  });

  ipcMain.handle('vpn:status', async () => {
    try {
      const status = await ipcClient?.getStatus();
      return { success: true, status };
    } catch (error) {
      return { success: false, error: (error as Error).message };
    }
  });

  ipcMain.handle('vpn:setPrivacyLevel', async (_event, level) => {
    try {
      await ipcClient?.setPrivacyLevel(level);
      return { success: true };
    } catch (error) {
      return { success: false, error: (error as Error).message };
    }
  });

  ipcMain.handle('window:minimize', () => {
    mainWindow?.minimize();
  });

  ipcMain.handle('window:close', () => {
    mainWindow?.hide();
  });
}

// Forward daemon events to renderer
function setupEventForwarding(): void {
  ipcClient?.on('stateChange', (state) => {
    mainWindow?.webContents.send('vpn:stateChange', state);
  });

  ipcClient?.on('statsUpdate', (stats) => {
    mainWindow?.webContents.send('vpn:statsUpdate', stats);
  });

  ipcClient?.on('error', (error) => {
    mainWindow?.webContents.send('vpn:error', error);
  });
}

app.whenReady().then(async () => {
  setupIpcHandlers();
  await startDaemon();
  setupEventForwarding();
  await createWindow();
  createTray();
});

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit();
  }
});

app.on('activate', () => {
  if (mainWindow === null) {
    createWindow();
  } else {
    mainWindow.show();
  }
});

app.on('before-quit', async () => {
  await ipcClient?.disconnect();
  await daemonManager?.stop();
});

declare module 'electron' {
  interface App {
    isQuitting?: boolean;
  }
}
