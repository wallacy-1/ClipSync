package app.clipsync.desktop

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.os.Process

/**
 * "Tentar de novo" com a inicialização travada: roda no processo `:restart`, mata o processo
 * principal (núcleo Rust incluso) e reabre a MainActivity do zero.
 */
class RestartActivity : Activity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)
    val pid = intent.getIntExtra(EXTRA_PID, -1)
    if (pid > 0 && pid != Process.myPid()) Process.killProcess(pid)
    startActivity(
      Intent(this, MainActivity::class.java)
        .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK),
    )
    finish()
    Runtime.getRuntime().exit(0)
  }

  companion object {
    private const val EXTRA_PID = "pid"

    fun start(ctx: Context) {
      ctx.startActivity(
        Intent(ctx, RestartActivity::class.java)
          .putExtra(EXTRA_PID, Process.myPid())
          .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
      )
    }
  }
}
