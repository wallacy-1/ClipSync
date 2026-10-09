import { useEffect, useState } from "react";
import { useT } from "../i18n";
import { Keyboard as KeyboardIcon } from "../components/Icons";
import { AboutRows, AppearanceRows, PrivacyRows, SyncRows } from "../components/SettingsRows";
import { Confirm } from "../components/ui";
import { api, errorText } from "../lib/ipc";
import { useApp } from "../store/app";
import { KeyboardView, imeState } from "./KeyboardView";

export function SettingsView({ initialSub }: { initialSub?: "keyboard" | null }) {
  const t = useT();
  const clearHistory = useApp((s) => s.clearHistory);
  const platform = useApp((s) => s.platform);
  const ime = useApp((s) => s.ime);
  const refreshIme = useApp((s) => s.refreshIme);
  const showToast = useApp((s) => s.showToast);
  const [confirm, setConfirm] = useState(false);
  const [sub, setSub] = useState<"keyboard" | null>(initialSub ?? null);

  useEffect(() => { if (platform === "android") void refreshIme(); }, [platform, refreshIme]);

  if (sub === "keyboard") return <KeyboardView onBack={() => setSub(null)} />;

  const quickAdd = async (what: "tile" | "widget") => {
    try {
      const r = await (what === "tile" ? api.quickAddTile() : api.quickAddWidget());
      if (r === "unsupported") showToast(t(what === "tile" ? "android.settings.tileUnsupported" : "android.settings.widgetUnsupported"));
    } catch (e) {
      showToast(errorText(e), "error");
    }
  };

  const state = imeState(ime);
  const imeSummary = t(state === "active" ? "android.settings.imeActive" : state === "enabled" ? "android.settings.imeEnabled" : "android.settings.imeOff");

  return (
    <>
      <div className="and-head" style={{ padding: "16px 18px 14px" }}>
        <span className="page-title">{t("android.settings.title")}</span>
      </div>
      <div className="and-scroll">
        <div className="and-settings">
          <div>
            <div className="section-title">{t("settings.sync")}</div>
            <div className="group"><SyncRows /></div>
          </div>
          {platform === "android" && (
            <div>
              <div className="section-title">{t("android.settings.keyboard")}</div>
              <div className="group">
                <button className="setting-row kb-link" onClick={() => setSub("keyboard")}>
                  <span className={`kb-dot ${state}`}><KeyboardIcon size={18} /></span>
                  <div className="info">
                    <div className="label">ClipSync Keyboard</div>
                    <div className="help">{imeSummary}</div>
                  </div>
                  <span className="chevron">›</span>
                </button>
              </div>
            </div>
          )}
          {platform === "android" && (
            <div>
              <div className="section-title">{t("android.settings.shortcuts")}</div>
              <div className="kb-actions">
                <p className="kb-note">{t("android.settings.shortcutsHelp")}</p>
                <button className="btn btn-secondary btn-block" onClick={() => void quickAdd("tile")}>{t("android.settings.addTile")}</button>
                <button className="btn btn-secondary btn-block" onClick={() => void quickAdd("widget")}>{t("android.settings.addWidget")}</button>
              </div>
            </div>
          )}
          <div>
            <div className="section-title">{t("settings.privacy")}</div>
            <div className="group"><PrivacyRows /></div>
          </div>
          <div>
            <div className="section-title">{t("settings.appearance")}</div>
            <div className="group"><AppearanceRows /></div>
          </div>
          <button className="btn btn-secondary btn-block" onClick={() => setConfirm(true)}>{t("settings.clearNow")}</button>
          <div>
            <div className="section-title">{t("settings.about")}</div>
            <div className="group"><AboutRows /></div>
          </div>
        </div>
      </div>
      {confirm && (
        <Confirm
          title={t("settings.clearTitle")}
          body={t("android.settings.clearBody")}
          confirmLabel={t("settings.clear")}
          danger
          onCancel={() => setConfirm(false)}
          onConfirm={() => { setConfirm(false); void clearHistory(true); }}
        />
      )}
    </>
  );
}
