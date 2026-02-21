/**
 * Settlement Configuration
 *
 * Defines Solana settlement parameters for credit purchase and node rewards.
 * Uses mock mode in development, devnet for testing, mainnet for production.
 */

export type SettlementMode = 'mock' | 'devnet' | 'mainnet';

export interface SettlementConfig {
  mode: SettlementMode;
  rpcUrl: string;
  programId: string;
  commitment: string;
}

// Program IDs
export const CRAFTNET_PROGRAM_ID = '2QQvVc5QmYkLEAFyoVd3hira43NE9qrhjRcuT1hmfMTH';
export const SETTLEMENT_PROGRAM_ID = CRAFTNET_PROGRAM_ID;

// Token mints
export const USDC_MINT_DEVNET = '4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU';
export const USDC_MINT_MAINNET = 'EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v';

// Pricing
export const GB_PRICE_USDC = 0.01;
export const CREDITS_PER_GB = 100;

// Revenue split
export const NODE_SHARE_PERCENT = 70;
export const TREASURY_SHARE_PERCENT = 30;

// Preset configs
export const DEVNET_CONFIG: SettlementConfig = {
  mode: 'devnet',
  rpcUrl: 'https://api.devnet.solana.com',
  programId: CRAFTNET_PROGRAM_ID,
  commitment: 'confirmed',
};

export const MAINNET_CONFIG: SettlementConfig = {
  mode: 'mainnet',
  rpcUrl: 'https://api.mainnet-beta.solana.com',
  programId: CRAFTNET_PROGRAM_ID,
  commitment: 'finalized',
};

const MOCK_CONFIG: SettlementConfig = {
  mode: 'mock',
  rpcUrl: '',
  programId: CRAFTNET_PROGRAM_ID,
  commitment: 'confirmed',
};

export function getSettlementConfig(): SettlementConfig {
  if (__DEV__) {
    return MOCK_CONFIG;
  }
  return DEVNET_CONFIG;
}
