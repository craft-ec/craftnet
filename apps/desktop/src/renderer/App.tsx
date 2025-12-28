import React from 'react';
import { VPNProvider } from './context/VPNContext';
import { TitleBar } from './components/TitleBar';
import { StatusCard } from './components/StatusCard';
import { ConnectButton } from './components/ConnectButton';
import { PrivacyLevelSelector } from './components/PrivacyLevelSelector';
import { StatsPanel } from './components/StatsPanel';
import './styles/App.css';

const App: React.FC = () => {
  return (
    <VPNProvider>
      <div className="app">
        <TitleBar />
        <main className="main-content">
          <StatusCard />
          <ConnectButton />
          <PrivacyLevelSelector />
          <StatsPanel />
        </main>
      </div>
    </VPNProvider>
  );
};

export default App;
