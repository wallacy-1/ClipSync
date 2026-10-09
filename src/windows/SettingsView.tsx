import { useState } from "react";
import { Shield } from "../components/Icons";
import { useT } from "../i18n";
import { AboutRows, AppearanceRows, PrivacyRows, ShortcutRows, SyncRows } from "../components/SettingsRows";
import { Confirm } from "../components/ui";
import { useApp } from "../store/app";

export function SettingsView() {
  const t = useT();
  const clearHistory = useApp((s) => s.clearHistory);
  const [confirm, setConfirm] = useState(false);

  return (
    <div className="main">
      <div className="page">
        <div className="settings">
          <div>
            <div className="section-title">{t("settings.sync")}</div>
            <SyncRows />
          </div>
          <div>
            <div className="section-title">{t("settings.privacy")}</div>
            <PrivacyRows />
          </div>
          <div>
            <div className="section-title">{t("settings.appearance")}</div>
            <AppearanceRows />
          </div>
          <div>
            <div className="section-title">{t("settings.shortcuts")}</div>
            <ShortcutRows />
          </div>
          <div className="security-note">
            <Shield size={20} style={{ color: "var(--color-accent-2-800)" }} />
            <span>{t("settings.securityNote")}</span>
          </div>
          <div>
            <button className="btn btn-secondary" onClick={() => setConfirm(true)}>{t("settings.clearNow")}</button>
          </div>
          <div>
            <div className="section-title">{t("settings.about")}</div>
            <AboutRows />
          </div>
        </div>
      </div>
      {confirm && (
        <Confirm
          title={t("settings.clearTitle")}
          body={t("settings.clearBody")}
          confirmLabel={t("settings.clear")}
          danger
          onCancel={() => setConfirm(false)}
          onConfirm={() => { setConfirm(false); void clearHistory(true); }}
        />
      )}
    </div>
  );
}
