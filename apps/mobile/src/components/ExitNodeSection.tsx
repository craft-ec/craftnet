/**
 * ExitNodeSection Component
 *
 * Displays current exit node with scoring, and allows changing exits
 * Shows score breakdown: Load, Latency, Throughput, Uptime
 */

import React, { useState, useCallback } from 'react';
import {
  View,
  Text,
  TouchableOpacity,
  StyleSheet,
  Modal,
  FlatList,
  Pressable,
} from 'react-native';
import { useTunnel } from '../context/TunnelContext';
import { ExitScoreGauge, ScoreBreakdownBar } from './ExitScoreGauge';
import { palette, theme, modeColors } from '../theme';
import { typography, spacing } from '../theme/typography';

// Extended exit info with scoring
export interface ExitNodeInfo {
  id: string;
  pubkey: string;
  countryCode: string;
  countryName: string;
  city?: string;
  region: string;

  // Scoring (0-100, lower = better)
  score: number;
  loadPercent: number;
  latencyMs: number | null;
  uplinkKbps: number | null;
  downlinkKbps: number | null;
  uptimeSecs: number;

  // Trust
  isTrusted: boolean; // Announced vs measured match
}

// Country flag emoji helper
const getCountryFlag = (countryCode: string): string => {
  const codePoints = countryCode
    .toUpperCase()
    .split('')
    .map(char => 127397 + char.charCodeAt(0));
  return String.fromCodePoint(...codePoints);
};

// Format uptime
const formatUptime = (secs: number): string => {
  if (secs < 3600) return `${Math.floor(secs / 60)}m`;
  if (secs < 86400) return `${Math.floor(secs / 3600)}h`;
  return `${Math.floor(secs / 86400)}d`;
};

// Format throughput
const formatThroughput = (kbps: number | null): string => {
  if (kbps === null) return '—';
  if (kbps < 1000) return `${kbps} KB/s`;
  return `${(kbps / 1000).toFixed(1)} MB/s`;
};

interface ExitNodeSectionProps {
  currentExit: ExitNodeInfo | null;
  availableExits: ExitNodeInfo[];
  onChangeExit: (exit: ExitNodeInfo) => void;
}

export function ExitNodeSection({
  currentExit,
  availableExits,
  onChangeExit,
}: ExitNodeSectionProps) {
  const { mode, isConnected } = useTunnel();
  const [showExitList, setShowExitList] = useState(false);
  const colors = modeColors[mode];

  const showClient = mode === 'client' || mode === 'both';

  if (!showClient) {
    return null; // Don't show exit selection in Node-only mode
  }

  const handleSelectExit = useCallback((exit: ExitNodeInfo) => {
    onChangeExit(exit);
    setShowExitList(false);
  }, [onChangeExit]);

  return (
    <View style={styles.container}>
      {/* Current Exit Display */}
      {currentExit ? (
        <View style={styles.currentExit}>
          <View style={styles.exitHeader}>
            <View style={styles.exitInfo}>
              <Text style={styles.flag}>{getCountryFlag(currentExit.countryCode)}</Text>
              <View style={styles.exitDetails}>
                <Text style={styles.exitName}>
                  {currentExit.city || currentExit.countryName}
                </Text>
                <Text style={styles.exitLocation}>
                  {currentExit.countryName} • {currentExit.region.toUpperCase()}
                </Text>
              </View>
            </View>

            <ExitScoreGauge score={currentExit.score} size="medium" />
          </View>

          {/* Score Breakdown */}
          <View style={styles.breakdown}>
            <Text style={styles.breakdownTitle}>Score Breakdown</Text>

            <ScoreBreakdownBar
              label="Load"
              value={`${currentExit.loadPercent}%`}
              percentage={(currentExit.loadPercent / 100) * 15}
              maxPercentage={15}
              color={palette.cyan[400]}
            />

            <ScoreBreakdownBar
              label="Latency"
              value={currentExit.latencyMs ? `${currentExit.latencyMs}ms` : '—'}
              percentage={currentExit.latencyMs ? Math.min(currentExit.latencyMs / 500, 1) * 25 : 12.5}
              maxPercentage={25}
              color={palette.cyan[500]}
            />

            <ScoreBreakdownBar
              label="Throughput"
              value={formatThroughput(currentExit.downlinkKbps)}
              percentage={currentExit.downlinkKbps
                ? 40 - Math.min(currentExit.downlinkKbps / 50000, 1) * 40
                : 20
              }
              maxPercentage={40}
              color={palette.amber[400]}
            />

            <ScoreBreakdownBar
              label="Uptime"
              value={formatUptime(currentExit.uptimeSecs)}
              percentage={20 - Math.min(currentExit.uptimeSecs / 86400, 1) * 20}
              maxPercentage={20}
              color={palette.violet[400]}
            />

            {!currentExit.isTrusted && (
              <View style={styles.trustWarning}>
                <Text style={styles.trustWarningIcon}>⚠</Text>
                <Text style={styles.trustWarningText}>
                  Trust penalty applied (announced &gt; 3x measured)
                </Text>
              </View>
            )}
          </View>

          {/* Change Exit Button */}
          <TouchableOpacity
            style={[styles.changeButton, { borderColor: colors.primary + '40' }]}
            onPress={() => setShowExitList(true)}
            activeOpacity={0.7}
          >
            <Text style={[styles.changeButtonText, { color: colors.primary }]}>
              Change Exit
            </Text>
          </TouchableOpacity>
        </View>
      ) : (
        <View style={styles.noExit}>
          <Text style={styles.noExitText}>No exit selected</Text>
          <TouchableOpacity
            style={[styles.selectButton, { backgroundColor: colors.primary }]}
            onPress={() => setShowExitList(true)}
            activeOpacity={0.8}
          >
            <Text style={styles.selectButtonText}>Select Exit</Text>
          </TouchableOpacity>
        </View>
      )}

      {/* Exit Selection Modal */}
      <Modal
        visible={showExitList}
        animationType="slide"
        presentationStyle="pageSheet"
        onRequestClose={() => setShowExitList(false)}
      >
        <View style={styles.modalContainer}>
          <View style={styles.modalHeader}>
            <Text style={styles.modalTitle}>Select Exit Node</Text>
            <TouchableOpacity onPress={() => setShowExitList(false)}>
              <Text style={styles.modalClose}>Done</Text>
            </TouchableOpacity>
          </View>

          <FlatList
            data={availableExits.sort((a, b) => a.score - b.score)}
            keyExtractor={(item) => item.id}
            contentContainerStyle={styles.exitList}
            renderItem={({ item }) => (
              <Pressable
                style={({ pressed }) => [
                  styles.exitItem,
                  pressed && styles.exitItemPressed,
                  currentExit?.id === item.id && styles.exitItemSelected,
                ]}
                onPress={() => handleSelectExit(item)}
              >
                <Text style={styles.exitItemFlag}>{getCountryFlag(item.countryCode)}</Text>

                <View style={styles.exitItemInfo}>
                  <Text style={styles.exitItemName}>
                    {item.city || item.countryName}
                  </Text>
                  <Text style={styles.exitItemMeta}>
                    {item.countryName} • {item.latencyMs ? `${item.latencyMs}ms` : '—'} • {formatUptime(item.uptimeSecs)}
                  </Text>
                </View>

                <View style={styles.exitItemScore}>
                  <ExitScoreGauge score={item.score} size="small" showLabel={false} />
                </View>
              </Pressable>
            )}
          />
        </View>
      </Modal>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {},

  currentExit: {},

  exitHeader: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    marginBottom: spacing.lg,
  },

  exitInfo: {
    flexDirection: 'row',
    alignItems: 'center',
    flex: 1,
  },

  flag: {
    fontSize: 32,
    marginRight: spacing.md,
  },

  exitDetails: {
    flex: 1,
  },

  exitName: {
    ...typography.headingSmall,
    color: theme.text.primary,
  },

  exitLocation: {
    ...typography.bodySmall,
    color: theme.text.tertiary,
    marginTop: 2,
  },

  breakdown: {
    backgroundColor: theme.background.tertiary,
    borderRadius: 12,
    padding: spacing.lg,
    marginBottom: spacing.lg,
  },

  breakdownTitle: {
    ...typography.labelSmall,
    color: theme.text.tertiary,
    marginBottom: spacing.md,
    textTransform: 'uppercase',
    letterSpacing: 0.5,
  },

  trustWarning: {
    flexDirection: 'row',
    alignItems: 'center',
    backgroundColor: palette.error + '15',
    padding: spacing.sm,
    borderRadius: 8,
    marginTop: spacing.sm,
  },

  trustWarningIcon: {
    fontSize: 14,
    marginRight: spacing.xs,
  },

  trustWarningText: {
    ...typography.bodySmall,
    color: palette.error,
    flex: 1,
  },

  changeButton: {
    borderWidth: 1,
    borderRadius: 12,
    paddingVertical: spacing.md,
    alignItems: 'center',
  },

  changeButtonText: {
    ...typography.bodyMedium,
    fontWeight: '600',
  },

  noExit: {
    alignItems: 'center',
    paddingVertical: spacing.xl,
  },

  noExitText: {
    ...typography.bodyMedium,
    color: theme.text.tertiary,
    marginBottom: spacing.md,
  },

  selectButton: {
    paddingHorizontal: spacing.xl,
    paddingVertical: spacing.md,
    borderRadius: 12,
  },

  selectButtonText: {
    ...typography.bodyMedium,
    color: theme.text.inverse,
    fontWeight: '600',
  },

  // Modal styles
  modalContainer: {
    flex: 1,
    backgroundColor: theme.background.primary,
  },

  modalHeader: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    paddingHorizontal: spacing.lg,
    paddingVertical: spacing.lg,
    borderBottomWidth: 1,
    borderBottomColor: theme.border.subtle,
  },

  modalTitle: {
    ...typography.headingSmall,
    color: theme.text.primary,
  },

  modalClose: {
    ...typography.bodyMedium,
    color: palette.cyan[500],
    fontWeight: '600',
  },

  exitList: {
    padding: spacing.lg,
  },

  exitItem: {
    flexDirection: 'row',
    alignItems: 'center',
    backgroundColor: theme.background.secondary,
    borderRadius: 12,
    padding: spacing.lg,
    marginBottom: spacing.sm,
    borderWidth: 1,
    borderColor: theme.border.subtle,
  },

  exitItemPressed: {
    backgroundColor: theme.background.tertiary,
  },

  exitItemSelected: {
    borderColor: palette.cyan[500],
    backgroundColor: palette.cyan[500] + '10',
  },

  exitItemFlag: {
    fontSize: 28,
    marginRight: spacing.md,
  },

  exitItemInfo: {
    flex: 1,
  },

  exitItemName: {
    ...typography.bodyLarge,
    color: theme.text.primary,
    fontWeight: '600',
  },

  exitItemMeta: {
    ...typography.bodySmall,
    color: theme.text.tertiary,
    marginTop: 2,
  },

  exitItemScore: {
    marginLeft: spacing.md,
  },
});
