package app.clipsync.desktop.ime

import android.content.Context

/**
 * Fronteira Kotlin ↔ núcleo Rust para o clipboard do IME.
 *
 * - `nativeOnClip` (implementada em `src-tauri/src/ime.rs`): entrega um clip ao
 *   `clipboard::handle_text`. Só pode ser chamada quando [ready] é `true` — antes
 *   disso a biblioteca nativa pode nem estar carregada (processo criado só para o
 *   IME, sem a Activity do Tauri).
 * - `setReady` é chamada pelo Rust no fim do `setup`.
 * - `listInbox` / `removeInbox` são chamadas pelo Rust para drenar o [ImeInbox].
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
