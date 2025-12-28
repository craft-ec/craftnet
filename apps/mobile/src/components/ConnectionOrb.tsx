/**
 * Connection Orb Component
 *
 * The main focal point - a large animated orb that visualizes
 * connection state with pulsing effects and mode-specific colors
 */

import React, { useEffect, useRef } from 'react';
import {
  View,
  Text,
  Pressable,
  StyleSheet,
  Animated,
  Easing,
} from 'react-native';
import { modeColors, theme, palette } from '../theme';
import { typography, spacing, radius } from '../theme/typography';
import { useTunnel } from '../context/TunnelContext';

const ORB_SIZE = 200;
const RING_COUNT = 3;

export function ConnectionOrb() {
  const { connectionState, isConnected, mode, toggleConnection, stats } = useTunnel();
  const colors = modeColors[mode];

  // Animations
  const pulseAnim = useRef(new Animated.Value(1)).current;
  const rotateAnim = useRef(new Animated.Value(0)).current;
  const glowAnim = useRef(new Animated.Value(0)).current;
  const ringAnims = useRef(
    Array(RING_COUNT)
      .fill(0)
      .map(() => new Animated.Value(0))
  ).current;

  // Pulse animation when connected
  useEffect(() => {
    if (isConnected) {
      Animated.loop(
        Animated.sequence([
          Animated.timing(pulseAnim, {
            toValue: 1.05,
            duration: 2000,
            easing: Easing.inOut(Easing.ease),
            useNativeDriver: true,
          }),
          Animated.timing(pulseAnim, {
            toValue: 1,
            duration: 2000,
            easing: Easing.inOut(Easing.ease),
            useNativeDriver: true,
          }),
        ])
      ).start();

      // Glow animation
      Animated.loop(
        Animated.sequence([
          Animated.timing(glowAnim, {
            toValue: 1,
            duration: 1500,
            easing: Easing.inOut(Easing.ease),
            useNativeDriver: true,
          }),
          Animated.timing(glowAnim, {
            toValue: 0.6,
            duration: 1500,
            easing: Easing.inOut(Easing.ease),
            useNativeDriver: true,
          }),
        ])
      ).start();
    } else {
      pulseAnim.setValue(1);
      glowAnim.setValue(0);
    }
  }, [isConnected, pulseAnim, glowAnim]);

  // Rotating ring animation when connecting
  useEffect(() => {
    if (connectionState === 'connecting' || connectionState === 'disconnecting') {
      Animated.loop(
        Animated.timing(rotateAnim, {
          toValue: 1,
          duration: 2000,
          easing: Easing.linear,
          useNativeDriver: true,
        })
      ).start();
    } else {
      rotateAnim.setValue(0);
    }
  }, [connectionState, rotateAnim]);

  // Expanding rings animation when connected
  useEffect(() => {
    if (isConnected) {
      ringAnims.forEach((anim, index) => {
        Animated.loop(
          Animated.sequence([
            Animated.delay(index * 800),
            Animated.timing(anim, {
              toValue: 1,
              duration: 2400,
              easing: Easing.out(Easing.ease),
              useNativeDriver: true,
            }),
            Animated.timing(anim, {
              toValue: 0,
              duration: 0,
              useNativeDriver: true,
            }),
          ])
        ).start();
      });
    } else {
      ringAnims.forEach((anim) => anim.setValue(0));
    }
  }, [isConnected, ringAnims]);

  const rotation = rotateAnim.interpolate({
    inputRange: [0, 1],
    outputRange: ['0deg', '360deg'],
  });

  const getStatusText = () => {
    switch (connectionState) {
      case 'connecting':
        return 'Connecting...';
      case 'disconnecting':
        return 'Disconnecting...';
      case 'connected':
        return 'Protected';
      case 'error':
        return 'Error';
      default:
        return 'Tap to Connect';
    }
  };

  const getStatusIcon = () => {
    switch (connectionState) {
      case 'connecting':
      case 'disconnecting':
        return '⏳';
      case 'connected':
        return mode === 'node' ? '🌐' : '🛡️';
      case 'error':
        return '⚠️';
      default:
        return '🔌';
    }
  };

  return (
    <View style={styles.container}>
      {/* Expanding rings */}
      {isConnected &&
        ringAnims.map((anim, index) => (
          <Animated.View
            key={index}
            style={[
              styles.ring,
              {
                borderColor: colors.primary,
                opacity: anim.interpolate({
                  inputRange: [0, 0.5, 1],
                  outputRange: [0.6, 0.3, 0],
                }),
                transform: [
                  {
                    scale: anim.interpolate({
                      inputRange: [0, 1],
                      outputRange: [1, 1.8],
                    }),
                  },
                ],
              },
            ]}
          />
        ))}

      {/* Rotating ring when connecting */}
      {(connectionState === 'connecting' || connectionState === 'disconnecting') && (
        <Animated.View
          style={[
            styles.connectingRing,
            {
              borderColor: colors.primary,
              transform: [{ rotate: rotation }],
            },
          ]}
        />
      )}

      {/* Main orb */}
      <Pressable onPress={toggleConnection}>
        <Animated.View
          style={[
            styles.orb,
            {
              backgroundColor: isConnected ? colors.primary : theme.background.elevated,
              borderColor: isConnected ? colors.primary : theme.border.strong,
              transform: [{ scale: pulseAnim }],
              shadowColor: isConnected ? colors.primary : 'transparent',
              shadowOpacity: isConnected ? 0.5 : 0,
            },
          ]}
        >
          {/* Inner glow */}
          {isConnected && (
            <Animated.View
              style={[
                styles.innerGlow,
                {
                  backgroundColor: colors.primaryLight,
                  opacity: glowAnim.interpolate({
                    inputRange: [0, 1],
                    outputRange: [0.1, 0.25],
                  }),
                },
              ]}
            />
          )}

          {/* Content */}
          <View style={styles.orbContent}>
            <Text style={styles.statusIcon}>{getStatusIcon()}</Text>
            <Text
              style={[
                styles.statusText,
                { color: isConnected ? theme.text.inverse : theme.text.primary },
              ]}
            >
              {getStatusText()}
            </Text>
            {isConnected && (
              <Text style={[styles.peersText, { color: theme.text.inverse }]}>
                {stats.connectedPeers} peers
              </Text>
            )}
          </View>
        </Animated.View>
      </Pressable>

      {/* Connection hint */}
      {!isConnected && connectionState === 'disconnected' && (
        <Text style={styles.hint}>Tap the orb to connect</Text>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    alignItems: 'center',
    justifyContent: 'center',
    height: ORB_SIZE + 100,
    marginVertical: spacing['2xl'],
  },
  ring: {
    position: 'absolute',
    width: ORB_SIZE,
    height: ORB_SIZE,
    borderRadius: ORB_SIZE / 2,
    borderWidth: 2,
  },
  connectingRing: {
    position: 'absolute',
    width: ORB_SIZE + 20,
    height: ORB_SIZE + 20,
    borderRadius: (ORB_SIZE + 20) / 2,
    borderWidth: 3,
    borderStyle: 'dashed',
  },
  orb: {
    width: ORB_SIZE,
    height: ORB_SIZE,
    borderRadius: ORB_SIZE / 2,
    borderWidth: 2,
    alignItems: 'center',
    justifyContent: 'center',
    shadowOffset: { width: 0, height: 0 },
    shadowRadius: 30,
    elevation: 10,
    overflow: 'hidden',
  },
  innerGlow: {
    position: 'absolute',
    top: 0,
    left: 0,
    right: 0,
    bottom: 0,
    borderRadius: ORB_SIZE / 2,
  },
  orbContent: {
    alignItems: 'center',
    justifyContent: 'center',
  },
  statusIcon: {
    fontSize: 40,
    marginBottom: spacing.sm,
  },
  statusText: {
    ...typography.labelLarge,
    fontWeight: '600',
  },
  peersText: {
    ...typography.labelSmall,
    marginTop: spacing.xs,
    opacity: 0.8,
    textTransform: 'none',
  },
  hint: {
    ...typography.bodySmall,
    color: theme.text.tertiary,
    marginTop: spacing.lg,
  },
});
