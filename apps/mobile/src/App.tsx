/**
 * TunnelCraft Mobile App
 *
 * A decentralized P2P VPN with unified Client/Node/Both mode support
 */

import React from 'react';
import { View, StyleSheet } from 'react-native';
import { SafeAreaProvider } from 'react-native-safe-area-context';
import { TunnelProvider } from './context/TunnelContext';
import { HomeScreen } from './screens/HomeScreen';

function App() {
  return (
    <View style={styles.root}>
      <SafeAreaProvider>
        <TunnelProvider>
          <HomeScreen />
        </TunnelProvider>
      </SafeAreaProvider>
    </View>
  );
}

const styles = StyleSheet.create({
  root: {
    flex: 1,
  },
});

export default App;
