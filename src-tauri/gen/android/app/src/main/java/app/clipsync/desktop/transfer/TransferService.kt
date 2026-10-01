package app.clipsync.desktop.transfer

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
import android.os.SystemClock
import androidx.core.app.NotificationCompat
import java.util.concurrent.atomic.AtomicInteger

/**
 * Foreground Service `dataSync` que existe **só enquanto há transferência de
 * arquivo ativa**. Mantém o processo fora do estado *cached* (rede viva no
 * Android 15+) com notificação de progresso e ação Cancelar.
 *
 * Fronteira: IME = clipboard em segundo plano; este serviço = transferência ativa.
 * Um não depende do outro. O Rust liga/atualiza/desliga via [TransferBridge].
 *
  * Contrato com o sistema:
 * - Só o START passa por `startForegroundService()`; cada um exige um
 *   `startForeground()` em seguida, mesmo que o pedido de parar já tenha chegado
 *   (senão `ForegroundServiceDidNotStartInTimeException`).
 * - UPDATE e STOP falam **direto com a instância** (nunca `startService()` do
 *   background — `IllegalStateException`/`ForegroundServiceStartNotAllowedException`).
 * - "Último pedido vence": [desired] é o estado que o Rust quer; START/STOP fora
 *   de ordem convergem para ele.
 * - Nada aqui lança: o sistema recusar o FGS (app em segundo plano) não é fatal,
 *   a transferência segue sem ele e o Rust tenta de novo quando a Activity volta.
 */
class TransferService : Service() {
    companion object {
        private const val TAG = "ClipSyncTransfer"
        const val CHANNEL = "clipsync_transfer"
        const val NOTIFICATION_ID = 0x7F11
        const val ACTION_START = "app.clipsync.transfer.START"
        const val ACTION_CANCEL = "app.clipsync.transfer.CANCEL"
        const val EXTRA_TITLE = "title"
        const val EXTRA_TEXT = "text"
        const val EXTRA_PERCENT = "percent"
        const val EXTRA_TRANSFER_ID = "transferId"
        /** Piso entre `notify()` (o NotificationService descarta acima de ~5/s por pacote). */
        private const val MIN_NOTIFY_INTERVAL_MS = 250L

        /** Instância viva (entre `onCreate` e `onDestroy`). */
        @Volatile private var instance: TransferService? = null
        /** O que o Rust quer: true entre o último start e o último stop. */
        @Volatile private var desired = false
        /** `startForegroundService()` emitidos cujo `onStartCommand` ainda não rodou. */
        private val pendingStarts = AtomicInteger(0)
        private val main = Handler(Looper.getMainLooper())

        /** `startForeground()` aceito e ainda em vigor. */
        @Volatile var running = false
            private set

        /**
         * Sobe o serviço (ou reafirma o FGS se já existe). Do background (Android 12+) o
         * sistema recusa: retorna false, nada muda, sem exceção para o chamador.
         */
        fun start(context: Context, title: String, text: String, percent: Int, transferId: String): Boolean {
            desired = true
            val i = Intent(context, TransferService::class.java).setAction(ACTION_START)
                .putExtra(EXTRA_TITLE, title).putExtra(EXTRA_TEXT, text).putExtra(EXTRA_PERCENT, percent).putExtra(EXTRA_TRANSFER_ID, transferId)
            pendingStarts.incrementAndGet()
            return try {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) context.startForegroundService(i) else context.startService(i)
                true
            } catch (e: Exception) {
                pendingStarts.decrementAndGet()
                android.util.Log.w(TAG, "startForegroundService recusado: ${e.javaClass.simpleName}")
                if (instance == null) desired = false
                false
            }
        }

        /** Só a notificação; ignorado se o FGS não está em vigor. Nunca `startService()`. */
        fun update(title: String, text: String, percent: Int, transferId: String) {
            val s = instance ?: return
            if (!running) return
            main.post { s.updateNotification(title, text, percent, transferId) }
        }

        /** Pede para parar. Um START ainda em voo é resolvido quando chegar. */
        fun stop() {
            desired = false
            val s = instance ?: return
            main.post { s.stopIfNotDesired() }
        }
    }

    override fun onBind(intent: Intent?): IBinder? = null

    private var lastNotifyAt = 0L

    override fun onCreate() {
        super.onCreate()
        instance = this
        android.util.Log.i(TAG, "onCreate desired=$desired pending=${pendingStarts.get()}")
        if (!desired && pendingStarts.get() == 0) {
            // Ninguém neste processo pediu o serviço: o sistema o recriou depois de o processo
            // morrer com FGS ativo (swipe/kill) e já repôs a notificação antiga. Não há núcleo
            // Rust nem transferência aqui — encerra (o `onStartCommand` reentregue pode nem vir).
            main.post {
                if (desired || pendingStarts.get() > 0) return@post
                android.util.Log.w(TAG, "serviço recriado pelo sistema sem transferência ativa: encerrando")
                foreground(build("ClipSync", "", -1, ""))
                stopNow()
            }
        }
    }

    override fun onDestroy() {
        android.util.Log.i(TAG, "onDestroy")
        running = false
        if (instance === this) instance = null
        super.onDestroy()
    }

    /**
     * `startForeground()` (idempotente). Do background o sistema pode recusar
     * (`ForegroundServiceStartNotAllowedException`): não é fatal, o serviço encerra e
     * a transferência segue sem FGS.
     */
    private fun foreground(n: Notification): Boolean {
        return try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                startForeground(NOTIFICATION_ID, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
            } else {
                startForeground(NOTIFICATION_ID, n)
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

    /** Para, salvo se um start posterior já reafirmou o FGS ou ainda está em voo. */
    private fun stopIfNotDesired() {
        if (desired || pendingStarts.get() > 0) return
        stopNow()
    }

    private fun updateNotification(title: String, text: String, percent: Int, transferId: String) {
        if (!running) return
        val now = SystemClock.elapsedRealtime()
        // Piso global: mais de uma transferência ativa não pode somar acima do limite do sistema.
        if (now - lastNotifyAt < MIN_NOTIFY_INTERVAL_MS && percent < 100) return
        lastNotifyAt = now
        runCatching {
            (getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager).notify(NOTIFICATION_ID, build(title, text, percent, transferId))
        }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        android.util.Log.i(TAG, "onStartCommand action=${intent?.action} desired=$desired pending=${pendingStarts.get()} running=$running")
        when (intent?.action) {
            ACTION_START -> {
                // START sem `start()` neste processo = o sistema reentregou o intent depois de o
                // processo morrer (swipe/kill com FGS ativo). Não há núcleo Rust nem transferência
                // aqui: cumpre o contrato (startForeground) e encerra, senão fica um FGS fantasma.
                val ours = pendingStarts.getAndUpdate { if (it > 0) it - 1 else 0 } > 0
                val ok = foreground(build(intent))
                if (!ours) {
                    android.util.Log.w(TAG, "START reentregue pelo sistema sem transferência ativa: encerrando")
                    if (ok) stopNow()
                    return START_NOT_STICKY
                }
                // O pedido de parar chegou antes deste START: contrato cumprido, agora pode parar.
                if (ok && !desired && pendingStarts.get() == 0) stopNow()
            }
            ACTION_CANCEL -> {
                intent.getStringExtra(EXTRA_TRANSFER_ID)?.let { TransferBridge.cancelFromNotification(it) }
                // Toque numa notificação já obsoleta pode ter (re)criado o serviço sem FGS: não fica vivo à toa.
                if (!running) stopSelf()
            }
            else -> if (!running) stopSelf()
        }
        return START_NOT_STICKY
    }

    private fun build(intent: Intent): Notification = build(
        intent.getStringExtra(EXTRA_TITLE) ?: "ClipSync",
        intent.getStringExtra(EXTRA_TEXT) ?: "",
        intent.getIntExtra(EXTRA_PERCENT, -1),
        intent.getStringExtra(EXTRA_TRANSFER_ID) ?: "",
    )

    private fun build(title: String, text: String, percent: Int, id: String): Notification {
        val nm = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O && nm.getNotificationChannel(CHANNEL) == null) {
            nm.createNotificationChannel(
                NotificationChannel(CHANNEL, "Transferência de arquivos", NotificationManager.IMPORTANCE_LOW).apply {
                    description = "Progresso das transferências de arquivo do ClipSync"
                    setShowBadge(false)
                },
            )
        }
        val cancel = PendingIntent.getService(
            this, id.hashCode(),
            Intent(this, TransferService::class.java).setAction(ACTION_CANCEL).putExtra(EXTRA_TRANSFER_ID, id),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        return NotificationCompat.Builder(this, CHANNEL)
            .setSmallIcon(android.R.drawable.stat_sys_download)
            .setContentTitle(title)
            .setContentText(text)
            .setOnlyAlertOnce(true)
            .setOngoing(true)
            .setProgress(100, percent.coerceIn(0, 100), percent < 0)
            .addAction(0, "Cancelar", cancel)
            .build()
    }
}
