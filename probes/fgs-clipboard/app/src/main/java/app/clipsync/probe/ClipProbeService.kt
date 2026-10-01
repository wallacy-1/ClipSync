package app.clipsync.probe

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.SystemClock
import android.util.Log
import java.security.MessageDigest

/**
 * Foreground Service de TESTE: observa o ClipboardManager por listener e por
 * polling (a cada 2 s) e registra apenas eventos + hash curto do conteúdo.
 * Nunca registra o texto.
 */
class ClipProbeService : Service() {
    companion object {
        const val TAG = "ClipProbe"
        const val CHANNEL = "clipprobe"
        const val NOTIF_ID = 42
        @Volatile var running = false
        @Volatile var lastHash: String? = null
    }

    private lateinit var cm: ClipboardManager
    private val handler = Handler(Looper.getMainLooper())
    private val started = SystemClock.elapsedRealtime()
    private var polls = 0
    private var nullReads = 0

    private val listener = ClipboardManager.OnPrimaryClipChangedListener {
        Log.i(TAG, "[LISTENER] onPrimaryClipChanged fired uptime=${uptime()}s")
        readAndLog("listener")
    }

    private val poller = object : Runnable {
        override fun run() {
            polls++
            readAndLog("poll#$polls")
            handler.postDelayed(this, 2000)
        }
    }

    override fun onCreate() {
        super.onCreate()
        cm = getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
        val nm = getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(NotificationChannel(CHANNEL, "ClipProbe", NotificationManager.IMPORTANCE_LOW))
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val notif: Notification = Notification.Builder(this, CHANNEL)
            .setContentTitle("ClipProbe ativo")
            .setContentText("Teste: observando o clipboard em foreground service")
            .setSmallIcon(android.R.drawable.ic_menu_edit)
            .setOngoing(true)
            .build()
        // API 34+: o tipo passado aqui precisa bater com o manifesto e com a permissão FOREGROUND_SERVICE_DATA_SYNC.
        if (Build.VERSION.SDK_INT >= 34) {
            startForeground(NOTIF_ID, notif, ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
        } else {
            startForeground(NOTIF_ID, notif)
        }
        running = true
        cm.addPrimaryClipChangedListener(listener)
        handler.removeCallbacks(poller)
        handler.post(poller)
        Log.i(TAG, "[SERVICE] started sdk=${Build.VERSION.SDK_INT} type=dataSync")
        return START_STICKY
    }

    private fun readAndLog(src: String) {
        val has = cm.hasPrimaryClip()
        val desc = try { cm.primaryClipDescription } catch (e: Exception) { null }
        val clip = try { cm.primaryClip } catch (e: SecurityException) { Log.w(TAG, "[READ] SecurityException: ${e.message}"); null }
        val text = clip?.getItemAt(0)?.coerceToText(this)?.toString()
        val hash = text?.let { sha8(it) }
        if (clip == null) nullReads++
        val changed = hash != null && hash != lastHash
        if (changed) lastHash = hash
        val fg = MainActivity.inForeground
        // Só metadados: nunca o conteúdo.
        Log.i(TAG, "[READ] src=$src activityForeground=$fg hasPrimaryClip=$has desc=${desc != null} clip=${clip != null} " +
            "len=${text?.length ?: -1} hash=${hash ?: "-"} changed=$changed nullReads=$nullReads uptime=${uptime()}s")
    }

    private fun uptime() = (SystemClock.elapsedRealtime() - started) / 1000

    private fun sha8(s: String): String =
        MessageDigest.getInstance("SHA-256").digest(s.toByteArray()).take(4).joinToString("") { "%02x".format(it) }

    override fun onDestroy() {
        running = false
        handler.removeCallbacks(poller)
        cm.removePrimaryClipChangedListener(listener)
        Log.i(TAG, "[SERVICE] destroyed uptime=${uptime()}s")
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null
}
