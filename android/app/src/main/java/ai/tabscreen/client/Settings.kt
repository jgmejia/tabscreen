package ai.tabscreen.client

import android.content.Context
import android.graphics.Color

/**
 * How the decoded video is presented on screen. NATIVE avoids the extra
 * upscale-stretch pass FILL adds on top of the encoder's own downscale
 * (see docs/adr/0035) -- sharper, at the cost of a visible frame/border
 * around the image.
 */
enum class DisplayMode { FILL, NATIVE }

/**
 * User-facing display preferences, persisted locally (this device only --
 * nothing here is negotiated with or known to the host). Opened via a
 * long-press on Volume Down (MainActivity), since every touch on this
 * app's window is otherwise forwarded to the host as remote input and
 * can't double as a local settings gesture.
 */
object Settings {
    private const val PREFS_NAME = "tabscreen_settings"
    private const val KEY_DISPLAY_MODE = "display_mode"
    private const val KEY_FRAME_COLOR = "frame_color"

    fun displayMode(context: Context): DisplayMode {
        val raw = prefs(context).getString(KEY_DISPLAY_MODE, DisplayMode.FILL.name)
        return runCatching { DisplayMode.valueOf(raw!!) }.getOrDefault(DisplayMode.FILL)
    }

    fun setDisplayMode(context: Context, mode: DisplayMode) {
        prefs(context).edit().putString(KEY_DISPLAY_MODE, mode.name).apply()
    }

    fun frameColor(context: Context): Int =
        prefs(context).getInt(KEY_FRAME_COLOR, Color.BLACK)

    fun setFrameColor(context: Context, color: Int) {
        prefs(context).edit().putInt(KEY_FRAME_COLOR, color).apply()
    }

    private fun prefs(context: Context) =
        context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
}
