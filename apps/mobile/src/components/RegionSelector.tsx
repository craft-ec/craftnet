/**
 * Region Selector Component
 *
 * - Client mode: Select exit region (flexible) or country (strict)
 * - Node mode: Shows auto-detected location (read-only)
 * - Both mode: Shows both detected location AND exit selector
 */

import React, { useState, useMemo } from 'react';
import {
  View,
  Text,
  Pressable,
  StyleSheet,
  Modal,
  ScrollView,
  ActivityIndicator,
} from 'react-native';
import { modeColors, theme, palette } from '../theme';
import { typography, spacing, radius } from '../theme/typography';
import {
  useTunnel,
  ExitRegion,
  ExitSelection,
  AvailableExit,
} from '../context/TunnelContext';

interface RegionOption {
  key: ExitRegion;
  label: string;
  flag: string;
}

const regions: RegionOption[] = [
  { key: 'auto', label: 'Auto (Best)', flag: '🌍' },
  { key: 'na', label: 'North America', flag: '🇺🇸' },
  { key: 'eu', label: 'Europe', flag: '🇪🇺' },
  { key: 'ap', label: 'Asia Pacific', flag: '🇯🇵' },
  { key: 'oc', label: 'Oceania', flag: '🇦🇺' },
  { key: 'sa', label: 'South America', flag: '🇧🇷' },
  { key: 'me', label: 'Middle East', flag: '🇦🇪' },
  { key: 'af', label: 'Africa', flag: '🇿🇦' },
];

// Country code to flag emoji mapping
const countryFlags: Record<string, string> = {
  US: '🇺🇸', CA: '🇨🇦', MX: '🇲🇽', GB: '🇬🇧', DE: '🇩🇪', FR: '🇫🇷',
  IT: '🇮🇹', ES: '🇪🇸', NL: '🇳🇱', JP: '🇯🇵', KR: '🇰🇷', CN: '🇨🇳',
  SG: '🇸🇬', AU: '🇦🇺', NZ: '🇳🇿', BR: '🇧🇷', AR: '🇦🇷', AE: '🇦🇪',
  SA: '🇸🇦', IL: '🇮🇱', ZA: '🇿🇦', EG: '🇪🇬', IN: '🇮🇳', TH: '🇹🇭',
  VN: '🇻🇳', PH: '🇵🇭', ID: '🇮🇩', MY: '🇲🇾', HK: '🇭🇰', TW: '🇹🇼',
  CH: '🇨🇭', AT: '🇦🇹', SE: '🇸🇪', NO: '🇳🇴', DK: '🇩🇰', FI: '🇫🇮',
  PL: '🇵🇱', CZ: '🇨🇿', PT: '🇵🇹', IE: '🇮🇪', BE: '🇧🇪', GR: '🇬🇷',
};

// Detected location display component (for Node/Both modes)
function DetectedLocationCard() {
  const { mode, detectedLocation, isDetectingLocation } = useTunnel();
  const colors = modeColors[mode];

  return (
    <View style={styles.card}>
      <View style={styles.cardLeft}>
        {isDetectingLocation ? (
          <ActivityIndicator size="small" color={colors.primary} />
        ) : (
          <Text style={styles.cardIcon}>
            {detectedLocation ? (countryFlags[detectedLocation.countryCode] || '🌍') : '🌍'}
          </Text>
        )}
        <View>
          <Text style={styles.cardLabel}>Your Location</Text>
          <Text style={[styles.cardValue, { color: colors.primary }]}>
            {isDetectingLocation
              ? 'Detecting...'
              : detectedLocation
                ? detectedLocation.countryName
                : 'Unknown'}
          </Text>
        </View>
      </View>
      <View style={styles.cardRight}>
        <View style={styles.locationDetails}>
          <Text style={styles.cardDesc}>
            {detectedLocation?.city || 'Auto-detected'}
          </Text>
          {detectedLocation?.isp && (
            <Text style={styles.ispText} numberOfLines={1}>
              {detectedLocation.isp}
            </Text>
          )}
        </View>
        <Text style={[styles.badge, { backgroundColor: colors.primary + '20', color: colors.primary }]}>
          NODE
        </Text>
      </View>
    </View>
  );
}

// Exit region/country selector component
function ExitSelector() {
  const { mode, exitSelection, setExitSelection, availableExits } = useTunnel();
  const [isOpen, setIsOpen] = useState(false);
  const [viewMode, setViewMode] = useState<'regions' | 'countries'>('regions');
  const [expandedRegion, setExpandedRegion] = useState<ExitRegion | null>(null);
  const colors = modeColors[mode];

  // Group exits by country
  const exitsByCountry = useMemo(() => {
    const grouped: Record<string, { exits: AvailableExit[]; region: ExitRegion }> = {};
    availableExits.forEach((exit) => {
      if (!grouped[exit.countryCode]) {
        grouped[exit.countryCode] = { exits: [], region: exit.region };
      }
      grouped[exit.countryCode].exits.push(exit);
    });
    return grouped;
  }, [availableExits]);

  // Count exits per region
  const exitsPerRegion = useMemo(() => {
    const counts: Record<ExitRegion, number> = {
      auto: availableExits.length,
      na: 0, eu: 0, ap: 0, oc: 0, sa: 0, me: 0, af: 0,
    };
    availableExits.forEach((exit) => {
      counts[exit.region]++;
    });
    return counts;
  }, [availableExits]);

  // Get countries for a region
  const getCountriesInRegion = (region: ExitRegion) => {
    const countries: Record<string, { name: string; count: number; bestLatency: number }> = {};
    availableExits
      .filter((exit) => region === 'auto' || exit.region === region)
      .forEach((exit) => {
        if (!countries[exit.countryCode]) {
          countries[exit.countryCode] = {
            name: exit.countryName,
            count: 0,
            bestLatency: exit.latencyMs,
          };
        }
        countries[exit.countryCode].count++;
        countries[exit.countryCode].bestLatency = Math.min(
          countries[exit.countryCode].bestLatency,
          exit.latencyMs
        );
      });
    return Object.entries(countries).sort((a, b) => a[1].bestLatency - b[1].bestLatency);
  };

  // Get current selection display
  const getSelectionDisplay = () => {
    if (exitSelection.type === 'country' && exitSelection.countryCode) {
      const country = exitsByCountry[exitSelection.countryCode];
      return {
        flag: countryFlags[exitSelection.countryCode] || '🌍',
        label: country?.exits[0]?.countryName || exitSelection.countryCode,
        desc: `${country?.exits.length || 0} nodes (strict)`,
      };
    }
    const region = regions.find((r) => r.key === exitSelection.region) || regions[0];
    return {
      flag: region.flag,
      label: region.label,
      desc: `${exitsPerRegion[exitSelection.region]} nodes (flexible)`,
    };
  };

  const display = getSelectionDisplay();

  const handleSelectRegion = (region: ExitRegion) => {
    setExitSelection({ type: 'region', region });
    setIsOpen(false);
    setExpandedRegion(null);
  };

  const handleSelectCountry = (countryCode: string, region: ExitRegion) => {
    setExitSelection({ type: 'country', region, countryCode });
    setIsOpen(false);
    setExpandedRegion(null);
  };

  return (
    <>
      <Pressable
        style={[styles.card, isOpen && { borderColor: colors.primary }]}
        onPress={() => setIsOpen(true)}
      >
        <View style={styles.cardLeft}>
          <Text style={styles.cardIcon}>{display.flag}</Text>
          <View>
            <Text style={styles.cardLabel}>Exit Location</Text>
            <Text style={[styles.cardValue, { color: colors.primary }]}>
              {display.label}
            </Text>
          </View>
        </View>
        <View style={styles.cardRight}>
          <Text style={styles.cardDesc}>{display.desc}</Text>
          <Text style={styles.chevron}>▼</Text>
        </View>
      </Pressable>

      <Modal
        visible={isOpen}
        transparent
        animationType="fade"
        onRequestClose={() => { setIsOpen(false); setExpandedRegion(null); }}
      >
        <Pressable
          style={styles.overlay}
          onPress={() => { setIsOpen(false); setExpandedRegion(null); }}
        >
          <Pressable style={styles.dropdown} onPress={(e) => e.stopPropagation()}>
            <Text style={styles.dropdownTitle}>Select Exit Location</Text>

            {/* Toggle: Regions / Countries */}
            <View style={styles.toggleRow}>
              <Pressable
                style={[
                  styles.toggleButton,
                  viewMode === 'regions' && { backgroundColor: colors.primary + '20' },
                ]}
                onPress={() => setViewMode('regions')}
              >
                <Text style={[
                  styles.toggleText,
                  viewMode === 'regions' && { color: colors.primary },
                ]}>
                  Regions (Flexible)
                </Text>
              </Pressable>
              <Pressable
                style={[
                  styles.toggleButton,
                  viewMode === 'countries' && { backgroundColor: colors.primary + '20' },
                ]}
                onPress={() => setViewMode('countries')}
              >
                <Text style={[
                  styles.toggleText,
                  viewMode === 'countries' && { color: colors.primary },
                ]}>
                  Countries (Strict)
                </Text>
              </Pressable>
            </View>

            <ScrollView style={styles.scrollView} showsVerticalScrollIndicator={false}>
              {viewMode === 'regions' ? (
                // Region selection (flexible)
                <>
                  {regions.map((region) => {
                    const isActive = exitSelection.type === 'region' && exitSelection.region === region.key;
                    const count = exitsPerRegion[region.key];
                    return (
                      <Pressable
                        key={region.key}
                        style={[
                          styles.option,
                          isActive && { backgroundColor: colors.primary + '15' },
                        ]}
                        onPress={() => handleSelectRegion(region.key)}
                      >
                        <View style={styles.optionLeft}>
                          <Text style={styles.optionIcon}>{region.flag}</Text>
                          <View>
                            <Text style={[styles.optionLabel, isActive && { color: colors.primary }]}>
                              {region.label}
                            </Text>
                            <Text style={styles.optionDesc}>
                              {count} exit nodes available
                            </Text>
                          </View>
                        </View>
                        <View style={styles.optionRight}>
                          {isActive && (
                            <Text style={[styles.checkmark, { color: colors.primary }]}>✓</Text>
                          )}
                        </View>
                      </Pressable>
                    );
                  })}
                </>
              ) : (
                // Country selection (strict)
                <>
                  {regions.filter((r) => r.key !== 'auto').map((region) => {
                    const countries = getCountriesInRegion(region.key);
                    if (countries.length === 0) return null;

                    const isExpanded = expandedRegion === region.key;

                    return (
                      <View key={region.key}>
                        <Pressable
                          style={styles.regionHeader}
                          onPress={() => setExpandedRegion(isExpanded ? null : region.key)}
                        >
                          <View style={styles.optionLeft}>
                            <Text style={styles.optionIcon}>{region.flag}</Text>
                            <Text style={styles.regionHeaderText}>{region.label}</Text>
                          </View>
                          <Text style={styles.expandIcon}>{isExpanded ? '▲' : '▼'}</Text>
                        </Pressable>

                        {isExpanded && countries.map(([code, info]) => {
                          const isActive = exitSelection.type === 'country' && exitSelection.countryCode === code;
                          return (
                            <Pressable
                              key={code}
                              style={[
                                styles.countryOption,
                                isActive && { backgroundColor: colors.primary + '15' },
                              ]}
                              onPress={() => handleSelectCountry(code, region.key)}
                            >
                              <View style={styles.optionLeft}>
                                <Text style={styles.countryFlag}>{countryFlags[code] || '🏳️'}</Text>
                                <View>
                                  <Text style={[styles.optionLabel, isActive && { color: colors.primary }]}>
                                    {info.name}
                                  </Text>
                                  <Text style={styles.optionDesc}>
                                    {info.count} nodes • {info.bestLatency}ms
                                  </Text>
                                </View>
                              </View>
                              {isActive && (
                                <Text style={[styles.checkmark, { color: colors.primary }]}>✓</Text>
                              )}
                            </Pressable>
                          );
                        })}
                      </View>
                    );
                  })}
                </>
              )}
            </ScrollView>

            <Text style={styles.hint}>
              {viewMode === 'regions'
                ? 'Flexible: Exit through any node in the selected region'
                : 'Strict: Exit only through nodes in the selected country'}
            </Text>
          </Pressable>
        </Pressable>
      </Modal>
    </>
  );
}

export function RegionSelector() {
  const { mode } = useTunnel();

  // Client mode: Only exit selector
  if (mode === 'client') {
    return (
      <View style={styles.container}>
        <ExitSelector />
      </View>
    );
  }

  // Node mode: Only detected location
  if (mode === 'node') {
    return (
      <View style={styles.container}>
        <DetectedLocationCard />
      </View>
    );
  }

  // Both mode: Show detected location AND exit selector
  return (
    <View style={styles.container}>
      <DetectedLocationCard />
      <View style={styles.spacer} />
      <ExitSelector />
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    paddingHorizontal: spacing.xl,
    marginBottom: spacing.lg,
  },
  spacer: {
    height: spacing.md,
  },
  locationDetails: {
    alignItems: 'flex-end',
  },
  ispText: {
    ...typography.labelSmall,
    color: palette.silver,
    opacity: 0.7,
    marginTop: 2,
    maxWidth: 120,
  },
  card: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    backgroundColor: theme.background.tertiary,
    borderRadius: radius.lg,
    padding: spacing.lg,
    borderWidth: 2,
    borderColor: 'transparent',
  },
  cardLeft: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing.md,
  },
  cardIcon: {
    fontSize: 24,
  },
  cardLabel: {
    ...typography.labelSmall,
    color: theme.text.tertiary,
    textTransform: 'none',
  },
  cardValue: {
    ...typography.headingSmall,
    marginTop: 2,
  },
  cardRight: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: spacing.md,
  },
  cardDesc: {
    ...typography.bodySmall,
    color: theme.text.tertiary,
  },
  chevron: {
    fontSize: 12,
    color: theme.text.tertiary,
  },
  badge: {
    ...typography.labelSmall,
    paddingHorizontal: spacing.sm,
    paddingVertical: 2,
    borderRadius: radius.sm,
    overflow: 'hidden',
    fontWeight: '600',
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
    maxHeight: '80%',
  },
  dropdownTitle: {
    ...typography.headingSmall,
    color: theme.text.primary,
    textAlign: 'center',
    marginBottom: spacing.md,
  },
  toggleRow: {
    flexDirection: 'row',
    marginBottom: spacing.lg,
    gap: spacing.sm,
  },
  toggleButton: {
    flex: 1,
    paddingVertical: spacing.md,
    paddingHorizontal: spacing.sm,
    borderRadius: radius.md,
    alignItems: 'center',
  },
  toggleText: {
    ...typography.labelSmall,
    color: theme.text.tertiary,
  },
  scrollView: {
    flexGrow: 0,
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
    gap: spacing.sm,
  },
  checkmark: {
    fontSize: 18,
    fontWeight: '600',
  },
  regionHeader: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    padding: spacing.md,
    backgroundColor: theme.background.tertiary,
    borderRadius: radius.md,
    marginBottom: spacing.xs,
  },
  regionHeaderText: {
    ...typography.labelLarge,
    color: theme.text.secondary,
  },
  expandIcon: {
    fontSize: 10,
    color: theme.text.tertiary,
  },
  countryOption: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    paddingVertical: spacing.md,
    paddingHorizontal: spacing.lg,
    marginLeft: spacing.xl,
    borderRadius: radius.md,
    marginBottom: spacing.xs,
  },
  countryFlag: {
    fontSize: 20,
  },
  hint: {
    ...typography.bodySmall,
    color: theme.text.tertiary,
    textAlign: 'center',
    marginTop: spacing.lg,
    fontStyle: 'italic',
  },
});
