package app.clipsync.desktop.transfer

import android.content.Context

/** Fronteira Rust ↔ Kotlin do [TransferService] e do Cancelar da notificação (nativos em `src-tauri/src/transfer/android.rs`). */
object TransferBridge {
    /** Sobe (ou reafirma) o FGS. `false` = o sistema recusou iniciar do background. */
    @JvmStatic
    fun fgsStart(context: Context, title: String, text: String, percent: Int, transferId: String): Boolean =
        TransferService.start(context.applicationContext, title, text, percent, transferId)

    @JvmStatic
    fun fgsUpdate(title: String, text: String, percent: Int, transferId: String) =
        TransferService.update(title, text, percent, transferId)

    @JvmStatic
    fun fgsStop() = TransferService.stop()

    @JvmStatic
    fun fgsRunning(): Boolean = TransferService.running

    fun cancelFromNotification(transferId: String) {
        runCatching { nativeCancel(transferId) }
    }

    /**
     * `MainActivity.onResume`: se há transferência ativa sem FGS, o Rust tenta subir agora.
     * Seguro antes de o núcleo existir.
     */
    fun onActivityResumed() {
        runCatching { nativeActivityResumed() }
    }

    @JvmStatic
    external fun nativeCancel(transferId: String)

    @JvmStatic
    external fun nativeActivityResumed()
}
