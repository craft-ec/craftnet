/**
 * CraftNet Color System
 *
 * Three mode-based color schemes that subtly shift the entire UI feel
 */

export const palette = {
  // Base neutrals - warm dark theme
  black: '#0A0A0B',
  charcoal: '#121214',
  graphite: '#1A1A1E',
  slate: '#252529',
  zinc: '#3A3A42',
  silver: '#6B6B76',
  mist: '#9B9BA6',
  cloud: '#CCCCD4',
  snow: '#F4F4F6',
  white: '#FFFFFF',

  // Client Mode - Cyan/Teal (Protection)
  cyan: {
    50: '#E6FAFE',
    100: '#B3F0FA',
    200: '#80E6F5',
    300: '#4DD9F0',
    400: '#26CFEB',
    500: '#00C4E6',
    600: '#00A3BF',
    700: '#008299',
    800: '#006173',
    900: '#00404D',
  },

  // Node Mode - Amber/Gold (Contribution)
  amber: {
    50: '#FFF8E6',
    100: '#FFEAB3',
    200: '#FFDC80',
    300: '#FFCE4D',
    400: '#FFC226',
    500: '#FFB700',
    600: '#D49800',
    700: '#AA7A00',
    800: '#805C00',
    900: '#553D00',
  },

  // Both Mode - Violet (Harmony)
  violet: {
    50: '#F3E8FF',
    100: '#DFC2FF',
    200: '#CB9CFF',
    300: '#B776FF',
    400: '#A855F7',
    500: '#9333EA',
    600: '#7C2DC4',
    700: '#65269E',
    800: '#4E1F78',
    900: '#371852',
  },

  // Semantic
  success: '#22C55E',
  warning: '#F59E0B',
  error: '#EF4444',
  info: '#3B82F6',
};

export type NodeMode = 'client' | 'node' | 'both';

export const modeColors: Record<NodeMode, {
  primary: string;
  primaryLight: string;
  primaryDark: string;
  gradient: [string, string];
  glow: string;
}> = {
  client: {
    primary: palette.cyan[500],
    primaryLight: palette.cyan[300],
    primaryDark: palette.cyan[700],
    gradient: [palette.cyan[400], palette.cyan[600]],
    glow: 'rgba(0, 196, 230, 0.4)',
  },
  node: {
    primary: palette.amber[500],
    primaryLight: palette.amber[300],
    primaryDark: palette.amber[700],
    gradient: [palette.amber[400], palette.amber[600]],
    glow: 'rgba(255, 183, 0, 0.4)',
  },
  both: {
    primary: palette.violet[500],
    primaryLight: palette.violet[300],
    primaryDark: palette.violet[700],
    gradient: [palette.cyan[500], palette.amber[500]],
    glow: 'rgba(147, 51, 234, 0.4)',
  },
};

export const theme = {
  background: {
    primary: palette.black,
    secondary: palette.charcoal,
    tertiary: palette.graphite,
    card: palette.slate,
    elevated: palette.zinc,
  },
  text: {
    primary: palette.snow,
    secondary: palette.mist,
    tertiary: palette.silver,
    inverse: palette.black,
  },
  border: {
    subtle: 'rgba(255, 255, 255, 0.06)',
    default: 'rgba(255, 255, 255, 0.1)',
    strong: 'rgba(255, 255, 255, 0.15)',
  },
};
