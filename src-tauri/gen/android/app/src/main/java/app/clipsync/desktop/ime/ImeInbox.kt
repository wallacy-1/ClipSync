package app.clipsync.desktop.ime

import android.content.Context
import app.clipsync.desktop.KeyGuard
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.util.UUID

/**
 * Fila de clips detectados pelo IME com o núcleo Rust morto. Um arquivo por item em
 * `files/ime_inbox/`, selado com a chave do Keystore que protege a KEK ([KeyGuard]).
 * Expira em [TTL_MS] (como a limpeza do clipboard do Android 13+) e limita a
 * [MAX_ITEMS]. O núcleo só remove cada item depois de processá-lo.
 */
object ImeInbox {
    private const val DIR = "ime_inbox"
    private const val EXT = ".sealed"
    const val TTL_MS = 60L * 60L * 1000L
    const val MAX_ITEMS = 50

    private fun dir(context: Context): File =
        File(context.applicationContext.filesDir, DIR).apply { mkdirs() }

    private fun files(context: Context): List<File> =
        dir(context).listFiles { f -> f.isFile && f.name.endsWith(EXT) }?.sortedBy { it.name } ?: emptyList()

    /** Nome `<ts>-<id>.sealed`: ordena por tempo e permite expirar sem decifrar. */
    private fun tsOf(f: File): Long = f.name.substringBefore("-").toLongOrNull() ?: 0L
    private fun idOf(f: File): String = f.name.substringAfter("-").removeSuffix(EXT)

    private fun open(f: File): JSONObject? =
        runCatching { JSONObject(String(KeyGuard.open(f.readBytes()), Charsets.UTF_8)) }.getOrNull()

    @Synchronized
    fun put(context: Context, text: String, hash: String, sensitive: Boolean): Boolean {
        prune(context)
        val existing = files(context)
        // Mesmo hash já no fim da fila (cópia repetida com o núcleo morto): não duplica.
        if (existing.lastOrNull()?.let { open(it)?.optString("hash") } == hash) return true
        if (existing.size >= MAX_ITEMS) existing.first().delete()
        val ts = System.currentTimeMillis()
        val id = UUID.randomUUID().toString()
        val payload = JSONObject()
            .put("id", id)
            .put("hash", hash)
            .put("text", text)
            .put("ts", ts)
            .put("sensitive", sensitive)
            .toString()
            .toByteArray(Charsets.UTF_8)
        val sealed = runCatching { KeyGuard.seal(payload) }.getOrNull() ?: return false
        val target = File(dir(context), "$ts-$id$EXT")
        val tmp = File(dir(context), "$ts-$id.tmp")
        return runCatching {
            tmp.writeBytes(sealed)
            tmp.renameTo(target)
        }.getOrDefault(false)
    }

    @Synchronized
    fun prune(context: Context) {
        val now = System.currentTimeMillis()
        for (f in files(context)) {
            if (now - tsOf(f) > TTL_MS) f.delete()
        }
    }

    /** Itens válidos, decifrados, em ordem; arquivos ilegíveis são descartados. */
    @Synchronized
    fun listJson(context: Context): String {
        prune(context)
        val arr = JSONArray()
        for (f in files(context)) {
            val obj = open(f)
            if (obj == null) {
                f.delete()
                continue
            }
            arr.put(obj)
        }
        return arr.toString()
    }

    @Synchronized
    fun remove(context: Context, id: String) {
        files(context).firstOrNull { idOf(it) == id }?.delete()
    }

    fun count(context: Context): Int = files(context).size

    // ---- imagens ----
    // PNG em `files/ime_inbox_img/<ts>-<id>.png` + `<ts>-<id>.stamp` (carimbo da cópia, para o
    // dedupe do núcleo). Não selados: o histórico guarda as imagens em PNG puro de qualquer forma.

    private const val IMG_DIR = "ime_inbox_img"
    const val MAX_IMAGES = 5
    /** Mesmo limite do núcleo (`image::MAX_BYTES`). */
    private const val MAX_IMAGE_BYTES = 64L * 1024 * 1024

    private fun imgDir(context: Context): File =
        File(context.applicationContext.filesDir, IMG_DIR).apply { mkdirs() }

    private fun images(context: Context): List<File> =
        imgDir(context).listFiles { f -> f.isFile && f.name.endsWith(".png") }?.sortedBy { it.name } ?: emptyList()

    private fun stampFile(png: File) = File(png.parentFile, png.name.removeSuffix(".png") + ".stamp")

    @Synchronized
    fun putImage(context: Context, uri: android.net.Uri, stamp: String): Boolean {
        pruneImages(context)
        val existing = images(context)
        if (existing.any { runCatching { stampFile(it).readText() }.getOrNull() == stamp }) return true
        if (existing.size >= MAX_IMAGES) existing.first().let { stampFile(it).delete(); it.delete() }
        val ts = System.currentTimeMillis()
        val id = UUID.randomUUID().toString()
        val tmp = File(imgDir(context), "$ts-$id.tmp")
        val size = app.clipsync.desktop.clip.ClipImageBridge.clipToPng(context, tmp, uri)
        if (size <= 0 || size > MAX_IMAGE_BYTES) {
            tmp.delete()
            return false
        }
        val png = File(imgDir(context), "$ts-$id.png")
        return runCatching {
            stampFile(png).writeText(stamp)
            tmp.renameTo(png)
        }.getOrDefault(false)
    }

    @Synchronized
    fun pruneImages(context: Context) {
        val now = System.currentTimeMillis()
        for (f in images(context)) {
            if (now - tsOf(f) > TTL_MS) { stampFile(f).delete(); f.delete() }
        }
        imgDir(context).listFiles { f -> f.name.endsWith(".tmp") }?.forEach { it.delete() }
    }

    /** JSON `[{id, path, stamp, ts}]` das imagens pendentes, da mais antiga para a mais nova. */
    @Synchronized
    fun listImagesJson(context: Context): String {
        pruneImages(context)
        val arr = JSONArray()
        for (f in images(context)) {
            val stamp = runCatching { stampFile(f).readText() }.getOrNull() ?: continue
            arr.put(
                JSONObject()
                    .put("id", f.name.substringAfter("-").removeSuffix(".png"))
                    .put("path", f.absolutePath)
                    .put("stamp", stamp)
                    .put("ts", tsOf(f)),
            )
        }
        return arr.toString()
    }

    /** O núcleo já moveu o PNG para o histórico; apaga o que sobrou. */
    @Synchronized
    fun removeImage(context: Context, id: String) {
        imgDir(context).listFiles { f -> f.name.substringAfter("-").substringBeforeLast(".") == id }?.forEach { it.delete() }
    }
}
