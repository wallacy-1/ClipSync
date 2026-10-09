// Nunca ativa nem troca o teclado sozinho: só mostra o estado do IME e leva às telas do sistema.
import { useEffect } from "react";
import { useT, type Key } from "../i18n";
import { Back, Check, Keyboard as KeyboardIcon } from "../components/Icons";
import { useApp } from "../store/app";

export type ImeState = "off" | "enabled" | "active";

export function imeState(ime: { enabled: boolean; selected: boolean } | null): ImeState {
  if (!ime || !ime.enabled) return "off";
  return ime.selected ? "active" : "enabled";
}

export function KeyboardView({ onBack }: { onBack: () => void }) {
  const t = useT();
  const ime = useApp((s) => s.ime);
  const refreshIme = useApp((s) => s.refreshIme);
  const setImeTheme = useApp((s) => s.setImeTheme);
  const openImeSettings = useApp((s) => s.openImeSettings);
  const showImePicker = useApp((s) => s.showImePicker);

  // Re-lê o estado ao abrir e sempre que o usuário volta das Configurações do Android.
  useEffect(() => {
    void refreshIme();
    const onVis = () => { if (document.visibilityState === "visible") void refreshIme(); };
    document.addEventListener("visibilitychange", onVis);
    return () => document.removeEventListener("visibilitychange", onVis);
  }, [refreshIme]);

  const state = imeState(ime);

  return (
    <>
      <div className="and-head" style={{ padding: "16px 18px 14px" }}>
        <button className="icon-btn lg" style={{ marginLeft: -10 }} onClick={onBack} aria-label={t("android.back")}><Back size={22} /></button>
        <span className="page-title">{t("android.keyboard.title")}</span>
      </div>
      <div className="and-scroll">
        <div className="and-settings">
          <div className={`kb-status ${state}`}>
            <div className="kb-status-icon"><KeyboardIcon size={26} /></div>
            <div className="kb-status-body">
              <div className="kb-status-title">ClipSync Keyboard</div>
              {state === "active" && <div className="kb-status-line ok"><Check size={14} /> {t("android.keyboard.active")}</div>}
              {state === "enabled" && <div className="kb-status-line warn">{t("android.keyboard.enabledWarn")}</div>}
              {state === "off" && <div className="kb-status-line">{t("android.keyboard.off")}</div>}
              <p className="kb-status-help">
                {state === "active" && t("android.keyboard.helpActive")}
                {state === "enabled" && t("android.keyboard.helpEnabled")}
                {state === "off" && t("android.keyboard.helpOff")}
              </p>
            </div>
          </div>

          <div className="kb-actions">
            {state === "off" && (
              <button className="btn btn-primary btn-block" onClick={() => void openImeSettings()}>{t("android.keyboard.enable")}</button>
            )}
            {state === "enabled" && (
              <>
                <button className="btn btn-primary btn-block" onClick={() => void showImePicker()}>{t("android.keyboard.select")}</button>
                <button className="btn btn-secondary btn-block" onClick={() => void openImeSettings()}>{t("android.keyboard.settings")}</button>
              </>
            )}
            {state === "active" && (
              <button className="btn btn-secondary btn-block" onClick={() => void openImeSettings()}>{t("android.keyboard.settings")}</button>
            )}
            <p className="kb-note">{t("android.keyboard.note")}</p>
          </div>

          <div>
            <div className="section-title">{t("android.keyboard.appearance")}</div>
            <div className="group">
              {(ime?.themes ?? []).map((th) => {
                const on = ime?.theme === th.id;
                return (
                  <button key={th.id} className={`kb-theme-row${on ? " on" : ""}`} onClick={() => void setImeTheme(th.id)}>
                    <span className={`radio${on ? " on" : ""}`} />
                    <span className="kb-theme-label">{th.id in THEME_KEYS ? t(THEME_KEYS[th.id].label) : th.label}</span>
                    <span className="kb-theme-help">{th.id in THEME_KEYS ? t(THEME_KEYS[th.id].help) : ""}</span>
                  </button>
                );
              })}
              {!ime?.themes?.length && <div className="setting-row"><div className="info"><div className="help">{t("android.keyboard.noThemes")}</div></div></div>}
            </div>
          </div>

          <div>
            <div className="section-title">{t("settings.privacy")}</div>
            <div className="group">
              <div className="setting-row">
                <div className="info">
                  <div className="label">{t("android.keyboard.doesTitle")}</div>
                  <div className="help">{t("android.keyboard.doesBody")}</div>
                </div>
              </div>
              <div className="setting-row">
                <div className="info">
                  <div className="label">{t("android.keyboard.whyTitle")}</div>
                  <div className="help">{t("android.keyboard.whyBody")}</div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </>
  );
}

/** Nome e ajuda dos temas conhecidos do teclado; tema desconhecido usa o `label` que o nativo mandou. */
const THEME_KEYS: Record<string, { label: Key; help: Key }> = {
  default: { label: "android.keyboard.themeDefault", help: "android.keyboard.themeDefaultHelp" },
  clipsync: { label: "android.keyboard.themeClipsync", help: "android.keyboard.themeClipsyncHelp" },
};
