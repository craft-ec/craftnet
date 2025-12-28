import React from 'react';
import {View, Text, TouchableOpacity, StyleSheet} from 'react-native';
import {PrivacyLevel} from '../native/TunnelCraftVPN';

interface PrivacyLevelPickerProps {
  selected: PrivacyLevel;
  onChange: (level: PrivacyLevel) => void;
}

const levels: {level: PrivacyLevel; label: string; description: string}[] = [
  {level: 'direct', label: 'Direct', description: 'Fastest - No relay hops'},
  {level: 'light', label: 'Light', description: '1 hop - Basic privacy'},
  {level: 'standard', label: 'Standard', description: '2 hops - Recommended'},
  {level: 'paranoid', label: 'Paranoid', description: '3 hops - Maximum privacy'},
];

export const PrivacyLevelPicker: React.FC<PrivacyLevelPickerProps> = ({
  selected,
  onChange,
}) => {
  return (
    <View style={styles.container}>
      <Text style={styles.label}>Privacy Level</Text>

      <View style={styles.options}>
        {levels.map(({level, label}) => (
          <TouchableOpacity
            key={level}
            style={[
              styles.option,
              selected === level && styles.optionSelected,
            ]}
            onPress={() => onChange(level)}>
            <Text
              style={[
                styles.optionText,
                selected === level && styles.optionTextSelected,
              ]}>
              {label}
            </Text>
          </TouchableOpacity>
        ))}
      </View>

      <Text style={styles.description}>
        {levels.find(l => l.level === selected)?.description}
      </Text>
    </View>
  );
};

const styles = StyleSheet.create({
  container: {
    backgroundColor: '#fff',
    borderRadius: 12,
    padding: 16,
    shadowColor: '#000',
    shadowOffset: {width: 0, height: 2},
    shadowOpacity: 0.1,
    shadowRadius: 4,
    elevation: 3,
  },
  label: {
    fontSize: 14,
    fontWeight: '500',
    color: '#666',
    marginBottom: 12,
  },
  options: {
    flexDirection: 'row',
    backgroundColor: '#f0f0f0',
    borderRadius: 8,
    padding: 4,
  },
  option: {
    flex: 1,
    paddingVertical: 10,
    paddingHorizontal: 8,
    borderRadius: 6,
    alignItems: 'center',
  },
  optionSelected: {
    backgroundColor: '#3498db',
  },
  optionText: {
    fontSize: 13,
    fontWeight: '500',
    color: '#666',
  },
  optionTextSelected: {
    color: '#fff',
  },
  description: {
    fontSize: 12,
    color: '#888',
    marginTop: 12,
    textAlign: 'center',
  },
});
