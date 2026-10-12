package app.clipsync.desktop.update

import android.content.Context
import app.clipsync.desktop.BuildConfig
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL

/**
 * Última release no GitHub para o aviso de atualização (o Rust do Android não tem cliente HTTP).
 * Só transporte: devolve o JSON ou lança; quem interpreta é o `updater.rs`.
 */
object UpdateBridge {
    private const val TIMEOUT_MS = 10_000
    private const val MAX_BYTES = 1 shl 20

    @JvmStatic
    @Suppress("UNUSED_PARAMETER")
    fun fetch(ctx: Context, url: String): String {
        require(url.startsWith("https://api.github.com/")) { "URL não permitida" }
        val conn = URL(url).openConnection() as HttpURLConnection
        try {
            conn.connectTimeout = TIMEOUT_MS
            conn.readTimeout = TIMEOUT_MS
            conn.instanceFollowRedirects = true
            conn.setRequestProperty("User-Agent", "ClipSync-Android/${BuildConfig.VERSION_NAME}")
            conn.setRequestProperty("Accept", "application/vnd.github+json")
            val code = conn.responseCode
            if (code != HttpURLConnection.HTTP_OK) throw IOException("GitHub respondeu HTTP $code")
            val out = java.io.ByteArrayOutputStream()
            conn.inputStream.use { input ->
                val buf = ByteArray(8192)
                while (true) {
                    val n = input.read(buf)
                    if (n < 0) break
                    out.write(buf, 0, n)
                    if (out.size() > MAX_BYTES) throw IOException("resposta grande demais")
                }
            }
            return out.toString("UTF-8")
        } finally {
            conn.disconnect()
        }
    }
}
