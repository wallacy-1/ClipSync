package app.clipsync.desktop.quick

import android.app.PendingIntent
import android.content.Intent
import android.os.Build
import android.service.quicksettings.Tile
import android.service.quicksettings.TileService
import app.clipsync.desktop.R

/** Bloco de Ajustes rápidos "Enviar clipboard": abre a [ClipSendActivity] e recolhe o painel. */
class SendTileService : TileService() {
    override fun onStartListening() {
        val tile = qsTile ?: return
        val status = StatusStore.read(this)
        val paired = status?.optInt("paired") ?: 0
        val connected = status?.optInt("connected") ?: 0
        val subtitle = when {
            paired == 0 -> getString(R.string.tile_no_device)
            status?.optBoolean("paused") == true -> getString(R.string.tile_paused)
            connected > 0 -> getString(R.string.tile_connected, connected)
            else -> getString(R.string.tile_offline)
        }
        tile.state = if (paired == 0) Tile.STATE_UNAVAILABLE else Tile.STATE_INACTIVE
        if (Build.VERSION.SDK_INT >= 29) tile.subtitle = subtitle
        tile.updateTile()
    }

    override fun onClick() {
        val intent = ClipSendActivity.intentFor(this, ClipSendActivity.ACTION_SEND_CLIPBOARD)
        // Tela bloqueada: pede o desbloqueio antes de abrir a Activity.
        if (isLocked) unlockAndRun { launch(intent) } else launch(intent)
    }

    private fun launch(intent: Intent) {
        if (Build.VERSION.SDK_INT >= 34) {
            // API 34+: só a versão com PendingIntent é aceita.
            val pi = PendingIntent.getActivity(this, 0, intent, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
            startActivityAndCollapse(pi)
        } else {
            @Suppress("DEPRECATION")
            startActivityAndCollapse(intent)
        }
    }
}
