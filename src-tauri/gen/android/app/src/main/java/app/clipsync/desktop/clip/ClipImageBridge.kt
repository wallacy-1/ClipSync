package app.clipsync.desktop.clip

import android.content.ClipData
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
 * Imagem no clipboard do Android (ponte JNI). Só funciona com o app em primeiro
 * plano (Android 10+), como o texto. Sem regra de sync aqui: o Rust decide o que
 * enviar e quando aplicar.
 */
object ClipImageBridge {
    /** Rótulo das imagens que o próprio ClipSync põe no clipboard (anti-loop). */
    private const val OWN_LABEL = "ClipSync"
    private const val CACHE_SUBDIR = "clip"
    private const val KEEP_IN_CACHE = 5
    private const val TAG = "ClipSyncClip"

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
        if (desc.label?.toString() == OWN_LABEL) return null
        val ts = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) desc.timestamp else 0L
        val mime = (0 until desc.mimeTypeCount).map { desc.getMimeType(it) }.firstOrNull { it.startsWith("image/") }
        return JSONObject()
            .put("stamp", "$ts|${desc.label ?: ""}|$mime")
            .put("mime", mime ?: JSONObject.NULL)
            .toString()
    }

    /**
     * Converte a imagem do clipboard para PNG em [destPath] (o fio leva sempre PNG,
     * o PC só decodifica PNG). Retorna o tamanho gravado, ou -1.
     */
    @JvmStatic
    fun copyAsPng(context: Context, destPath: String): Long {
        return try {
            val clip = manager(context)?.primaryClip ?: return -1
            if (clip.itemCount == 0) return -1
            val uri = clip.getItemAt(0).uri ?: return -1
            val bitmap = context.contentResolver.openInputStream(uri)?.use { BitmapFactory.decodeStream(it) } ?: return -1
            val out = File(destPath)
            out.parentFile?.mkdirs()
            FileOutputStream(out).use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
            bitmap.recycle()
            out.length()
        } catch (_: Exception) {
            -1
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
}
