package app.clipsync.desktop.quick

import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.text.format.DateFormat
import android.view.View
import android.widget.RemoteViews
import app.clipsync.desktop.MainActivity
import app.clipsync.desktop.R
import java.util.Date

/**
 * Widget da tela inicial: status, prévia do último item e os botões Enviar / Copiar último.
 * RemoteViews não tem relógio vivo, então mostra o horário absoluto do item; quem redesenha é o
 * [StatusStore] (a cada status empurrado pelo Rust) e o sistema (`onUpdate`).
 */
class ClipWidgetProvider : AppWidgetProvider() {
    override fun onUpdate(context: Context, manager: AppWidgetManager, appWidgetIds: IntArray) {
        appWidgetIds.forEach { render(context, manager, it) }
    }

    // Redimensionar 4×1 ↔ 4×2 muda quantas linhas da prévia cabem.
    override fun onAppWidgetOptionsChanged(context: Context, manager: AppWidgetManager, appWidgetId: Int, newOptions: Bundle?) {
        render(context, manager, appWidgetId)
    }

    companion object {
        private const val REQ_OPEN = 1
        private const val REQ_SEND = 2
        private const val REQ_COPY = 3

        /** A partir desta altura (dp) o widget está em 4×2: prévia com 2 linhas. */
        private const val TALL_DP = 110

        fun refreshAll(context: Context) {
            val mgr = AppWidgetManager.getInstance(context)
            mgr.getAppWidgetIds(ComponentName(context, ClipWidgetProvider::class.java)).forEach { render(context, mgr, it) }
        }

        private fun render(context: Context, manager: AppWidgetManager, id: Int) {
            val status = StatusStore.read(context)
            val views = RemoteViews(context.packageName, R.layout.widget_clip)

            val paired = status?.optInt("paired") ?: 0
            val connected = status?.optInt("connected") ?: 0
            val paused = status?.optBoolean("paused") == true
            val (statusText, dot) = when {
                status == null -> context.getString(R.string.widget_open_app) to R.color.widget_dot_off
                paired == 0 -> context.getString(R.string.widget_no_device) to R.color.widget_dot_off
                paused -> pausedText(context, status.optLong("pausedUntil", 0)) to R.color.widget_dot_warn
                connected > 0 -> context.resources.getQuantityString(R.plurals.widget_connected, connected, connected) to R.color.widget_dot_ok
                else -> context.getString(R.string.widget_offline) to R.color.widget_dot_off
            }
            views.setTextViewText(R.id.widget_status, statusText)
            views.setInt(R.id.widget_dot, "setColorFilter", context.getColor(dot))

            val last = status?.optJSONObject("last")
            if (status == null) {
                views.setViewVisibility(R.id.widget_preview, View.GONE)
                views.setViewVisibility(R.id.widget_meta, View.GONE)
            } else {
                views.setViewVisibility(R.id.widget_preview, View.VISIBLE)
                val preview = when {
                    last == null -> context.getString(R.string.widget_nothing)
                    last.optBoolean("sensitive") && last.optString("preview").isEmpty() -> context.getString(R.string.widget_sensitive)
                    last.optString("type") == "image" -> context.getString(R.string.widget_image)
                    else -> last.optString("preview")
                }
                views.setTextViewText(R.id.widget_preview, preview)
                val tall = (manager.getAppWidgetOptions(id).getInt(AppWidgetManager.OPTION_APPWIDGET_MIN_HEIGHT, 0)) >= TALL_DP
                views.setInt(R.id.widget_preview, "setMaxLines", if (tall) 2 else 1)
                if (last == null) {
                    views.setViewVisibility(R.id.widget_meta, View.GONE)
                } else {
                    val time = DateFormat.getTimeFormat(context).format(Date(last.optLong("at")))
                    val from = last.optString("from").ifBlank { context.getString(R.string.widget_other_device) }
                    val meta = if (last.optBoolean("local")) context.getString(R.string.widget_meta_local, time) else context.getString(R.string.widget_meta_remote, from, time)
                    views.setTextViewText(R.id.widget_meta, meta)
                    views.setViewVisibility(R.id.widget_meta, View.VISIBLE)
                }
            }

            val flags = PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
            views.setOnClickPendingIntent(
                R.id.widget_root,
                PendingIntent.getActivity(context, REQ_OPEN, Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK), flags),
            )
            views.setOnClickPendingIntent(
                R.id.widget_send,
                PendingIntent.getActivity(context, REQ_SEND, ClipSendActivity.intentFor(context, ClipSendActivity.ACTION_SEND_CLIPBOARD), flags),
            )
            views.setOnClickPendingIntent(
                R.id.widget_copy,
                PendingIntent.getActivity(context, REQ_COPY, ClipSendActivity.intentFor(context, ClipSendActivity.ACTION_COPY_LATEST), flags),
            )
            manager.updateAppWidget(id, views)
        }

        private fun pausedText(context: Context, until: Long): String =
            if (until > 0) context.getString(R.string.widget_paused_until, DateFormat.getTimeFormat(context).format(Date(until)))
            else context.getString(R.string.widget_paused)
    }
}
