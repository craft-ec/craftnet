/**
 * TunnelCraft Typography System
 *
 * Using system fonts with fallbacks for a native feel
 * with specific weights for hierarchy
 */

import { Platform, TextStyle } from 'react-native';

// Font families - using system fonts for performance
// Custom fonts can be added via react-native-asset
export const fonts = {
  // Display font - for headings and emphasis
  display: Platform.select({
    ios: 'SF Pro Display',
    android: 'Roboto',
    default: 'System',
  }),
  // Body font - for readability
  body: Platform.select({
    ios: 'SF Pro Text',
    android: 'Roboto',
    default: 'System',
  }),
  // Mono font - for stats and numbers
  mono: Platform.select({
    ios: 'SF Mono',
    android: 'Roboto Mono',
    default: 'monospace',
  }),
};

// Type scale
export const typography: Record<string, TextStyle> = {
  // Display styles
  displayLarge: {
    fontFamily: fonts.display,
    fontSize: 48,
    fontWeight: '700',
    lineHeight: 56,
    letterSpacing: -1,
  },
  displayMedium: {
    fontFamily: fonts.display,
    fontSize: 36,
    fontWeight: '600',
    lineHeight: 44,
    letterSpacing: -0.5,
  },
  displaySmall: {
    fontFamily: fonts.display,
    fontSize: 28,
    fontWeight: '600',
    lineHeight: 36,
    letterSpacing: -0.25,
  },

  // Heading styles
  headingLarge: {
    fontFamily: fonts.display,
    fontSize: 24,
    fontWeight: '600',
    lineHeight: 32,
    letterSpacing: 0,
  },
  headingMedium: {
    fontFamily: fonts.display,
    fontSize: 20,
    fontWeight: '600',
    lineHeight: 28,
    letterSpacing: 0,
  },
  headingSmall: {
    fontFamily: fonts.display,
    fontSize: 17,
    fontWeight: '600',
    lineHeight: 24,
    letterSpacing: 0,
  },

  // Body styles
  bodyLarge: {
    fontFamily: fonts.body,
    fontSize: 17,
    fontWeight: '400',
    lineHeight: 26,
    letterSpacing: 0,
  },
  bodyMedium: {
    fontFamily: fonts.body,
    fontSize: 15,
    fontWeight: '400',
    lineHeight: 22,
    letterSpacing: 0,
  },
  bodySmall: {
    fontFamily: fonts.body,
    fontSize: 13,
    fontWeight: '400',
    lineHeight: 18,
    letterSpacing: 0,
  },

  // Label styles
  labelLarge: {
    fontFamily: fonts.body,
    fontSize: 15,
    fontWeight: '500',
    lineHeight: 20,
    letterSpacing: 0.1,
  },
  labelMedium: {
    fontFamily: fonts.body,
    fontSize: 13,
    fontWeight: '500',
    lineHeight: 18,
    letterSpacing: 0.25,
  },
  labelSmall: {
    fontFamily: fonts.body,
    fontSize: 11,
    fontWeight: '500',
    lineHeight: 16,
    letterSpacing: 0.5,
    textTransform: 'uppercase',
  },

  // Mono styles - for stats
  monoLarge: {
    fontFamily: fonts.mono,
    fontSize: 32,
    fontWeight: '600',
    lineHeight: 40,
    letterSpacing: -0.5,
  },
  monoMedium: {
    fontFamily: fonts.mono,
    fontSize: 20,
    fontWeight: '500',
    lineHeight: 28,
    letterSpacing: 0,
  },
  monoSmall: {
    fontFamily: fonts.mono,
    fontSize: 14,
    fontWeight: '400',
    lineHeight: 20,
    letterSpacing: 0,
  },
};

// Spacing scale
export const spacing = {
  xs: 4,
  sm: 8,
  md: 12,
  lg: 16,
  xl: 24,
  '2xl': 32,
  '3xl': 48,
  '4xl': 64,
} as const;

// Border radius
export const radius = {
  sm: 8,
  md: 12,
  lg: 16,
  xl: 24,
  full: 9999,
} as const;
