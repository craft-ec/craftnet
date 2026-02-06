import React from 'react';
import { VPNProvider, useVPN } from './context/VPNContext';
import { TitleBar } from './components/TitleBar';
import { StatusCard } from './components/StatusCard';
import { ConnectButton } from './components/ConnectButton';
import { PrivacyLevelSelector } from './components/PrivacyLevelSelector';
import { ModeSelector } from './components/ModeSelector';
import { ExitNodePanel } from './components/ExitNodePanel';
import { CreditPanel } from './components/CreditPanel';
import { StatsPanel } from './components/StatsPanel';
import { RequestPanel } from './components/RequestPanel';
import { SettingsPanel } from './components/SettingsPanel';
import './styles/App.css';

const AppContent: React.FC = () => {
  const { mode } = useVPN();

  return (
    <div className={`app mode-${mode}`}>
      <TitleBar />
      <main className="main-content">
        <StatusCard />
        <ConnectButton />
        <ModeSelector />
        <ExitNodePanel />
        <PrivacyLevelSelector />
        <CreditPanel />
        <StatsPanel />
        <RequestPanel />
        <SettingsPanel />
      </main>
    </div>
  );
};

const App: React.FC = () => {
  return (
    <VPNProvider>
      <AppContent />
    </VPNProvider>
  );
};

export default App;
