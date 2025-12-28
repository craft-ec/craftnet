/**
 * Mode Selector Component
 *
 * A beautiful segmented control for switching between Client/Node/Both modes
 * with fluid animations and mode-specific colors
 */

import React, { useEffect, useRef } from 'react';
import {
  View,
  Text,
  Pressable,
  StyleSheet,
  Animated,
  Dimensions,
} from 'react-native';
import { NodeMode, modeColors, theme, palette } from '../theme';
import { typography, spacing, radius } from '../theme/typography';
import { useTunnel } from '../context/TunnelContext';

const { width: SCREEN_WIDTH } = Dimensions.get('window');
const SELECTOR_PADDING = spacing.xs;
const SELECTOR_WIDTH = SCREEN_WIDTH - spacing.xl * 2;
const SEGMENT_WIDTH = (SELECTOR_WIDTH - SELECTOR_PADDING * 2) / 3;

interface ModeOption {
  key: NodeMode;
  label: string;
  icon: string;
  description: string;
}

const modes: ModeOption[] = [
  {
    key: 'client',
    label: 'Client',
    icon: '🛡️',
    description: 'Use VPN',
  },
  {
    key: 'both',
    label: 'Both',
    icon: '⚡',
    description: 'VPN + Node',
  },
  {
    key: 'node',
    label: 'Node',
    icon: '🌐',
    description: 'Earn Credits',
  },
];

export function ModeSelector() {
  const { mode, setMode, isConnected } = useTunnel();
  const slideAnim = useRef(new Animated.Value(getModeIndex(mode) * SEGMENT_WIDTH)).current;
  const scaleAnim = useRef(new Animated.Value(1)).current;
  const colorAnim = useRef(new Animated.Value(0)).current;

  function getModeIndex(m: NodeMode): number {
    return modes.findIndex((opt) => opt.key === m);
  }

  useEffect(() => {
    const index = getModeIndex(mode);
    Animated.spring(slideAnim, {
      toValue: index * SEGMENT_WIDTH,
      useNativeDriver: true,
      tension: 60,
      friction: 10,
    }).start();
  }, [mode, slideAnim]);

  const handlePress = (newMode: NodeMode) => {
    // Bounce animation
    Animated.sequence([
      Animated.timing(scaleAnim, {
        toValue: 0.95,
        duration: 100,
        useNativeDriver: true,
      }),
      Animated.spring(scaleAnim, {
        toValue: 1,
        useNativeDriver: true,
        tension: 100,
        friction: 8,
      }),
    ]).start();

    setMode(newMode);
  };

  const currentColor = modeColors[mode];

  return (
    <View style={styles.container}>
      <View style={styles.labelRow}>
        <Text style={styles.sectionLabel}>Operating Mode</Text>
        <View style={[styles.statusBadge, isConnected && { backgroundColor: currentColor.primary + '20' }]}>
          <View style={[styles.statusDot, { backgroundColor: isConnected ? palette.success : palette.silver }]} />
          <Text style={[styles.statusText, isConnected && { color: currentColor.primary }]}>
            {isConnected ? 'Active' : 'Inactive'}
          </Text>
        </View>
      </View>

      <Animated.View style={[styles.selector, { transform: [{ scale: scaleAnim }] }]}>
        {/* Sliding indicator */}
        <Animated.View
          style={[
            styles.indicator,
            {
              width: SEGMENT_WIDTH,
              backgroundColor: currentColor.primary,
              transform: [{ translateX: slideAnim }],
              shadowColor: currentColor.primary,
            },
          ]}
        />

        {/* Mode buttons */}
        {modes.map((option) => {
          const isActive = mode === option.key;
          return (
            <Pressable
              key={option.key}
              style={styles.segment}
              onPress={() => handlePress(option.key)}
              android_ripple={{ color: 'rgba(255,255,255,0.1)', borderless: true }}
            >
              <Text style={styles.segmentIcon}>{option.icon}</Text>
              <Text
                style={[
                  styles.segmentLabel,
                  isActive && styles.segmentLabelActive,
                ]}
              >
                {option.label}
              </Text>
            </Pressable>
          );
        })}
      </Animated.View>

      {/* Mode description */}
      <View style={styles.descriptionContainer}>
        <Text style={styles.modeDescription}>
          {mode === 'client' && '🛡️ Route your traffic through the VPN network'}
          {mode === 'node' && '🌐 Help others by relaying traffic and earn credits'}
          {mode === 'both' && '⚡ Use VPN protection while earning from the network'}
        </Text>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    paddingHorizontal: spacing.xl,
    marginBottom: spacing.xl,
  },
  labelRow: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: spacing.md,
  },
  sectionLabel: {
    ...typography.labelSmall,
    color: theme.text.tertiary,
  },
  statusBadge: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: spacing.sm,
    paddingVertical: spacing.xs,
    borderRadius: radius.full,
    backgroundColor: theme.background.elevated,
  },
  statusDot: {
    width: 6,
    height: 6,
    borderRadius: 3,
    marginRight: spacing.xs,
  },
  statusText: {
    ...typography.labelSmall,
    color: theme.text.tertiary,
    textTransform: 'none',
  },
  selector: {
    flexDirection: 'row',
    backgroundColor: theme.background.tertiary,
    borderRadius: radius.lg,
    padding: SELECTOR_PADDING,
    position: 'relative',
  },
  indicator: {
    position: 'absolute',
    top: SELECTOR_PADDING,
    left: SELECTOR_PADDING,
    bottom: SELECTOR_PADDING,
    borderRadius: radius.md,
    shadowOffset: { width: 0, height: 4 },
    shadowOpacity: 0.3,
    shadowRadius: 8,
    elevation: 4,
  },
  segment: {
    width: SEGMENT_WIDTH,
    paddingVertical: spacing.md,
    alignItems: 'center',
    justifyContent: 'center',
    zIndex: 1,
  },
  segmentIcon: {
    fontSize: 20,
    marginBottom: spacing.xs,
  },
  segmentLabel: {
    ...typography.labelMedium,
    color: theme.text.tertiary,
  },
  segmentLabelActive: {
    color: theme.text.inverse,
    fontWeight: '600',
  },
  descriptionContainer: {
    marginTop: spacing.md,
    paddingHorizontal: spacing.sm,
  },
  modeDescription: {
    ...typography.bodySmall,
    color: theme.text.secondary,
    textAlign: 'center',
  },
});
