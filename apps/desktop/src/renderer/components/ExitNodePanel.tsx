import React, { useState } from 'react';
import { useVPN } from '../context/VPNContext';
import type { ExitNode } from '../context/VPNContext';
import './ExitNodePanel.css';

const getCountryFlag = (countryCode: string): string => {
  const codePoints = countryCode
    .toUpperCase()
    .split('')
    .map((char) => 127397 + char.charCodeAt(0));
  return String.fromCodePoint(...codePoints);
};

const scoreClass = (score: number): string => {
  if (score <= 40) return 'score-good';
  if (score <= 70) return 'score-medium';
  return 'score-poor';
};

export const ExitNodePanel: React.FC = () => {
  const { mode, exitNode, availableExits, setExitNode } = useVPN();
  const [showList, setShowList] = useState(false);

  if (mode !== 'client' && mode !== 'both') {
    return null;
  }

  const handleSelect = (node: ExitNode) => {
    setExitNode(node);
    setShowList(false);
  };

  const sorted = [...availableExits].sort((a, b) => a.score - b.score);

  return (
    <div className="exit-node-panel">
      <h3 className="panel-title">Exit Node</h3>

      {exitNode ? (
        <div className="current-exit">
          <span className="exit-flag">{getCountryFlag(exitNode.countryCode)}</span>
          <div className="exit-details">
            <span className="exit-city">{exitNode.city}</span>
            <span className="exit-country">
              {exitNode.countryName} &middot; {exitNode.region.toUpperCase()}
            </span>
          </div>
          <span className={`exit-score-badge ${scoreClass(exitNode.score)}`}>
            {exitNode.score}
          </span>
        </div>
      ) : (
        <div className="no-exit">No exit selected</div>
      )}

      <button
        className="change-exit-button"
        onClick={() => setShowList(!showList)}
      >
        {showList ? 'Close' : exitNode ? 'Change' : 'Select Exit'}
      </button>

      {showList && (
        <div className="exit-list">
          {sorted.map((node) => (
            <button
              key={node.id}
              className={`exit-list-item ${exitNode?.id === node.id ? 'selected' : ''}`}
              onClick={() => handleSelect(node)}
            >
              <span className="exit-item-flag">{getCountryFlag(node.countryCode)}</span>
              <div className="exit-item-details">
                <span className="exit-item-city">{node.city}</span>
                <span className="exit-item-meta">
                  {node.countryCode} &middot; {node.latencyMs}ms &middot; {node.loadPercent}% load
                </span>
              </div>
              <span className={`exit-item-score ${scoreClass(node.score)}`}>
                {node.score}
              </span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
};
