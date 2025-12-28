/**
 * Privacy Level Selector Component
 *
 * Compact dropdown for selecting privacy level (hop count)
 */

import React, { useState } from 'react';
import {
  View,
  Text,
  Pressable,
  StyleSheet,
  Modal,
} from 'react-native';
import { modeColors, theme, palette } from '../theme';
import { typography, spacing, radius } from '../theme/typography';
import { useTunnel, PrivacyLevel } from '../context/TunnelContext';

interface PrivacyOption {
  key: PrivacyLevel;
  label: string;
  hops: number;
  icon: string;
  description: string;
}

const options: PrivacyOption[] = [
  {
    key: 'direct',
    label: 'Direct',
    hops: 0,
    icon: '⚡',
    description: 'Fastest • Basic privacy',
  },
  {
    key: 'light',
    label: 'Light',
    hops: 1,
    icon: '🔒',
    description: '1 hop • Good privacy',
  },
  {
    key: 'standard',
    label: 'Standard',
    hops: 2,
    icon: '🛡️',
    description: '2 hops • Strong privacy',
  },
  {
    key: 'paranoid',
    label: 'Paranoid',
    hops: 3,
    icon: '🔐',
    description: '3 hops • Maximum privacy',
  },
];

function HopDots({ count, color }: { count: number; color: string }) {
  return (
    <View style={styles.hopDots}>
      {[1, 2, 3].map((i) => (
        <View
          key={i}
          style={[
            styles.hopDot,
            { backgroundColor: i <= count ? color : theme.background.elevated },
          ]}
        />
      ))}
    </View>
  );
}

export function PrivacySelector() {
  const { mode, privacyLevel, setPrivacyLevel } = useTunnel();
  const [isOpen, setIsOpen] = useState(false);
  const colors = modeColors[mode];

  // Only show for client/both modes
  if (mode === 'node') {
    return null;
  }

  const currentOption = options.find((o) => o.key === privacyLevel) || options[2];

  const handleSelect = (key: PrivacyLevel) => {
    setPrivacyLevel(key);
    setIsOpen(false);
  };

  return (
    <View style={styles.container}>
      {/* Dropdown trigger */}
      <Pressable
        style={[styles.trigger, isOpen && { borderColor: colors.primary }]}
        onPress={() => setIsOpen(true)}
      >
        <View style={styles.triggerLeft}>
          <Text style={styles.triggerIcon}>{currentOption.icon}</Text>
          <View>
            <Text style={styles.triggerLabel}>Privacy Level</Text>
            <Text style={[styles.triggerValue, { color: colors.primary }]}>
              {currentOption.label}
            </Text>
          </View>
        </View>
        <View style={styles.triggerRight}>
          <HopDots count={currentOption.hops} color={colors.primary} />
          <Text style={styles.chevron}>▼</Text>
        </View>
      </Pressable>

      {/* Dropdown modal */}
      <Modal
        visible={isOpen}
        transparent
        animationType="fade"
        onRequestClose={() => setIsOpen(false)}
      >
        <Pressable style={styles.overlay} onPress={() => setIsOpen(false)}>
          <View style={styles.dropdown}>
            <Text style={styles.dropdownTitle}>Select Privacy Level</Text>

            {options.map((option) => {
              const isActive = privacyLevel === option.key;
              return (
                <Pressable
                  key={option.key}
                  style={[
                    styles.option,
                    isActive && { backgroundColor: colors.primary + '15' },
                  ]}
                  onPress={() => handleSelect(option.key)}
                >
                  <View style={styles.optionLeft}>
                    <Text style={styles.optionIcon}>{option.icon}</Text>
                    <View>
                      <Text style={[styles.optionLabel, isActive && { color: colors.primary }]}>
                        {option.label}
                      </Text>
                      <Text style={styles.optionDesc}>{option.description}</Text>
                    </View>
                  </View>
                  <View style={styles.optionRight}>
                    <HopDots count={option.hops} color={isActive ? colors.primary : palette.silver} />
                    {isActive && (
                      <Text style={[styles.checkmark, { color: colors.primary }]}>✓</Text>
                    )}
                  </View>
                </Pressable>
              );
            })}
          </View>
        </Pressable>
      </Modal>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    paddingHorizontal: spacing.xl,
    marginBottom: spacing.xl,
  },
  trigger: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    backgroundColor: theme.background.tertiary,
    borderRadius: radius.lg,
    padding: spacing.lg,
    borderWidth: 2,
    borderColor: 'transparent',
  },
  triggerLeft: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing.md,
  },
  triggerIcon: {
    fontSize: 24,
  },
  triggerLabel: {
    ...typography.labelSmall,
    color: theme.text.tertiary,
    textTransform: 'none',
  },
  triggerValue: {
    ...typography.headingSmall,
    marginTop: 2,
  },
  triggerRight: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing.md,
  },
  chevron: {
    fontSize: 12,
    color: theme.text.tertiary,
  },
  hopDots: {
    flexDirection: 'row',
    gap: 4,
  },
  hopDot: {
    width: 16,
    height: 4,
    borderRadius: 2,
  },
  overlay: {
    flex: 1,
    backgroundColor: 'rgba(0, 0, 0, 0.7)',
    justifyContent: 'center',
    padding: spacing.xl,
  },
  dropdown: {
    backgroundColor: theme.background.secondary,
    borderRadius: radius.xl,
    padding: spacing.lg,
  },
  dropdownTitle: {
    ...typography.headingSmall,
    color: theme.text.primary,
    textAlign: 'center',
    marginBottom: spacing.lg,
  },
  option: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    padding: spacing.lg,
    borderRadius: radius.lg,
    marginBottom: spacing.sm,
  },
  optionLeft: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing.md,
    flex: 1,
  },
  optionIcon: {
    fontSize: 24,
  },
  optionLabel: {
    ...typography.labelLarge,
    color: theme.text.primary,
  },
  optionDesc: {
    ...typography.bodySmall,
    color: theme.text.tertiary,
    marginTop: 2,
  },
  optionRight: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing.md,
  },
  checkmark: {
    fontSize: 18,
    fontWeight: '600',
  },
});
