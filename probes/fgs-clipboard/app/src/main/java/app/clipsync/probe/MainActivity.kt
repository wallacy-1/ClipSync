package app.clipsync.probe

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.util.Log
import android.widget.Button
import android.widget.LinearLayout
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat

class MainActivity : AppCompatActivity() {
    companion object { @Volatile var inForeground = false }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val root = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL; setPadding(48, 96, 48, 48) }
        val status = TextView(this).apply { textSize = 18f; text = "ClipProbe — teste de Foreground Service" }
        val start = Button(this).apply { text = "Iniciar service"; setOnClickListener { startProbe() } }
        val stop = Button(this).apply { text = "Parar service"; setOnClickListener { stopService(Intent(this@MainActivity, ClipProbeService::class.java)) } }
        root.addView(status); root.addView(start); root.addView(stop)
        setContentView(root)

        if (Build.VERSION.SDK_INT >= 33 &&
            ContextCompat.checkSelfPermission(this, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            ActivityCompat.requestPermissions(this, arrayOf(Manifest.permission.POST_NOTIFICATIONS), 1)
        }
        // Automação: `am start ... --ez start true` inicia o service (FGS só pode nascer com o app em foreground).
        if (intent?.getBooleanExtra("start", false) == true) startProbe()
        // Automação: `am start ... --es copy_image x.png` põe uma imagem no clipboard como
        // um app qualquer faria ("Copiar imagem"). O PNG vem de getExternalFilesDir (adb push).
        intent?.getStringExtra("copy_image")?.let { copyImage(it) }
    }

    private fun copyImage(name: String) {
        runCatching {
            val src = java.io.File(getExternalFilesDir(null), name)
            val values = android.content.ContentValues().apply {
                put(android.provider.MediaStore.Images.Media.DISPLAY_NAME, "probe-${System.currentTimeMillis()}-$name")
                put(android.provider.MediaStore.Images.Media.MIME_TYPE, "image/png")
            }
            val uri = contentResolver.insert(android.provider.MediaStore.Images.Media.EXTERNAL_CONTENT_URI, values)!!
            contentResolver.openOutputStream(uri)!!.use { out -> src.inputStream().use { it.copyTo(out) } }
            val cm = getSystemService(android.content.ClipboardManager::class.java)
            cm.setPrimaryClip(android.content.ClipData.newUri(contentResolver, "probe", uri))
            Log.i(ClipProbeService.TAG, "[COPY] image on clipboard uri=$uri bytes=${src.length()}")
        }.onFailure { Log.w(ClipProbeService.TAG, "[COPY] failed: $it") }
    }

    private fun startProbe() {
        val i = Intent(this, ClipProbeService::class.java)
        ContextCompat.startForegroundService(this, i)
        Log.i(ClipProbeService.TAG, "[ACTIVITY] startForegroundService requested")
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        inForeground = hasFocus
        Log.i(ClipProbeService.TAG, "[ACTIVITY] windowFocus=$hasFocus")
    }

    override fun onStop() {
        inForeground = false
        Log.i(ClipProbeService.TAG, "[ACTIVITY] onStop (background)")
        super.onStop()
    }
}
