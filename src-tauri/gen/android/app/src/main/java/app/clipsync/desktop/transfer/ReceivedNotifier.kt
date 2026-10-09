package app.clipsync.desktop.transfer

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.ProcessLifecycleOwner
import app.clipsync.desktop.R

/**
 * "Arquivo recebido" ao concluir um recebimento; tocar abre o arquivo (o FGS some sem aviso ao
 * terminar). Com o app na frente não notifica: a UI já mostra. Sem permissão, nada.
 */
object ReceivedNotifier {
    private const val CHANNEL = "clipsync_received"

    @JvmStatic
    fun notify(context: Context, transferId: String, title: String, text: String, uri: String, mime: String?): Boolean {
        val foreground = runCatching {
            ProcessLifecycleOwner.get().lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)
        }.getOrDefault(false)
        if (foreground) return false
        if (Build.VERSION.SDK_INT >= 33 &&
            ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) return false

        val app = context.applicationContext
        val nm = app.getSystemService(NotificationManager::class.java)
        // Sempre recria: o sistema só atualiza nome/descrição do canal existente, e assim ele segue o idioma atual.
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            nm.createNotificationChannel(NotificationChannel(CHANNEL, app.getString(R.string.channel_received), NotificationManager.IMPORTANCE_DEFAULT))
        }

        // URI do MediaStore é nossa (Downloads/ClipSync): podemos conceder leitura a quem abrir.
        val view = Intent(Intent.ACTION_VIEW)
            .setDataAndType(Uri.parse(uri), mime ?: app.contentResolver.getType(Uri.parse(uri)) ?: "*/*")
            .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
        val chooser = Intent.createChooser(view, app.getString(R.string.received_open_with)).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        val open = PendingIntent.getActivity(app, transferId.hashCode(), chooser, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)

        val n = NotificationCompat.Builder(app, CHANNEL)
            .setSmallIcon(R.drawable.ic_stat_clipsync)
            .setContentTitle(title)
            .setContentText(text)
            // Tocar abre o arquivo e some (botão de ação não auto-cancela: ficaria a notificação velha).
            .setContentIntent(open)
            .setAutoCancel(true)
            .build()
        return runCatching { NotificationManagerCompat.from(app).notify(transferId.hashCode(), n); true }.getOrDefault(false)
    }
}
