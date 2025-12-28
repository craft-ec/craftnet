import React from 'react';
import {View, Text, StyleSheet} from 'react-native';
import {ConnectionState} from '../native/TunnelCraftVPN';

interface StatusIndicatorProps {
  state: ConnectionState;
  size?: 'small' | 'medium' | 'large';
}

const stateConfig: Record<
  ConnectionState,
  {color: string; icon: string; text: string}
> = {
  disconnected: {color: '#95a5a6', icon: '🔓', text: 'Not Protected'},
  connecting: {color: '#f39c12', icon: '⏳', text: 'Connecting...'},
  connected: {color: '#27ae60', icon: '🔒', text: 'Protected'},
  disconnecting: {color: '#f1c40f', icon: '⏳', text: 'Disconnecting...'},
  error: {color: '#e74c3c', icon: '⚠️', text: 'Error'},
};

const sizes = {
  small: {circle: 60, icon: 24, text: 14},
  medium: {circle: 100, icon: 40, text: 18},
  large: {circle: 140, icon: 56, text: 22},
};

export const StatusIndicator: React.FC<StatusIndicatorProps> = ({
  state,
  size = 'medium',
}) => {
  const config = stateConfig[state];
  const sizeConfig = sizes[size];

  return (
    <View style={styles.container}>
      <View
        style={[
          styles.circle,
          {
            width: sizeConfig.circle,
            height: sizeConfig.circle,
            borderRadius: sizeConfig.circle / 2,
            backgroundColor: config.color,
            shadowColor: config.color,
          },
        ]}>
        <Text style={[styles.icon, {fontSize: sizeConfig.icon}]}>
          {config.icon}
        </Text>
      </View>
      <Text style={[styles.text, {fontSize: sizeConfig.text}]}>
        {config.text}
      </Text>
    </View>
  );
};

const styles = StyleSheet.create({
  container: {
    alignItems: 'center',
    gap: 12,
  },
  circle: {
    justifyContent: 'center',
    alignItems: 'center',
    shadowOffset: {width: 0, height: 4},
    shadowOpacity: 0.4,
    shadowRadius: 10,
    elevation: 8,
  },
  icon: {
    textAlign: 'center',
  },
  text: {
    fontWeight: '600',
    color: '#666',
  },
});
