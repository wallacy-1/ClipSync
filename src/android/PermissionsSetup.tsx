// Tudo o que o app pode pedir numa tela só. Nada é ativado sozinho: cada item leva ao diálogo/tela do sistema.
// Teclado e "Manter conectado" são exigidos para parear: sem eles o Android só deixa sincronizar com o app na tela.
import { useEffect } from "react";
import { Bell, Check, Keyboard as KeyboardIcon, Shield, Transfers } from "../components/Icons";
import { Toggle } from "../components/ui";
import { useT } from "../i18n";
import { useApp } from "../store/app";
import { imeState } from "./KeyboardView";

export function PermissionsSetup({ children }: { children: (ready: boolean) => React.ReactNode }) {
  const t = useT();
  const notifyPerm = useApp((s) => s.notifyPerm);
  const requestNotifyPerm = useApp((s) => s.requestNotifyPerm);
  const ime = useApp((s) => s.ime);
  const refreshIme = useApp((s) => s.refreshIme);
  const openImeSettings = useApp((s) => s.openImeSettings);
  const showImePicker = useApp((s) => s.showImePicker);
  const keepAlive = useApp((s) => s.settings?.keepAlive ?? false);
  const saveSettings = useApp((s) => s.saveSettings);

  // Diálogos e telas são do sistema: re-lê o estado quando o usuário volta.
  useEffect(() => {
    void refreshIme();
    const onVis = () => { if (document.visibilityState === "visible") void refreshIme(); };
    document.addEventListener("visibilitychange", onVis);
    return () => document.removeEventListener("visibilitychange", onVis);
  }, [refreshIme]);

  const kb = imeState(ime);
  const ready = kb === "active" && keepAlive;
  const showNotify = notifyPerm !== null && notifyPerm !== "unsupported";

  return (
    <div className="onboard">
      <div className="logo"><Shield size={36} /></div>
      <h3>{t("android.perms.title")}</h3>
      <p>{t("android.perms.body")}</p>

      <div className="perm-list">
        {showNotify && (
          <PermRow
            Icon={Bell}
            title={t("android.perms.notifyTitle")}
            help={t("android.perms.notifyHelp")}
            done={notifyPerm === "granted"}
            action={notifyPerm === "blocked" ? t("android.perms.openSettings") : t("android.perms.allow")}
            onAction={() => void requestNotifyPerm()}
          />
        )}
        <PermRow
          Icon={KeyboardIcon}
          title={t("android.perms.kbTitle")}
          help={kb === "enabled" ? t("android.perms.kbHelpEnabled") : t("android.perms.kbHelp")}
          required
          done={kb === "active"}
          action={kb === "enabled" ? t("android.perms.kbSelect") : t("android.perms.kbEnable")}
          onAction={() => void (kb === "enabled" ? showImePicker() : openImeSettings())}
        />
        <div className={`perm-row${keepAlive ? " done" : ""}`}>
          <span className="perm-icon"><Transfers size={18} /></span>
          <div className="perm-body">
            <div className="perm-title">{t("android.perms.bgTitle")} {!keepAlive && <span className="perm-tag">{t("android.perms.required")}</span>}</div>
            <div className="perm-help">{t("android.perms.bgHelp")}</div>
          </div>
          <Toggle on={keepAlive} onChange={(v) => void saveSettings({ keepAlive: v })} />
        </div>
      </div>
      <p className="perm-note">{t(ready ? "android.perms.noteDone" : "android.perms.note")}</p>

      <div className="grow" />
      {children(ready)}
    </div>
  );
}

function PermRow({ Icon, title, help, required, done, action, onAction }: {
  Icon: typeof Bell;
  title: string;
  help: string;
  required?: boolean;
  done: boolean;
  action: string;
  onAction: () => void;
}) {
  const t = useT();
  return (
    <div className={`perm-row${done ? " done" : ""}`}>
      <span className="perm-icon">{done ? <Check size={18} /> : <Icon size={18} />}</span>
      <div className="perm-body">
        <div className="perm-title">{title} {required && !done && <span className="perm-tag">{t("android.perms.required")}</span>}</div>
        <div className="perm-help">{help}</div>
      </div>
      {done
        ? <span className="perm-ok">{t("android.perms.done")}</span>
        : <button className="btn btn-secondary perm-btn" onClick={onAction}>{action}</button>}
    </div>
  );
}
