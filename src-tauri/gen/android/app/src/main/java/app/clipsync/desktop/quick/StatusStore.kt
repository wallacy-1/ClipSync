package app.clipsync.desktop.quick

import android.content.ComponentName
import android.content.Context
import android.service.quicksettings.TileService
import org.json.JSONObject
import java.security.SecureRandom

/**
 * Último resumo de status empurrado pelo Rust (`quick::status_json`), em `SharedPreferences` privado,
 * para o bloco e o widget lerem sem o núcleo. Também guarda o token que autentica os Intents de
 * envio/cópia: `ClipSendActivity` é exportada (por causa do PROCESS_TEXT), então só aceita as
 * ações cujo Intent traz este segredo, que apenas o próprio app consegue ler.
 */
object StatusStore {
    private const val PREFS = "clipsync_quick"
    private const val KEY_STATUS = "status"
    private const val KEY_TOKEN = "token"

    /** Chamado do Rust (`quick::platform::push`) a cada mudança relevante. */
    @JvmStatic
    fun update(context: Context, json: String) {
        val app = context.applicationContext
        prefs(app).edit().putString(KEY_STATUS, json).apply()
        notifyAll(app)
    }

    fun read(context: Context): JSONObject? =
        prefs(context).getString(KEY_STATUS, null)?.let { runCatching { JSONObject(it) }.getOrNull() }

    @Synchronized
    fun token(context: Context): String {
        val p = prefs(context)
        p.getString(KEY_TOKEN, null)?.let { return it }
        val bytes = ByteArray(16).also { SecureRandom().nextBytes(it) }
        val t = bytes.joinToString("") { "%02x".format(it) }
        p.edit().putString(KEY_TOKEN, t).apply()
        return t
    }

    private fun notifyAll(app: Context) {
        runCatching { TileService.requestListeningState(app, ComponentName(app, SendTileService::class.java)) }
        runCatching { ClipWidgetProvider.refreshAll(app) }
    }

    private fun prefs(context: Context) = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
}
