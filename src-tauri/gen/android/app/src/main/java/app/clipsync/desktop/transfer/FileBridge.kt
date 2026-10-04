package app.clipsync.desktop.transfer

import android.app.Activity
import android.content.ContentResolver
import android.content.ContentValues
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.os.Handler
import android.os.Looper
import android.os.StatFs
import android.provider.MediaStore
import android.provider.OpenableColumns
import android.webkit.MimeTypeMap
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.io.FileInputStream
import java.io.FileOutputStream

/** Ponte JNI para as APIs de arquivo do Android (SAF, ContentResolver, MediaStore, espaço livre). Sem regra de protocolo/estado. */
object FileBridge {
    const val PICK_REQUEST = 0x4C1F // "CLIF"
    private const val RELATIVE_DIR = "ClipSync"

    @Volatile private var pickResult: String? = null
    @Volatile private var pickPending = false

    /**
     * Abre o seletor de documentos do sistema. Resultado chega em
     * [onPickResult] (via `MainActivity.onActivityResult`) e é lido pelo Rust
     * com [takePickResult]. Retorna `false` se não há Activity para abrir o seletor.
     */
    @JvmStatic
    fun pickFile(context: Context): Boolean {
        val activity = context as? Activity ?: return false
        pickResult = null
        pickPending = true
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = "*/*"
            putExtra(Intent.EXTRA_ALLOW_MULTIPLE, true)
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION)
        }
        Handler(Looper.getMainLooper()).post {
            runCatching { activity.startActivityForResult(intent, PICK_REQUEST) }.onFailure {
                pickPending = false
                pickResult = JSONObject().put("error", it.message ?: "picker").toString()
            }
        }
        return true
    }

    /** Chamado pela Activity. Persiste a permissão de leitura para retomar depois. */
    fun onPickResult(context: Context, resultCode: Int, data: Intent?) {
        val uris = mutableListOf<Uri>()
        if (resultCode == Activity.RESULT_OK && data != null) {
            val clip = data.clipData
            if (clip != null) for (i in 0 until clip.itemCount) clip.getItemAt(i).uri?.let { uris += it }
            else data.data?.let { uris += it }
        }
        if (uris.isEmpty()) {
            pickResult = JSONObject().put("cancelled", true).toString()
            pickPending = false
            return
        }
        val files = JSONArray()
        for (uri in uris) {
            runCatching {
                context.contentResolver.takePersistableUriPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION)
            }
            files.put(JSONObject(describe(context, uri.toString())))
        }
        pickResult = JSONObject().put("files", files).toString()
        pickPending = false
    }

    /** JSON do resultado (`{files: [{uri,name,size,mime}]}` | `{cancelled}` | `{error}`), ou null se ainda aguardando. */
    @JvmStatic
    fun takePickResult(): String? {
        val r = pickResult ?: return null
        pickResult = null
        return r
    }

    /** `{name, size, mime}` de uma `content://` URI (size = -1 se desconhecido). */
    @JvmStatic
    fun describe(context: Context, uri: String): String {
        val u = Uri.parse(uri)
        val cr = context.contentResolver
        var name: String? = null
        var size: Long = -1
        cr.query(u, arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE), null, null, null)?.use { c ->
            if (c.moveToFirst()) {
                val ni = c.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                val si = c.getColumnIndex(OpenableColumns.SIZE)
                if (ni >= 0) name = c.getString(ni)
                if (si >= 0 && !c.isNull(si)) size = c.getLong(si)
            }
        }
        val mime = cr.getType(u)
        return JSONObject()
            .put("uri", uri)
            .put("name", name ?: u.lastPathSegment ?: "arquivo")
            .put("size", size)
            .put("mime", mime ?: JSONObject.NULL)
            .toString()
    }

    /**
     * Abre a URI para leitura e **desanexa** o descritor: o Rust passa a ser o dono
     * (`File::from_raw_fd`). Retorna `[fd, size]`; `fd < 0` = falha (permissão
     * revogada, arquivo removido). Nenhum byte é copiado aqui.
     */
    @JvmStatic
    fun openRead(context: Context, uri: String): LongArray {
        return try {
            val pfd = context.contentResolver.openFileDescriptor(Uri.parse(uri), "r") ?: return longArrayOf(-1, -1)
            val size = pfd.statSize
            val fd = pfd.detachFd()
            longArrayOf(fd.toLong(), size)
        } catch (_: Exception) {
            longArrayOf(-1, -1)
        }
    }

    @JvmStatic
    fun freeSpace(@Suppress("UNUSED_PARAMETER") context: Context, path: String): Long =
        runCatching { StatFs(path).availableBytes }.getOrDefault(-1L)

    /** MIME só como metadado para o MediaStore: extensão local primeiro, nunca o do peer para decidir nada. */
    private fun mimeFor(name: String, offered: String?): String {
        val ext = name.substringAfterLast('.', "").lowercase()
        val byExt = if (ext.isNotEmpty()) MimeTypeMap.getSingleton().getMimeTypeFromExtension(ext) else null
        return byExt ?: offered?.takeIf { it.contains('/') && !it.contains(' ') } ?: "application/octet-stream"
    }

    /**
     * Publica um `.part` já verificado em Downloads/ClipSync via MediaStore:
     * cria a linha com `IS_PENDING=1` (invisível), copia o conteúdo, só então
     * `IS_PENDING=0`. Em erro, apaga a linha pendente. Retorna `{uri, name}`.
     * O `.part` é apagado só depois do sucesso.
     */
    @JvmStatic
    fun publishToDownloads(context: Context, partPath: String, displayName: String, mime: String?): String {
        val part = File(partPath)
        require(part.isFile) { "part inexistente" }
        val cr = context.contentResolver
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            val values = ContentValues().apply {
                put(MediaStore.Downloads.DISPLAY_NAME, displayName)
                put(MediaStore.Downloads.MIME_TYPE, mimeFor(displayName, mime))
                put(MediaStore.Downloads.RELATIVE_PATH, Environment.DIRECTORY_DOWNLOADS + "/" + RELATIVE_DIR)
                put(MediaStore.Downloads.IS_PENDING, 1)
            }
            val collection = MediaStore.Downloads.getContentUri(MediaStore.VOLUME_EXTERNAL_PRIMARY)
            val item = cr.insert(collection, values) ?: throw IllegalStateException("MediaStore.insert falhou")
            try {
                cr.openOutputStream(item, "w")!!.use { out -> FileInputStream(part).use { it.copyTo(out, 1 shl 20) } }
                val done = ContentValues().apply { put(MediaStore.Downloads.IS_PENDING, 0) }
                cr.update(item, done, null, null)
            } catch (e: Exception) {
                runCatching { cr.delete(item, null, null) }
                throw e
            }
            val finalName = queryName(cr, item) ?: displayName
            part.delete()
            return JSONObject().put("uri", item.toString()).put("name", finalName).toString()
        } else {
            // API 26–28: sem MediaStore.Downloads; diretório externo privado do app (sem permissão).
            val dir = File(context.getExternalFilesDir(Environment.DIRECTORY_DOWNLOADS), RELATIVE_DIR).apply { mkdirs() }
            var target = File(dir, displayName)
            var n = 2
            while (target.exists()) {
                val dot = displayName.lastIndexOf('.')
                val stem = if (dot > 0) displayName.substring(0, dot) else displayName
                val ext = if (dot > 0) displayName.substring(dot) else ""
                target = File(dir, "$stem ($n)$ext"); n++
            }
            FileInputStream(part).use { i -> FileOutputStream(target).use { o -> i.copyTo(o, 1 shl 20) } }
            part.delete()
            return JSONObject().put("uri", Uri.fromFile(target).toString()).put("name", target.name).toString()
        }
    }

    private fun queryName(cr: ContentResolver, uri: Uri): String? =
        cr.query(uri, arrayOf(MediaStore.MediaColumns.DISPLAY_NAME), null, null, null)?.use { c ->
            if (c.moveToFirst()) c.getString(0) else null
        }

    /**
     * Reconciliação na subida: linhas pendentes nossas em Downloads (processo morreu
     * durante a publicação) são apagadas — o `.part` ainda existe e a transferência
     * será concluída/publicada de novo. Retorna quantas foram removidas.
     */
    @JvmStatic
    fun cleanupPending(context: Context): Int {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) return 0
        val cr = context.contentResolver
        val collection = MediaStore.Downloads.getContentUri(MediaStore.VOLUME_EXTERNAL_PRIMARY)
        val uri = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            collection.buildUpon().appendQueryParameter("includePending", "1").build()
        } else {
            @Suppress("DEPRECATION") MediaStore.setIncludePending(collection)
        }
        var removed = 0
        runCatching {
            cr.query(
                uri,
                arrayOf(MediaStore.MediaColumns._ID),
                "${MediaStore.MediaColumns.IS_PENDING}=1 AND ${MediaStore.MediaColumns.OWNER_PACKAGE_NAME}=?",
                arrayOf(context.packageName),
                null,
            )?.use { c ->
                while (c.moveToNext()) {
                    val id = c.getLong(0)
                    if (cr.delete(android.content.ContentUris.withAppendedId(collection, id), null, null) > 0) removed++
                }
            }
        }
        return removed
    }
}
