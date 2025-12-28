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
  const { stats, status } = useVPN();

  if (status.state !== 'connected') {
    return null;
  }

  return (
    <div className="stats-panel">
      <h3 className="panel-title">Network Statistics</h3>
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
    </div>
  );
};
