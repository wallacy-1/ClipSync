package app.clipsync.desktop.ime

import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import android.view.inputmethod.InputMethodManager
import org.json.JSONArray
import org.json.JSONObject

object ImeBridge {
    private fun component(context: Context) =
        ComponentName(context, ClipSyncImeService::class.java)

    @JvmStatic
    fun isEnabled(context: Context): Boolean {
        val imm = context.getSystemService(Context.INPUT_METHOD_SERVICE) as? InputMethodManager ?: return false
        val me = component(context)
        return imm.enabledInputMethodList.any { it.component == me }
    }

    @JvmStatic
    fun isSelected(context: Context): Boolean {
        val current = Settings.Secure.getString(context.contentResolver, Settings.Secure.DEFAULT_INPUT_METHOD)
            ?: return false
        val me = component(context)
        return ComponentName.unflattenFromString(current) == me
    }

    /** `{"enabled":bool,"selected":bool,"theme":"id","themes":[{"id","label"}]}` */
    @JvmStatic
    fun status(context: Context): String {
        val themes = JSONArray()
        for (t in KeyboardThemes.all) {
            themes.put(JSONObject().put("id", t.id).put("label", t.label))
        }
        return JSONObject()
            .put("enabled", isEnabled(context))
            .put("selected", isSelected(context))
            .put("theme", ImePrefs.themeId(context))
            .put("themes", themes)
            .toString()
    }

    @JvmStatic
    fun openSettings(context: Context) {
        val intent = Intent(Settings.ACTION_INPUT_METHOD_SETTINGS).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        Handler(Looper.getMainLooper()).post { context.startActivity(intent) }
    }

    @JvmStatic
    fun showPicker(context: Context) {
        val imm = context.getSystemService(Context.INPUT_METHOD_SERVICE) as? InputMethodManager ?: return
        Handler(Looper.getMainLooper()).post { imm.showInputMethodPicker() }
    }

    @JvmStatic
    fun setTheme(context: Context, id: String) {
        ImePrefs.setThemeId(context, id)
    }
}
