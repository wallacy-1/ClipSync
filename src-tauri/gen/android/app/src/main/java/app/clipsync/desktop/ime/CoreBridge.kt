package app.clipsync.desktop.ime

import android.content.Context

/**
 * Fronteira Kotlin/Rust do clipboard do IME. `nativeOnClip` (src-tauri/src/ime.rs)
 * só pode ser chamada com [ready] `true`: antes disso a biblioteca nativa pode nem
 * estar carregada (processo criado só para o IME, sem a Activity do Tauri).
 */
object CoreBridge {
    @Volatile
    var ready: Boolean = false
        private set

    @JvmStatic
    fun setReady(value: Boolean) {
        ready = value
    }

    /**
     * Retorna `true` se o núcleo recebeu o clip (mesmo que o tenha ignorado por
     * dedupe/anti-loop); `false` = núcleo indisponível, guardar no inbox.
     */
    @JvmStatic
    external fun nativeOnClip(text: String, sensitive: Boolean): Boolean

    /** JSON `[{id, hash, text, ts, sensitive}]` dos itens ainda não processados. */
    @JvmStatic
    fun listInbox(context: Context): String = ImeInbox.listJson(context)

    @JvmStatic
    fun removeInbox(context: Context, id: String) = ImeInbox.remove(context, id)
}
