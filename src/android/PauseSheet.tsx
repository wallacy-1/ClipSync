import { useT } from "../i18n";
import { PAUSE_OPTIONS, pausedUntilText } from "../components/pause";
import { useApp } from "../store/app";

export function PauseSheet({ onClose }: { onClose: () => void }) {
  const t = useT();
  const sync = useApp((s) => s.sync);
  const setPaused = useApp((s) => s.setPaused);

  return (
    <div className="sheet-backdrop" onClick={onClose}>
      <div className="sheet" onClick={(e) => e.stopPropagation()}>
        <div className="handle" />
        {sync.paused ? (
          <>
            <h3>{t("android.pause.pausedTitle")}</h3>
            <div className="sub">{t("android.pause.pausedSub", { until: pausedUntilText(sync.pausedUntil) })}</div>
            <button className="btn btn-primary btn-block cta" onClick={async () => { await setPaused(false); onClose(); }}>{t("android.pause.resumeNow")}</button>
          </>
        ) : (
          <>
            <h3>{t("android.pause.title")}</h3>
            <div className="sub">{t("android.pause.sub")}</div>
            <div className="targets">
              {PAUSE_OPTIONS.map((o) => (
                <button className="btn btn-secondary btn-block" key={o.label} style={{ height: 48 }} onClick={async () => { await setPaused(true, o.minutes); onClose(); }}>
                  {o.label}
                </button>
              ))}
              <button className="btn btn-ghost btn-block" onClick={onClose}>{t("common.cancel")}</button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
