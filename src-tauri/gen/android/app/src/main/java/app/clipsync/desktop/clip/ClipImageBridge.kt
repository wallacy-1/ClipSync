package app.clipsync.desktop.clip

import android.content.ClipData
import android.content.ClipDescription
import android.content.ClipboardManager
import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.util.Log
import androidx.core.content.FileProvider
import org.json.JSONObject
import java.io.File
import java.io.FileOutputStream

/**
 * Imagem no clipboard do Android (ponte JNI). Como o texto, só funciona com o app
 * em primeiro plano (Android 10+).
 */
object ClipImageBridge {
    /** Rótulo das imagens que o próprio ClipSync põe no clipboard (anti-loop). */
    private const val OWN_LABEL = "ClipSync"
    private const val CACHE_SUBDIR = "clip"
    private const val KEEP_IN_CACHE = 5
    private const val TAG = "ClipSyncClip"
    private const val READ_ATTEMPTS = 4
    private const val READ_RETRY_MS = 250L

    private fun manager(context: Context) =
        context.getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager

    /**
     * Imagem copiada por outro app? Só lê a descrição (não dispara o aviso de
     * "colou da área de transferência"). Retorna `{stamp, mime}` ou null.
     */
    @JvmStatic
    fun peek(context: Context): String? {
        val desc = runCatching { manager(context)?.primaryClipDescription }.getOrNull() ?: return null
        if (!desc.hasMimeType("image/*")) return null
        if (isOwn(desc)) return null
        return JSONObject()
            .put("stamp", stamp(desc))
            .put("mime", mimeOf(desc) ?: JSONObject.NULL)
            .toString()
    }

    @JvmStatic
    fun isOwn(desc: ClipDescription): Boolean = desc.label?.toString() == OWN_LABEL

    @JvmStatic
    fun mimeOf(desc: ClipDescription): String? =
        (0 until desc.mimeTypeCount).map { desc.getMimeType(it) }.firstOrNull { it.startsWith("image/") }

    /** Identifica uma cópia (não o conteúdo). O teclado usa o mesmo formato no inbox de imagens. */
    @JvmStatic
    fun stamp(desc: ClipDescription): String {
        val ts = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) desc.timestamp else 0L
        return "$ts|${desc.label ?: ""}|${mimeOf(desc)}"
    }

    /** Decodifica a imagem de [uri] e grava como PNG em [dest]. Retorna o tamanho gravado, ou -1. */
    @JvmStatic
    fun writePng(context: Context, uri: android.net.Uri, dest: File): Long {
        return try {
            val bitmap = context.contentResolver.openInputStream(uri)?.use { BitmapFactory.decodeStream(it) } ?: return -1
            dest.parentFile?.mkdirs()
            FileOutputStream(dest).use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
            bitmap.recycle()
            dest.length()
        } catch (_: Throwable) {
            -1
        }
    }

    /** Converte a imagem do clipboard para PNG em [destPath] (o PC só decodifica PNG). Retorna o tamanho, ou -1. */
    @JvmStatic
    fun copyAsPng(context: Context, destPath: String): Long = clipToPng(context, File(destPath))

    /**
     * Imagem atual do clipboard em PNG. Logo após a cópia a permissão de leitura pode ainda não valer
     * (`SecurityException`): relê o clipboard e tenta de novo; desiste se já não é [expected]. Bloqueia.
     */
    @JvmStatic
    @JvmOverloads
    fun clipToPng(context: Context, dest: File, expected: android.net.Uri? = null): Long {
        repeat(READ_ATTEMPTS) { attempt ->
            if (attempt > 0) Thread.sleep(READ_RETRY_MS)
            val current = runCatching { manager(context)?.primaryClip?.takeIf { it.itemCount > 0 }?.getItemAt(0)?.uri }.getOrNull()
            if (expected != null && current != expected) return -1
            val n = writePng(context, current ?: return -1, dest)
            if (n > 0) return n
        }
        return -1
    }

    /**
     * Miniatura PNG de [src] em [dest] (lado maior até [max] px), decodificada já reduzida. Retorna
     * `[largura, altura, larguraMiniatura, alturaMiniatura]` (as duas primeiras do original), ou null.
     */
    @JvmStatic
    fun thumbnail(src: String, dest: String, max: Int): IntArray? {
        return try {
            val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
            BitmapFactory.decodeFile(src, bounds)
            val w = bounds.outWidth
            val h = bounds.outHeight
            if (w <= 0 || h <= 0 || max <= 0) return null
            var sample = 1
            while (maxOf(w, h) / (sample * 2) >= max) sample *= 2
            val decoded = BitmapFactory.decodeFile(src, BitmapFactory.Options().apply { inSampleSize = sample }) ?: return null
            val longest = maxOf(decoded.width, decoded.height)
            val thumb = if (longest > max) {
                val tw = maxOf(1, Math.round(decoded.width * max.toFloat() / longest))
                val th = maxOf(1, Math.round(decoded.height * max.toFloat() / longest))
                Bitmap.createScaledBitmap(decoded, tw, th, true).also { if (it !== decoded) decoded.recycle() }
            } else {
                decoded
            }
            val out = File(dest)
            out.parentFile?.mkdirs()
            FileOutputStream(out).use { thumb.compress(Bitmap.CompressFormat.PNG, 100, it) }
            val result = intArrayOf(w, h, thumb.width, thumb.height)
            thumb.recycle()
            result
        } catch (_: Throwable) {
            null
        }
    }

    /**
     * Põe a imagem de [path] no clipboard. Cópia no cache servida pelo FileProvider:
     * o sistema concede leitura ao app que colar. Escrita em segundo plano é permitida.
     */
    @JvmStatic
    fun setImage(context: Context, path: String): Boolean {
        return try {
            val dir = File(context.cacheDir, CACHE_SUBDIR).apply { mkdirs() }
            dir.listFiles()?.sortedByDescending { it.lastModified() }?.drop(KEEP_IN_CACHE - 1)?.forEach { it.delete() }
            val target = File(dir, "clip-${System.currentTimeMillis()}.png")
            File(path).copyTo(target, overwrite = true)
            val uri = FileProvider.getUriForFile(context, "${context.packageName}.fileprovider", target)
            val clip = ClipData.newUri(context.contentResolver, OWN_LABEL, uri)
            val cm = manager(context) ?: return false
            Handler(Looper.getMainLooper()).post {
                runCatching { cm.setPrimaryClip(clip) }
                    .onSuccess { Log.i(TAG, "imagem no clipboard: ${cm.primaryClipDescription?.getMimeType(0)} ${target.length()} B") }
                    .onFailure { Log.w(TAG, "setPrimaryClip falhou: $it") }
            }
            true
        } catch (_: Exception) {
            false
        }
    }

    /** HTML do clipboard (`ClipData.Item.getHtmlText`), ou null. Chamado só com "Manter formatação" ligado. */
    @JvmStatic
    fun readHtml(context: Context): String? {
        return try {
            val clip = manager(context)?.primaryClip ?: return null
            if (clip.itemCount == 0) return null
            clip.getItemAt(0).htmlText?.takeIf { it.isNotBlank() }
        } catch (_: Throwable) {
            null
        }
    }

    /** Texto + HTML no clipboard. Como `setImage`, a escrita vai pela thread principal. */
    @JvmStatic
    fun setHtml(context: Context, text: String, html: String): Boolean {
        return try {
            val clip = ClipData.newHtmlText(OWN_LABEL, text, html)
            val cm = manager(context) ?: return false
            Handler(Looper.getMainLooper()).post {
                runCatching { cm.setPrimaryClip(clip) }.onFailure { Log.w(TAG, "setPrimaryClip (html) falhou: $it") }
            }
            true
        } catch (_: Exception) {
            false
        }
    }
}
