import React, { createContext, useContext, useEffect, useState, useCallback, ReactNode } from 'react';

type ConnectionState = 'disconnected' | 'connecting' | 'connected' | 'disconnecting' | 'error';
type PrivacyLevel = 'direct' | 'light' | 'standard' | 'paranoid';

interface VPNStatus {
  state: ConnectionState;
  peerId: string;
  connectedPeers: number;
  credits: number;
  exitNode: string | null;
  errorMessage: string | null;
}

interface NetworkStats {
  bytesSent: number;
  bytesReceived: number;
  requestsMade: number;
  requestsCompleted: number;
  uptimeSecs: number;
}

interface VPNContextType {
  status: VPNStatus;
  stats: NetworkStats;
  privacyLevel: PrivacyLevel;
  isLoading: boolean;
  error: string | null;
  connect: () => Promise<void>;
  disconnect: () => Promise<void>;
  toggle: () => Promise<void>;
  setPrivacyLevel: (level: PrivacyLevel) => Promise<void>;
}

const defaultStatus: VPNStatus = {
  state: 'disconnected',
  peerId: '',
  connectedPeers: 0,
  credits: 0,
  exitNode: null,
  errorMessage: null,
};

const defaultStats: NetworkStats = {
  bytesSent: 0,
  bytesReceived: 0,
  requestsMade: 0,
  requestsCompleted: 0,
  uptimeSecs: 0,
};

const VPNContext = createContext<VPNContextType | null>(null);

export const useVPN = (): VPNContextType => {
  const context = useContext(VPNContext);
  if (!context) {
    throw new Error('useVPN must be used within a VPNProvider');
  }
  return context;
};

interface VPNProviderProps {
  children: ReactNode;
}

export const VPNProvider: React.FC<VPNProviderProps> = ({ children }) => {
  const [status, setStatus] = useState<VPNStatus>(defaultStatus);
  const [stats, setStats] = useState<NetworkStats>(defaultStats);
  const [privacyLevel, setPrivacyLevelState] = useState<PrivacyLevel>('standard');
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Subscribe to VPN events
  useEffect(() => {
    const unsubscribeState = window.electronAPI.onStateChange((state) => {
      setStatus((prev) => ({ ...prev, state: state as ConnectionState }));
    });

    const unsubscribeStats = window.electronAPI.onStatsUpdate((newStats) => {
      setStats(newStats as NetworkStats);
    });

    const unsubscribeError = window.electronAPI.onError((errorMessage) => {
      setError(errorMessage);
      setStatus((prev) => ({ ...prev, state: 'error', errorMessage }));
    });

    // Get initial status
    window.electronAPI.getStatus().then((result) => {
      if (result.success && result.status) {
        setStatus(result.status as VPNStatus);
      }
    });

    return () => {
      unsubscribeState();
      unsubscribeStats();
      unsubscribeError();
    };
  }, []);

  const connect = useCallback(async () => {
    setIsLoading(true);
    setError(null);

    try {
      const result = await window.electronAPI.connect({ privacyLevel });
      if (!result.success) {
        throw new Error(result.error || 'Connection failed');
      }
    } catch (err) {
      setError((err as Error).message);
      setStatus((prev) => ({ ...prev, state: 'error' }));
    } finally {
      setIsLoading(false);
    }
  }, [privacyLevel]);

  const disconnect = useCallback(async () => {
    setIsLoading(true);
    setError(null);

    try {
      const result = await window.electronAPI.disconnect();
      if (!result.success) {
        throw new Error(result.error || 'Disconnect failed');
      }
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setIsLoading(false);
    }
  }, []);

  const toggle = useCallback(async () => {
    if (status.state === 'connected' || status.state === 'connecting') {
      await disconnect();
    } else {
      await connect();
    }
  }, [status.state, connect, disconnect]);

  const setPrivacyLevel = useCallback(async (level: PrivacyLevel) => {
    try {
      const result = await window.electronAPI.setPrivacyLevel(level);
      if (result.success) {
        setPrivacyLevelState(level);
      }
    } catch (err) {
      setError((err as Error).message);
    }
  }, []);

  const value: VPNContextType = {
    status,
    stats,
    privacyLevel,
    isLoading,
    error,
    connect,
    disconnect,
    toggle,
    setPrivacyLevel,
  };

  return <VPNContext.Provider value={value}>{children}</VPNContext.Provider>;
};
