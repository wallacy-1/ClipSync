# Add project specific ProGuard rules here.
# You can control the set of applied configuration files using the
# proguardFiles setting in build.gradle.
#
# For more details, see
#   http://developer.android.com/guide/developing/tools/proguard.html

# If your project uses WebView with JS, uncomment the following
# and specify the fully qualified class name to the JavaScript interface
# class:
#-keepclassmembers class fqcn.of.javascript.interface.for.webview {
#   public *;
#}

# Uncomment this to preserve the line number information for
# debugging stack traces.
#-keepattributes SourceFile,LineNumberTable

# If you keep the line number information, uncomment this to
# hide the original source file name.
#-renamesourcefileattribute SourceFile
# Classes chamadas do Rust por JNI (nome e assinaturas têm de sobreviver ao R8).
-keep class app.clipsync.desktop.KeyGuard { *; }
-keep class app.clipsync.desktop.ime.ImeBridge { *; }
-keep class app.clipsync.desktop.ime.CoreBridge { *; }
-keep class app.clipsync.desktop.transfer.FileBridge { *; }
-keep class app.clipsync.desktop.transfer.TransferBridge { *; }
-keepclasseswithmembers class * { native <methods>; }
-keep class app.clipsync.desktop.clip.ClipImageBridge { *; }
-keep class app.clipsync.desktop.share.ShareBridge { *; }
-keep class app.clipsync.desktop.transfer.NotifyPermission { *; }
-keep class app.clipsync.desktop.transfer.ReceivedNotifier { *; }
-keep class app.clipsync.desktop.keepalive.KeepAliveService { *; }
