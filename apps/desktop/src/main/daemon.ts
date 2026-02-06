import { spawn, ChildProcess } from 'child_process';
import * as path from 'path';
import * as fs from 'fs';
import * as net from 'net';
import { app } from 'electron';

export class DaemonManager {
  private process: ChildProcess | null = null;
  private isRunning = false;

  private getDaemonPath(): string {
    const isDev = process.env.NODE_ENV === 'development';

    if (isDev) {
      // In development, use the target directory
      return path.join(__dirname, '../../../../target/release/tunnelcraft-daemon');
    }

    // In production, use the bundled daemon
    return path.join(app.getAppPath(), '../daemon/tunnelcraft-daemon');
  }

  async start(): Promise<void> {
    if (this.isRunning) {
      return;
    }

    const daemonPath = this.getDaemonPath();

    // Check if daemon exists
    if (!fs.existsSync(daemonPath)) {
      console.warn('Daemon not found at:', daemonPath);
      console.warn('Running in development mode without daemon');
      return;
    }

    return new Promise((resolve, reject) => {
      try {
        this.process = spawn(daemonPath, [], {
          stdio: ['ignore', 'pipe', 'pipe'],
          detached: false,
        });

        this.process.stdout?.on('data', (data) => {
          console.log('[daemon]', data.toString());
        });

        this.process.stderr?.on('data', (data) => {
          console.error('[daemon]', data.toString());
        });

        this.process.on('error', (err) => {
          console.error('Failed to start daemon:', err);
          this.isRunning = false;
          reject(err);
        });

        this.process.on('exit', (code) => {
          console.log('Daemon exited with code:', code);
          this.isRunning = false;
          this.process = null;
        });

        // Poll for socket existence + connectivity instead of fixed timeout
        const socketPath = process.platform === 'darwin'
          ? '/tmp/tunnelcraft.sock'
          : (process.env.XDG_RUNTIME_DIR || '/tmp') + '/tunnelcraft.sock';
        const maxAttempts = 50; // 50 * 100ms = 5 seconds max
        let attempts = 0;

        const checkSocket = () => {
          attempts++;
          if (!fs.existsSync(socketPath)) {
            if (attempts >= maxAttempts) {
              this.isRunning = false;
              reject(new Error('Daemon socket not created within timeout'));
              return;
            }
            setTimeout(checkSocket, 100);
            return;
          }

          // Socket file exists, try connecting to verify daemon is listening
          const client = net.createConnection({ path: socketPath }, () => {
            client.destroy();
            this.isRunning = true;
            resolve();
          });
          client.on('error', () => {
            if (attempts >= maxAttempts) {
              this.isRunning = false;
              reject(new Error('Daemon socket exists but not accepting connections'));
              return;
            }
            setTimeout(checkSocket, 100);
          });
          // Don't let the check hang
          client.setTimeout(500, () => {
            client.destroy();
            if (attempts < maxAttempts) {
              setTimeout(checkSocket, 100);
            }
          });
        };

        // Start polling after a brief initial delay
        setTimeout(checkSocket, 100);

      } catch (error) {
        reject(error);
      }
    });
  }

  async stop(): Promise<void> {
    if (!this.process || !this.isRunning) {
      return;
    }

    return new Promise((resolve) => {
      this.process?.on('exit', () => {
        this.process = null;
        this.isRunning = false;
        resolve();
      });

      // Try graceful shutdown first
      this.process?.kill('SIGTERM');

      // Force kill after timeout
      setTimeout(() => {
        if (this.process) {
          this.process.kill('SIGKILL');
        }
        resolve();
      }, 5000);
    });
  }

  getIsRunning(): boolean {
    return this.isRunning;
  }
}
