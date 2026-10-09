package app.clipsync.desktop.quick

import android.app.StatusBarManager
import android.appwidget.AppWidgetManager
import android.content.ComponentName
import android.content.Context
import android.graphics.drawable.Icon
import android.os.Build
import app.clipsync.desktop.R

/** Botões "Adicionar bloco" / "Adicionar widget" dos Ajustes. Retornam `requested` ou `unsupported`. */
object QuickAdd {
    /** Android 13+: o sistema mostra o diálogo de confirmação; a resposta não muda nada na UI. */
    @JvmStatic
    fun addTile(context: Context): String {
        if (Build.VERSION.SDK_INT < 33) return "unsupported"
        val sbm = context.getSystemService(StatusBarManager::class.java) ?: return "unsupported"
        return runCatching {
            sbm.requestAddTileService(
                ComponentName(context, SendTileService::class.java),
                context.getString(R.string.tile_label),
                Icon.createWithResource(context, R.drawable.ic_stat_clipsync),
                context.mainExecutor,
            ) { }
            "requested"
        }.getOrDefault("unsupported")
    }

    @JvmStatic
    fun addWidget(context: Context): String {
        val mgr = context.getSystemService(AppWidgetManager::class.java) ?: return "unsupported"
        if (!mgr.isRequestPinAppWidgetSupported) return "unsupported"
        val provider = ComponentName(context, ClipWidgetProvider::class.java)
        return if (runCatching { mgr.requestPinAppWidget(provider, null, null) }.getOrDefault(false)) "requested" else "unsupported"
    }
}
