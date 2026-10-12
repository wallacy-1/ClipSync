package app.clipsync.desktop.share

import android.app.Activity
import android.content.Intent
import android.os.Bundle
import app.clipsync.desktop.MainActivity

/**
 * Porta de entrada do "Compartilhar → ClipSync". Quem compartilha pedindo resultado (`FORWARD_RESULT`)
 * faz o Android ignorar o `singleTask` e criar outra `MainActivity` na task da origem; com o núcleo já
 * vivo (teclado, "Manter conectado") essa segunda Activity nunca ganha WebView e fica só o fundo escuro.
 * Aqui o Intent é repassado à `MainActivity` na task dela, levando a permissão de leitura das URIs.
 */
class ShareActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val src = intent
        if (src != null) {
            val forward = Intent(src).apply {
                setClass(this@ShareActivity, MainActivity::class.java)
                // Sem as flags da origem (FORWARD_RESULT, NEW_DOCUMENT...): só a task própria e a leitura.
                flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_GRANT_READ_URI_PERMISSION
            }
            runCatching { startActivity(forward) }
        }
        setResult(RESULT_OK)
        finish()
    }
}
