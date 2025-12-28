/**
 * CollapsibleSection Component
 *
 * A refined, animated collapsible container for organizing UI sections
 * Uses smooth height animations and subtle visual cues
 */

import React, { useState, useCallback, useRef, useEffect } from 'react';
import {
  View,
  Text,
  TouchableOpacity,
  StyleSheet,
  Animated,
  LayoutAnimation,
  Platform,
  UIManager,
} from 'react-native';
import { theme } from '../theme';
import { typography, spacing } from '../theme/typography';

// Enable LayoutAnimation on Android
if (Platform.OS === 'android' && UIManager.setLayoutAnimationEnabledExperimental) {
  UIManager.setLayoutAnimationEnabledExperimental(true);
}

interface CollapsibleSectionProps {
  title: string;
  subtitle?: string;
  icon?: React.ReactNode;
  badge?: string | number;
  badgeColor?: string;
  children: React.ReactNode;
  defaultExpanded?: boolean;
  onToggle?: (expanded: boolean) => void;
}

export function CollapsibleSection({
  title,
  subtitle,
  icon,
  badge,
  badgeColor,
  children,
  defaultExpanded = false,
  onToggle,
}: CollapsibleSectionProps) {
  const [expanded, setExpanded] = useState(defaultExpanded);
  const rotateAnim = useRef(new Animated.Value(defaultExpanded ? 1 : 0)).current;
  const opacityAnim = useRef(new Animated.Value(defaultExpanded ? 1 : 0)).current;

  useEffect(() => {
    Animated.parallel([
      Animated.timing(rotateAnim, {
        toValue: expanded ? 1 : 0,
        duration: 200,
        useNativeDriver: true,
      }),
      Animated.timing(opacityAnim, {
        toValue: expanded ? 1 : 0,
        duration: 150,
        useNativeDriver: true,
      }),
    ]).start();
  }, [expanded]);

  const handleToggle = useCallback(() => {
    LayoutAnimation.configureNext({
      duration: 200,
      update: { type: 'easeInEaseOut' },
      create: { type: 'easeInEaseOut', property: 'opacity' },
      delete: { type: 'easeInEaseOut', property: 'opacity' },
    });
    setExpanded(!expanded);
    onToggle?.(!expanded);
  }, [expanded, onToggle]);

  const chevronRotation = rotateAnim.interpolate({
    inputRange: [0, 1],
    outputRange: ['0deg', '90deg'],
  });

  return (
    <View style={styles.container}>
      <TouchableOpacity
        style={styles.header}
        onPress={handleToggle}
        activeOpacity={0.7}
      >
        <View style={styles.headerLeft}>
          {icon && <View style={styles.iconContainer}>{icon}</View>}
          <View style={styles.titleContainer}>
            <Text style={styles.title}>{title}</Text>
            {subtitle && <Text style={styles.subtitle}>{subtitle}</Text>}
          </View>
        </View>

        <View style={styles.headerRight}>
          {badge !== undefined && (
            <View style={[styles.badge, badgeColor ? { backgroundColor: badgeColor + '20' } : null]}>
              <Text style={[styles.badgeText, badgeColor ? { color: badgeColor } : null]}>
                {badge}
              </Text>
            </View>
          )}
          <Animated.View style={{ transform: [{ rotate: chevronRotation }] }}>
            <Text style={styles.chevron}>›</Text>
          </Animated.View>
        </View>
      </TouchableOpacity>

      {expanded && (
        <Animated.View style={[styles.content, { opacity: opacityAnim }]}>
          {children}
        </Animated.View>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    backgroundColor: theme.background.secondary,
    borderRadius: 16,
    marginHorizontal: spacing.lg,
    marginBottom: spacing.md,
    overflow: 'hidden',
    borderWidth: 1,
    borderColor: theme.border.subtle,
  },
  header: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    paddingVertical: spacing.lg,
    paddingHorizontal: spacing.lg,
  },
  headerLeft: {
    flexDirection: 'row',
    alignItems: 'center',
    flex: 1,
  },
  iconContainer: {
    width: 36,
    height: 36,
    borderRadius: 10,
    backgroundColor: theme.background.tertiary,
    alignItems: 'center',
    justifyContent: 'center',
    marginRight: spacing.md,
  },
  titleContainer: {
    flex: 1,
  },
  title: {
    ...typography.bodyLarge,
    color: theme.text.primary,
    fontWeight: '600',
  },
  subtitle: {
    ...typography.bodySmall,
    color: theme.text.tertiary,
    marginTop: 2,
  },
  headerRight: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing.sm,
  },
  badge: {
    paddingHorizontal: spacing.sm,
    paddingVertical: spacing.xs,
    borderRadius: 8,
    backgroundColor: theme.background.tertiary,
  },
  badgeText: {
    ...typography.labelSmall,
    color: theme.text.secondary,
    fontWeight: '600',
  },
  chevron: {
    fontSize: 24,
    color: theme.text.tertiary,
    fontWeight: '300',
  },
  content: {
    paddingHorizontal: spacing.lg,
    paddingBottom: spacing.lg,
    paddingTop: spacing.xs,
  },
});
