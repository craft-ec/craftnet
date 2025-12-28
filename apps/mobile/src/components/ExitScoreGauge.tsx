/**
 * ExitScoreGauge Component
 *
 * A circular gauge visualization for exit node scores
 * Score: 0-100 where lower is better
 * Colors: cyan (good) → amber (medium) → red (poor)
 */

import React from 'react';
import { View, Text, StyleSheet } from 'react-native';
import Svg, { Circle, Defs, LinearGradient, Stop } from 'react-native-svg';
import { palette, theme } from '../theme';
import { typography, spacing } from '../theme/typography';

interface ExitScoreGaugeProps {
  score: number; // 0-100, lower is better
  size?: 'small' | 'medium' | 'large';
  showLabel?: boolean;
}

const SIZES = {
  small: { diameter: 44, stroke: 4, fontSize: 12 },
  medium: { diameter: 64, stroke: 5, fontSize: 16 },
  large: { diameter: 88, stroke: 6, fontSize: 22 },
};

export function ExitScoreGauge({ score, size = 'medium', showLabel = true }: ExitScoreGaugeProps) {
  const config = SIZES[size];
  const radius = (config.diameter - config.stroke) / 2;
  const circumference = 2 * Math.PI * radius;

  // Invert score for visual (0 = full circle, 100 = empty)
  const normalizedScore = Math.max(0, Math.min(100, score));
  const fillPercentage = (100 - normalizedScore) / 100;
  const strokeDashoffset = circumference * (1 - fillPercentage);

  // Color based on score (lower = better = green/cyan)
  const getScoreColor = (s: number): string => {
    if (s <= 30) return palette.cyan[400];
    if (s <= 50) return palette.cyan[500];
    if (s <= 70) return palette.amber[400];
    return palette.error;
  };

  const getScoreLabel = (s: number): string => {
    if (s <= 25) return 'Excellent';
    if (s <= 40) return 'Good';
    if (s <= 55) return 'Fair';
    if (s <= 70) return 'Poor';
    return 'Bad';
  };

  const scoreColor = getScoreColor(normalizedScore);

  return (
    <View style={styles.container}>
      <View style={[styles.gaugeContainer, { width: config.diameter, height: config.diameter }]}>
        <Svg width={config.diameter} height={config.diameter}>
          <Defs>
            <LinearGradient id="scoreGradient" x1="0%" y1="0%" x2="100%" y2="100%">
              <Stop offset="0%" stopColor={scoreColor} stopOpacity={1} />
              <Stop offset="100%" stopColor={scoreColor} stopOpacity={0.6} />
            </LinearGradient>
          </Defs>

          {/* Background track */}
          <Circle
            cx={config.diameter / 2}
            cy={config.diameter / 2}
            r={radius}
            stroke={theme.background.elevated}
            strokeWidth={config.stroke}
            fill="transparent"
          />

          {/* Score arc */}
          <Circle
            cx={config.diameter / 2}
            cy={config.diameter / 2}
            r={radius}
            stroke="url(#scoreGradient)"
            strokeWidth={config.stroke}
            fill="transparent"
            strokeDasharray={circumference}
            strokeDashoffset={strokeDashoffset}
            strokeLinecap="round"
            transform={`rotate(-90 ${config.diameter / 2} ${config.diameter / 2})`}
          />
        </Svg>

        {/* Score number in center */}
        <View style={styles.scoreOverlay}>
          <Text style={[styles.scoreText, { fontSize: config.fontSize, color: scoreColor }]}>
            {normalizedScore}
          </Text>
        </View>
      </View>

      {showLabel && (
        <Text style={[styles.label, { color: scoreColor }]}>
          {getScoreLabel(normalizedScore)}
        </Text>
      )}
    </View>
  );
}

/**
 * Score breakdown bar for individual metrics
 */
interface ScoreBreakdownBarProps {
  label: string;
  value: number | string;
  percentage: number; // 0-100
  maxPercentage: number; // Maximum for this metric
  color?: string;
}

export function ScoreBreakdownBar({
  label,
  value,
  percentage,
  maxPercentage,
  color = palette.cyan[500],
}: ScoreBreakdownBarProps) {
  const fillWidth = (percentage / maxPercentage) * 100;

  return (
    <View style={styles.breakdownRow}>
      <View style={styles.breakdownLabelContainer}>
        <Text style={styles.breakdownLabel}>{label}</Text>
        <Text style={styles.breakdownValue}>{value}</Text>
      </View>
      <View style={styles.breakdownBarContainer}>
        <View style={styles.breakdownBarTrack}>
          <View
            style={[
              styles.breakdownBarFill,
              { width: `${Math.min(100, fillWidth)}%`, backgroundColor: color }
            ]}
          />
        </View>
        <Text style={styles.breakdownPoints}>{percentage.toFixed(0)}pts</Text>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    alignItems: 'center',
  },
  gaugeContainer: {
    position: 'relative',
  },
  scoreOverlay: {
    position: 'absolute',
    top: 0,
    left: 0,
    right: 0,
    bottom: 0,
    alignItems: 'center',
    justifyContent: 'center',
  },
  scoreText: {
    fontFamily: 'JetBrainsMono-Bold',
    fontWeight: '700',
  },
  label: {
    ...typography.labelSmall,
    marginTop: spacing.xs,
    fontWeight: '600',
  },

  // Breakdown bar styles
  breakdownRow: {
    marginBottom: spacing.md,
  },
  breakdownLabelContainer: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    marginBottom: spacing.xs,
  },
  breakdownLabel: {
    ...typography.bodySmall,
    color: theme.text.secondary,
  },
  breakdownValue: {
    ...typography.bodySmall,
    color: theme.text.primary,
    fontFamily: 'JetBrainsMono-Regular',
  },
  breakdownBarContainer: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing.sm,
  },
  breakdownBarTrack: {
    flex: 1,
    height: 6,
    backgroundColor: theme.background.elevated,
    borderRadius: 3,
    overflow: 'hidden',
  },
  breakdownBarFill: {
    height: '100%',
    borderRadius: 3,
  },
  breakdownPoints: {
    ...typography.labelSmall,
    color: theme.text.tertiary,
    width: 36,
    textAlign: 'right',
    fontFamily: 'JetBrainsMono-Regular',
  },
});
