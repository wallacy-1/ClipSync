package app.clipsync.desktop

import android.content.Context
import android.content.Intent
import android.net.wifi.WifiManager
import android.os.Bundle
import androidx.activity.enableEdgeToEdge
import app.clipsync.desktop.transfer.FileBridge
import app.clipsync.desktop.transfer.TransferBridge

class MainActivity : TauriActivity() {
  // mDNS precisa receber multicast; sem o lock o Android descarta esses pacotes.
  private var multicastLock: WifiManager.MulticastLock? = null

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    val wifi = applicationContext.getSystemService(Context.WIFI_SERVICE) as? WifiManager
    multicastLock = wifi?.createMulticastLock("clipsync-mdns")?.apply {
      setReferenceCounted(false)
      acquire()
    }
  }

  // Transferência ativa sem FGS (start recusado em segundo plano): agora pode subir.
  override fun onResume() {
    super.onResume()
    TransferBridge.onActivityResumed()
  }

  // Seletor de documentos (SAF) da transferência de arquivos.
  @Deprecated("Deprecated in Java")
  override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
    if (requestCode == FileBridge.PICK_REQUEST) {
      FileBridge.onPickResult(this, resultCode, data)
      return
    }
    @Suppress("DEPRECATION")
    super.onActivityResult(requestCode, resultCode, data)
  }

  override fun onDestroy() {
    multicastLock?.takeIf { it.isHeld }?.release()
    super.onDestroy()
  }
}
