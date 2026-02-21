# React Native ProGuard Rules

# Keep our native module
-keep class com.craftnet.** { *; }

# React Native
-keep class com.facebook.react.** { *; }
-keep class com.facebook.hermes.** { *; }

# Keep native methods
-keepclassmembers class * {
    native <methods>;
}

# CraftNet UniFFI bindings
-keep class uniffi.** { *; }
