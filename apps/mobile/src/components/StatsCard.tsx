import React from 'react';
import {View, Text, StyleSheet} from 'react-native';
import {ConnectionState, NetworkStats} from '../native/CraftNetVPN';

interface StatsCardProps {
  state: ConnectionState;
  connectedPeers: number;
  stats: NetworkStats;
}

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
  const mins = Math.floor((seconds % 3600) / 60);
  return `${hours}h ${mins}m`;
};

const stateLabels: Record<ConnectionState, string> = {
  disconnected: 'Disconnected',
  connecting: 'Connecting',
  connected: 'Connected',
  disconnecting: 'Disconnecting',
  error: 'Error',
};

export const StatsCard: React.FC<StatsCardProps> = ({
  state,
  connectedPeers,
  stats,
}) => {
  return (
    <View style={styles.container}>
      <StatRow label="Status" value={stateLabels[state]} />
      <StatRow label="Connected Peers" value={connectedPeers.toString()} />
      <StatRow label="Data Sent" value={formatBytes(stats.bytesSent)} />
      <StatRow label="Data Received" value={formatBytes(stats.bytesReceived)} />
      <StatRow label="Uptime" value={formatDuration(stats.uptimeSecs)} />
      <StatRow label="Protocol" value="CraftNet P2P" />
    </View>
  );
};

interface StatRowProps {
  label: string;
  value: string;
}

const StatRow: React.FC<StatRowProps> = ({label, value}) => (
  <View style={styles.row}>
    <Text style={styles.label}>{label}</Text>
    <Text style={styles.value}>{value}</Text>
  </View>
);

const styles = StyleSheet.create({
  container: {
    backgroundColor: '#fff',
    borderRadius: 12,
    padding: 16,
    shadowColor: '#000',
    shadowOffset: {width: 0, height: 2},
    shadowOpacity: 0.1,
    shadowRadius: 4,
    elevation: 3,
    gap: 12,
  },
  row: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
  },
  label: {
    fontSize: 14,
    color: '#888',
  },
  value: {
    fontSize: 14,
    fontWeight: '600',
    color: '#333',
  },
});
