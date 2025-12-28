import React from 'react';
import { useVPN } from '../context/VPNContext';
import './ConnectButton.css';

export const ConnectButton: React.FC = () => {
  const { status, isLoading, toggle } = useVPN();

  const isConnected = status.state === 'connected';
  const isTransitioning = status.state === 'connecting' || status.state === 'disconnecting';

  const getButtonText = () => {
    if (isLoading) return 'Please wait...';
    if (status.state === 'connecting') return 'Connecting...';
    if (status.state === 'disconnecting') return 'Disconnecting...';
    if (isConnected) return 'Disconnect';
    return 'Connect';
  };

  return (
    <button
      className={`connect-button ${isConnected ? 'connected' : ''} ${isTransitioning ? 'transitioning' : ''}`}
      onClick={toggle}
      disabled={isLoading || isTransitioning}
    >
      <div className="button-content">
        <div className="power-icon">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M12 2v10M18.4 6.6a9 9 0 1 1-12.8 0" />
          </svg>
        </div>
        <span>{getButtonText()}</span>
      </div>
    </button>
  );
};
