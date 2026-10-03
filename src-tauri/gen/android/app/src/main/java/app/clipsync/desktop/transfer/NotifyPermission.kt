package app.clipsync.desktop.transfer

import android.Manifest
import android.app.Activity
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat

/**
 * `POST_NOTIFICATIONS` (Android 13+). Sem ela o FGS da transferência roda, mas a
 * notificação com progresso e o botão Cancelar ficam invisíveis.
 */
object NotifyPermission {
    const val REQUEST = 0x4E07 // "NO"
    private const val PREFS = "clipsync_notify"
    private const val ASKED = "asked"

    /** `granted` | `prompt` (pode pedir) | `blocked` (negada de vez: só nas Configurações) | `unsupported` (< 13). */
    @JvmStatic
    fun status(context: Context): String {
        if (Build.VERSION.SDK_INT < 33) return "unsupported"
        if (ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED) return "granted"
        val asked = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getBoolean(ASKED, false)
        val activity = context as? Activity
        // Já pedimos e o sistema não mostra mais o diálogo (2 recusas ou "não perguntar de novo").
        if (asked && activity != null && !ActivityCompat.shouldShowRequestPermissionRationale(activity, Manifest.permission.POST_NOTIFICATIONS)) return "blocked"
        return "prompt"
    }

    /** Abre o diálogo do sistema. `false` = não há Activity ou não se aplica. */
    @JvmStatic
    fun request(context: Context): Boolean {
        if (Build.VERSION.SDK_INT < 33) return false
        val activity = context as? Activity ?: return false
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putBoolean(ASKED, true).apply()
        Handler(Looper.getMainLooper()).post {
            ActivityCompat.requestPermissions(activity, arrayOf(Manifest.permission.POST_NOTIFICATIONS), REQUEST)
        }
        return true
    }

    /** Configurações de notificação do app (caminho quando está `blocked`). */
    @JvmStatic
    fun openSettings(context: Context) {
        val intent = Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS)
            .putExtra(Settings.EXTRA_APP_PACKAGE, context.packageName)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        Handler(Looper.getMainLooper()).post { context.startActivity(intent) }
    }
}
