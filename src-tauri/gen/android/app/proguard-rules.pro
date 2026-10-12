# Classes chamadas do Rust por JNI (nome e assinaturas têm de sobreviver ao R8).
-keep class app.clipsync.desktop.SystemBars { *; }
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
-keep class app.clipsync.desktop.quick.StatusStore { *; }
-keep class app.clipsync.desktop.quick.QuickAdd { *; }
-keep class app.clipsync.desktop.update.UpdateBridge { *; }
# Ponte JS do "Tentar de novo" (addJavascriptInterface acha o método pelo nome).
-keepclassmembers class * { @android.webkit.JavascriptInterface <methods>; }
