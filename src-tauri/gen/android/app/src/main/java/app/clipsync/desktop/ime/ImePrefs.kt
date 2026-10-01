package app.clipsync.desktop.ime

import android.content.Context
import android.content.SharedPreferences

/**
 * Configuração do teclado. Fica em SharedPreferences (e não no SQLite do núcleo
 * Rust) porque o IME precisa ler o tema mesmo quando o app ClipSync não está
 * rodando. Só guarda preferências de aparência — nunca texto.
 */
object ImePrefs {
    private const val FILE = "clipsync_ime"
    const val KEY_THEME = "theme"

    fun prefs(context: Context): SharedPreferences =
        context.applicationContext.getSharedPreferences(FILE, Context.MODE_PRIVATE)

    fun themeId(context: Context): String =
        prefs(context).getString(KEY_THEME, null) ?: KeyboardThemes.default.id

    fun setThemeId(context: Context, id: String) {
        val valid = KeyboardThemes.byId(id).id
        prefs(context).edit().putString(KEY_THEME, valid).apply()
    }
}
