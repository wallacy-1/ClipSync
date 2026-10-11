package app.clipsync.desktop.ime

import android.content.Context
import android.content.SharedPreferences

/**
 * Configuração do teclado. Fica em SharedPreferences (e não no SQLite do núcleo
 * Rust) porque o IME precisa ler o tema mesmo quando o app ClipSync não está
 * rodando. Só guarda aparência e os emojis usados por último — nunca texto digitado.
 */
object ImePrefs {
    private const val FILE = "clipsync_ime"
    const val KEY_THEME = "theme"
    /** Emojis usados por último (só os emojis, como no Gboard), para a aba "recentes". */
    private const val KEY_EMOJI_RECENT = "emoji_recent"
    private const val MAX_RECENT = 32

    fun prefs(context: Context): SharedPreferences =
        context.applicationContext.getSharedPreferences(FILE, Context.MODE_PRIVATE)

    fun themeId(context: Context): String =
        prefs(context).getString(KEY_THEME, null) ?: KeyboardThemes.default.id

    fun setThemeId(context: Context, id: String) {
        val valid = KeyboardThemes.byId(id).id
        prefs(context).edit().putString(KEY_THEME, valid).apply()
    }

    fun recentEmojis(context: Context): List<String> =
        prefs(context).getString(KEY_EMOJI_RECENT, null)?.split('\n')?.filter { it.isNotEmpty() } ?: emptyList()

    fun addRecentEmoji(context: Context, emoji: String) {
        val list = (listOf(emoji) + recentEmojis(context).filter { it != emoji }).take(MAX_RECENT)
        prefs(context).edit().putString(KEY_EMOJI_RECENT, list.joinToString("\n")).apply()
    }
}
