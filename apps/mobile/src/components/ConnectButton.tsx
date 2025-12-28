import React from 'react';
import {
  TouchableOpacity,
  Text,
  StyleSheet,
  ActivityIndicator,
} from 'react-native';

interface ConnectButtonProps {
  isConnected: boolean;
  isLoading: boolean;
  onPress: () => void;
  size?: 'small' | 'medium' | 'large';
}

const sizes = {
  small: {width: 140, height: 44, fontSize: 16},
  medium: {width: 200, height: 56, fontSize: 18},
  large: {width: 260, height: 68, fontSize: 20},
};

export const ConnectButton: React.FC<ConnectButtonProps> = ({
  isConnected,
  isLoading,
  onPress,
  size = 'medium',
}) => {
  const sizeConfig = sizes[size];
  const backgroundColor = isConnected ? '#e74c3c' : '#3498db';
  const text = isConnected ? 'Disconnect' : 'Connect';

  return (
    <TouchableOpacity
      style={[
        styles.button,
        {
          width: sizeConfig.width,
          height: sizeConfig.height,
          borderRadius: sizeConfig.height / 2,
          backgroundColor,
        },
      ]}
      onPress={onPress}
      disabled={isLoading}
      activeOpacity={0.8}>
      {isLoading ? (
        <ActivityIndicator color="#fff" size="small" />
      ) : (
        <Text style={[styles.text, {fontSize: sizeConfig.fontSize}]}>
          {text}
        </Text>
      )}
    </TouchableOpacity>
  );
};

const styles = StyleSheet.create({
  button: {
    justifyContent: 'center',
    alignItems: 'center',
    shadowColor: '#000',
    shadowOffset: {width: 0, height: 4},
    shadowOpacity: 0.2,
    shadowRadius: 8,
    elevation: 5,
  },
  text: {
    color: '#fff',
    fontWeight: '600',
  },
});
