import React, {
  createContext,
  useContext,
  useEffect,
  useState,
  useCallback,
  ReactNode,
} from 'react';
import CraftNetVPN, {
  ConnectionState,
  PrivacyLevel,
  VPNStatus,
  NetworkStats,
} from '../native/CraftNetVPN';

interface VPNContextType {
  // State
  status: VPNStatus;
  stats: NetworkStats;
  isLoading: boolean;
  error: string | null;

  // Actions
  connect: () => Promise<void>;
  disconnect: () => Promise<void>;
  toggle: () => Promise<void>;
  setPrivacyLevel: (level: PrivacyLevel) => Promise<void>;
  clearError: () => void;
}

const defaultStatus: VPNStatus = {
  state: 'disconnected',
  peerId: '',
  connectedPeers: 0,
  credits: 0,
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

export const VPNProvider: React.FC<VPNProviderProps> = ({children}) => {
  const [status, setStatus] = useState<VPNStatus>(defaultStatus);
  const [stats, setStats] = useState<NetworkStats>(defaultStats);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Fetch initial status
  useEffect(() => {
    const fetchStatus = async () => {
      try {
        const currentStatus = await CraftNetVPN.getStatus();
        setStatus(currentStatus);
      } catch (err) {
        console.error('Failed to fetch VPN status:', err);
      }
    };
    fetchStatus();
  }, []);

  // Subscribe to events
  useEffect(() => {
    const unsubscribeState = CraftNetVPN.onStateChange(state => {
      setStatus(prev => ({...prev, state}));
      if (state === 'connected' || state === 'disconnected') {
        setIsLoading(false);
      }
    });

    const unsubscribeError = CraftNetVPN.onError(errorMessage => {
      setError(errorMessage);
      setIsLoading(false);
    });

    const unsubscribeStats = CraftNetVPN.onStatsUpdate(newStats => {
      setStats(newStats);
    });

    return () => {
      unsubscribeState();
      unsubscribeError();
      unsubscribeStats();
    };
  }, []);

  const connect = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      // Set test credits for MVP (no payment)
      await CraftNetVPN.setCredits(1000);
      await CraftNetVPN.connect();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Connection failed');
      setIsLoading(false);
    }
  }, []);

  const disconnect = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      await CraftNetVPN.disconnect();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Disconnect failed');
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
      await CraftNetVPN.setPrivacyLevel(level);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to set privacy level');
    }
  }, []);

  const clearError = useCallback(() => {
    setError(null);
  }, []);

  return (
    <VPNContext.Provider
      value={{
        status,
        stats,
        isLoading,
        error,
        connect,
        disconnect,
        toggle,
        setPrivacyLevel,
        clearError,
      }}>
      {children}
    </VPNContext.Provider>
  );
};
