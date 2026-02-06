import React, { useState } from 'react';
import { useVPN } from '../context/VPNContext';
import './CreditPanel.css';

export const CreditPanel: React.FC = () => {
  const { credits, purchaseCredits } = useVPN();
  const [amount, setAmount] = useState('100');
  const [isPurchasing, setIsPurchasing] = useState(false);

  const handlePurchase = async () => {
    const num = parseInt(amount, 10);
    if (isNaN(num) || num <= 0) return;

    setIsPurchasing(true);
    try {
      await purchaseCredits(num);
    } finally {
      setIsPurchasing(false);
    }
  };

  return (
    <div className="credit-panel">
      <h3 className="panel-title">Credits</h3>
      <div className="credit-balance">
        <div className="balance-display">
          <span className="balance-amount">{credits}</span>
          <span className="balance-label">credits</span>
        </div>
      </div>
      <div className="purchase-row">
        <input
          type="number"
          className="credit-input"
          value={amount}
          onChange={(e) => setAmount(e.target.value)}
          placeholder="Amount"
          min="1"
        />
        <button
          className="buy-button"
          onClick={handlePurchase}
          disabled={isPurchasing}
        >
          {isPurchasing ? 'Buying...' : 'Buy Credits'}
        </button>
      </div>
    </div>
  );
};
