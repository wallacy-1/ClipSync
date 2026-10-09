package app.clipsync.desktop

import android.app.Activity
import androidx.core.view.WindowCompat
import java.lang.ref.WeakReference

/**
 * Cor dos ícones das barras de status/navegação conforme o tema do app (não o do sistema).
 * Chamado pelo Rust (`set_system_bars`) fora da thread de UI; a Activity atual é registrada
 * em `MainActivity.onCreate`/`onResume`.
 */
object SystemBars {
    private var activity: WeakReference<Activity>? = null
    @Volatile
    private var dark: Boolean? = null

    @JvmStatic
    fun attach(a: Activity) {
        activity = WeakReference(a)
        dark?.let { apply(a, it) }
    }

    @JvmStatic
    fun set(dark: Boolean) {
        this.dark = dark
        activity?.get()?.let { apply(it, dark) }
    }

    private fun apply(a: Activity, dark: Boolean) {
        a.runOnUiThread {
            WindowCompat.getInsetsController(a.window, a.window.decorView).apply {
                isAppearanceLightStatusBars = !dark
                isAppearanceLightNavigationBars = !dark
            }
        }
    }
}
