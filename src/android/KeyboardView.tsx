// Nunca ativa nem troca o teclado sozinho: só mostra o estado do IME e leva às telas do sistema.
import { useEffect } from "react";
import { Back, Check, Keyboard as KeyboardIcon } from "../components/Icons";
import { useApp } from "../store/app";

export type ImeState = "off" | "enabled" | "active";

export function imeState(ime: { enabled: boolean; selected: boolean } | null): ImeState {
  if (!ime || !ime.enabled) return "off";
  return ime.selected ? "active" : "enabled";
}

export function KeyboardView({ onBack }: { onBack: () => void }) {
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
        <button className="icon-btn lg" style={{ marginLeft: -10 }} onClick={onBack} aria-label="Voltar"><Back size={22} /></button>
        <span className="page-title">Teclado</span>
      </div>
      <div className="and-scroll">
        <div className="and-settings">
          <div className={`kb-status ${state}`}>
            <div className="kb-status-icon"><KeyboardIcon size={26} /></div>
            <div className="kb-status-body">
              <div className="kb-status-title">ClipSync Keyboard</div>
              {state === "active" && <div className="kb-status-line ok"><Check size={14} /> Ativo</div>}
              {state === "enabled" && <div className="kb-status-line warn">⚠ Instalado, mas não selecionado</div>}
              {state === "off" && <div className="kb-status-line">○ Desativado</div>}
              <p className="kb-status-help">
                {state === "active" && "Você está usando o ClipSync Keyboard. Ele funciona como um teclado normal."}
                {state === "enabled" && "O teclado está habilitado no Android, mas outro teclado continua selecionado. Escolha o ClipSync Keyboard para usá-lo."}
                {state === "off" && "Você não precisa usar o ClipSync Keyboard. Se quiser, ative-o nas configurações de teclado do Android."}
              </p>
            </div>
          </div>

          <div className="kb-actions">
            {state === "off" && (
              <button className="btn btn-primary btn-block" onClick={() => void openImeSettings()}>Ativar ClipSync Keyboard</button>
            )}
            {state === "enabled" && (
              <>
                <button className="btn btn-primary btn-block" onClick={() => void showImePicker()}>Selecionar ClipSync Keyboard</button>
                <button className="btn btn-secondary btn-block" onClick={() => void openImeSettings()}>Configurações de teclado</button>
              </>
            )}
            {state === "active" && (
              <button className="btn btn-secondary btn-block" onClick={() => void openImeSettings()}>Configurações de teclado</button>
            )}
            <p className="kb-note">
              A ativação é feita nas Configurações do Android. O ClipSync nunca troca o seu teclado sozinho.
            </p>
          </div>

          <div>
            <div className="section-title">Aparência</div>
            <div className="group">
              {(ime?.themes ?? []).map((t) => {
                const on = ime?.theme === t.id;
                return (
                  <button key={t.id} className={`kb-theme-row${on ? " on" : ""}`} onClick={() => void setImeTheme(t.id)}>
                    <span className={`radio${on ? " on" : ""}`} />
                    <span className="kb-theme-label">{t.label}</span>
                    <span className="kb-theme-help">{THEME_HELP[t.id] ?? ""}</span>
                  </button>
                );
              })}
              {!ime?.themes?.length && <div className="setting-row"><div className="info"><div className="help">Temas indisponíveis nesta plataforma.</div></div></div>}
            </div>
          </div>

          <div>
            <div className="section-title">Privacidade</div>
            <div className="group">
              <div className="setting-row">
                <div className="info">
                  <div className="label">O que o teclado faz</div>
                  <div className="help">Digita no campo em que você está. Não registra, não guarda e não envia o que você digita — nem senhas.</div>
                </div>
              </div>
              <div className="setting-row">
                <div className="info">
                  <div className="label">Por que ele existe</div>
                  <div className="help">Em versões futuras, com o teclado selecionado, o ClipSync poderá detectar cópias enquanto você usa outros aplicativos. Hoje ele é só um teclado.</div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </>
  );
}

const THEME_HELP: Record<string, string> = {
  default: "Parece um teclado Android comum",
  clipsync: "Cores e tipografia do ClipSync",
};
