package app.clipsync.desktop.keepalive

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import androidx.core.app.NotificationCompat
import app.clipsync.desktop.MainActivity
import app.clipsync.desktop.R
import java.util.concurrent.atomic.AtomicInteger

/**
 * Foreground Service `connectedDevice` do ajuste "Manter conectado": tira o processo
 * do estado cached, onde o Android corta a rede, para o clipboard chegar com o app fechado.
 * Ligado/desligado pelo Rust (`src-tauri/src/keepalive.rs`); mesmo contrato do
 * `TransferService` contra o serviço recriado pelo sistema num processo sem núcleo.
 */
class KeepAliveService : Service() {
    companion object {
        private const val TAG = "ClipSyncKeepAlive"
        private const val CHANNEL = "clipsync_keepalive"
        private const val NOTIFICATION_ID = 0x7F12
        private const val ACTION_START = "app.clipsync.keepalive.START"

        @Volatile private var instance: KeepAliveService? = null
        @Volatile private var desired = false
        private val pendingStarts = AtomicInteger(0)
        private val main = Handler(Looper.getMainLooper())

        @Volatile private var running = false

        /** `false` = o sistema recusou (app em segundo plano); o Rust tenta de novo no próximo resume. */
        @JvmStatic
        fun start(context: Context): Boolean {
            desired = true
            if (running) return true
            val ctx = context.applicationContext
            val i = Intent(ctx, KeepAliveService::class.java).setAction(ACTION_START)
            pendingStarts.incrementAndGet()
            return try {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) ctx.startForegroundService(i) else ctx.startService(i)
                true
            } catch (e: Exception) {
                pendingStarts.decrementAndGet()
                android.util.Log.w(TAG, "startForegroundService recusado: ${e.javaClass.simpleName}")
                false
            }
        }

        @JvmStatic
        fun stop() {
            desired = false
            val s = instance ?: return
            main.post { s.stopIfNotDesired() }
        }

        @JvmStatic
        fun isRunning(): Boolean = running
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        instance = this
        if (!desired && pendingStarts.get() == 0) {
            // Recriado pelo sistema depois de o processo morrer: aqui não há núcleo Rust nem rede.
            main.post {
                if (desired || pendingStarts.get() > 0) return@post
                android.util.Log.w(TAG, "serviço recriado sem núcleo: encerrando")
                foreground()
                stopNow()
            }
        }
    }

    override fun onDestroy() {
        running = false
        if (instance === this) instance = null
        super.onDestroy()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_START) {
            val ours = pendingStarts.getAndUpdate { if (it > 0) it - 1 else 0 } > 0
            val ok = foreground()
            if (!ours || (ok && !desired && pendingStarts.get() == 0)) stopNow()
        } else if (!running) {
            stopSelf()
        }
        return START_NOT_STICKY
    }

    private fun foreground(): Boolean {
        return try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                startForeground(NOTIFICATION_ID, build(), ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE)
            } else {
                startForeground(NOTIFICATION_ID, build())
            }
            running = true
            true
        } catch (e: Exception) {
            android.util.Log.w(TAG, "startForeground recusado: ${e.javaClass.simpleName}")
            running = false
            stopSelf()
            false
        }
    }

    private fun stopNow() {
        running = false
        stopForeground(STOP_FOREGROUND_REMOVE)
        stopSelf()
    }

    private fun stopIfNotDesired() {
        if (desired || pendingStarts.get() > 0) return
        stopNow()
    }

    private fun build(): Notification {
        val nm = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O && nm.getNotificationChannel(CHANNEL) == null) {
            nm.createNotificationChannel(
                NotificationChannel(CHANNEL, "Manter conectado", NotificationManager.IMPORTANCE_MIN).apply {
                    description = "Mantém o ClipSync recebendo cópias com o app fechado"
                    setShowBadge(false)
                },
            )
        }
        val open = PendingIntent.getActivity(
            this, 0,
            Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        return NotificationCompat.Builder(this, CHANNEL)
            .setSmallIcon(R.drawable.ic_stat_clipsync)
            .setContentTitle("ClipSync conectado")
            .setContentText("Recebendo cópias em segundo plano. Desligue em Ajustes → Manter conectado.")
            .setPriority(NotificationCompat.PRIORITY_MIN)
            .setOngoing(true)
            .setShowWhen(false)
            .setContentIntent(open)
            .build()
    }
}
