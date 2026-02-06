import React from 'react';
import { useVPN } from '../context/VPNContext';
import './StatsPanel.css';

const formatBytes = (bytes: number): string => {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
};

const formatDuration = (seconds: number): string => {
  if (seconds < 60) return `${seconds}s`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ${seconds % 60}s`;
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  return `${hours}h ${minutes}m`;
};

export const StatsPanel: React.FC = () => {
  const { stats, nodeStats, status, mode } = useVPN();

  if (status.state !== 'connected') {
    return null;
  }

  const showClientStats = mode === 'client' || mode === 'both';
  const showNodeStats = (mode === 'node' || mode === 'both') && nodeStats;

  return (
    <div className="stats-panel">
      {showClientStats && (
        <>
          <h3 className="panel-title">Client Statistics</h3>
          <div className="stats-grid">
            <div className="stat">
              <svg className="stat-icon upload" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M12 19V5M5 12l7-7 7 7" />
              </svg>
              <div className="stat-info">
                <span className="stat-value">{formatBytes(stats.bytesSent)}</span>
                <span className="stat-label">Uploaded</span>
              </div>
            </div>
            <div className="stat">
              <svg className="stat-icon download" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M12 5v14M5 12l7 7 7-7" />
              </svg>
              <div className="stat-info">
                <span className="stat-value">{formatBytes(stats.bytesReceived)}</span>
                <span className="stat-label">Downloaded</span>
              </div>
            </div>
            <div className="stat">
              <svg className="stat-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <circle cx="12" cy="12" r="10" />
                <path d="M12 6v6l4 2" />
              </svg>
              <div className="stat-info">
                <span className="stat-value">{formatDuration(stats.uptimeSecs)}</span>
                <span className="stat-label">Uptime</span>
              </div>
            </div>
            <div className="stat">
              <svg className="stat-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M22 12h-4l-3 9L9 3l-3 9H2" />
              </svg>
              <div className="stat-info">
                <span className="stat-value">{stats.requestsCompleted}</span>
                <span className="stat-label">Requests</span>
              </div>
            </div>
          </div>
        </>
      )}
      {showNodeStats && (
        <>
          <h3 className="panel-title" style={showClientStats ? { marginTop: 16 } : undefined}>Node Statistics</h3>
          <div className="stats-grid">
            <div className="stat">
              <svg className="stat-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M13 2L3 14h9l-1 8 10-12h-9l1-8z" />
              </svg>
              <div className="stat-info">
                <span className="stat-value">{nodeStats.shards_relayed}</span>
                <span className="stat-label">Shards Relayed</span>
              </div>
            </div>
            <div className="stat">
              <svg className="stat-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M18 13v6a2 2 0 01-2 2H5a2 2 0 01-2-2V8a2 2 0 012-2h6" />
                <polyline points="15 3 21 3 21 9" />
                <line x1="10" y1="14" x2="21" y2="3" />
              </svg>
              <div className="stat-info">
                <span className="stat-value">{nodeStats.requests_exited}</span>
                <span className="stat-label">Requests Exited</span>
              </div>
            </div>
            <div className="stat">
              <svg className="stat-icon upload" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <line x1="12" y1="1" x2="12" y2="23" />
                <path d="M17 5H9.5a3.5 3.5 0 000 7h5a3.5 3.5 0 010 7H6" />
              </svg>
              <div className="stat-info">
                <span className="stat-value">{nodeStats.credits_earned}</span>
                <span className="stat-label">Credits Earned</span>
              </div>
            </div>
            <div className="stat">
              <svg className="stat-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M17 21v-2a4 4 0 00-4-4H5a4 4 0 00-4 4v2" />
                <circle cx="9" cy="7" r="4" />
                <path d="M23 21v-2a4 4 0 00-3-3.87" />
                <path d="M16 3.13a4 4 0 010 7.75" />
              </svg>
              <div className="stat-info">
                <span className="stat-value">{nodeStats.peers_connected}</span>
                <span className="stat-label">Peers</span>
              </div>
            </div>
          </div>
        </>
      )}
    </div>
  );
};
