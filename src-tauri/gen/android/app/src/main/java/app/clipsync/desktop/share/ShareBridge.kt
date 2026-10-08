package app.clipsync.desktop.share

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.util.Log
import app.clipsync.desktop.transfer.FileBridge
import org.json.JSONArray
import org.json.JSONObject

/**
 * "Compartilhar → ClipSync": traduz o Intent em `{files:[{uri,name,size,mime}], text}`
 * pendente; a UI busca via `share_take` ao abrir e ao voltar ao primeiro plano.
 */
object ShareBridge {
    @Volatile private var pending: String? = null

    /** `true` se o Intent era um compartilhamento (e virou pendente). */
    fun onIntent(context: Context, intent: Intent?): Boolean {
        if (intent == null) return false
        val uris: List<Uri> = when (intent.action) {
            Intent.ACTION_SEND -> listOfNotNull(streamExtra(intent))
            Intent.ACTION_SEND_MULTIPLE -> streamListExtra(intent)
            else -> return false
        }
        val files = JSONArray()
        for (uri in uris) {
            // Permissão de share é temporária; persistir só funciona se a origem permitiu.
            runCatching { context.contentResolver.takePersistableUriPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION) }
            runCatching { files.put(JSONObject(FileBridge.describe(context, uri.toString()))) }
        }
        val text = if (uris.isEmpty()) intent.getCharSequenceExtra(Intent.EXTRA_TEXT)?.toString()?.takeIf { it.isNotBlank() } else null
        if (files.length() == 0 && text == null) return false
        pending = JSONObject()
            .put("files", files)
            .put("text", text ?: JSONObject.NULL)
            .toString()
        // App já aberto (onNewIntent) não gera visibilitychange: avisa a UI. Antes de o
        // núcleo existir (cold start) falha calado e a UI busca ao montar.
        runCatching { nativeShareReceived() }.onFailure { Log.i("ClipSyncShare", "núcleo ainda não subiu: ${it.javaClass.simpleName}") }
        return true
    }

    /** JSON do compartilhamento pendente (consumido na leitura), ou null. */
    @JvmStatic
    fun take(): String? {
        val p = pending
        pending = null
        return p
    }

    @JvmStatic
    external fun nativeShareReceived()

    @Suppress("DEPRECATION")
    private fun streamExtra(intent: Intent): Uri? =
        if (Build.VERSION.SDK_INT >= 33) intent.getParcelableExtra(Intent.EXTRA_STREAM, Uri::class.java)
        else intent.getParcelableExtra(Intent.EXTRA_STREAM)

    @Suppress("DEPRECATION")
    private fun streamListExtra(intent: Intent): List<Uri> =
        (if (Build.VERSION.SDK_INT >= 33) intent.getParcelableArrayListExtra(Intent.EXTRA_STREAM, Uri::class.java)
        else intent.getParcelableArrayListExtra(Intent.EXTRA_STREAM)) ?: emptyList()
}
