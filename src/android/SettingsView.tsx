import { useEffect, useState } from "react";
import { Keyboard as KeyboardIcon } from "../components/Icons";
import { PrivacyRows, SyncRows } from "../components/SettingsRows";
import { Confirm } from "../components/ui";
import { useApp } from "../store/app";
import { KeyboardView, imeState } from "./KeyboardView";

export function SettingsView({ initialSub }: { initialSub?: "keyboard" | null }) {
  const clearHistory = useApp((s) => s.clearHistory);
  const platform = useApp((s) => s.platform);
  const ime = useApp((s) => s.ime);
  const refreshIme = useApp((s) => s.refreshIme);
  const [confirm, setConfirm] = useState(false);
  const [sub, setSub] = useState<"keyboard" | null>(initialSub ?? null);

  useEffect(() => { if (platform === "android") void refreshIme(); }, [platform, refreshIme]);

  if (sub === "keyboard") return <KeyboardView onBack={() => setSub(null)} />;

  const state = imeState(ime);
  const imeSummary = state === "active" ? "Ativo" : state === "enabled" ? "Instalado, mas não selecionado" : "Desativado · opcional";

  return (
    <>
      <div className="and-head" style={{ padding: "16px 18px 14px" }}>
        <span className="page-title">Ajustes</span>
      </div>
      <div className="and-scroll">
        <div className="and-settings">
          <div>
            <div className="section-title">Sincronização</div>
            <div className="group"><SyncRows /></div>
          </div>
          {platform === "android" && (
            <div>
              <div className="section-title">Teclado</div>
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
          <div>
            <div className="section-title">Privacidade</div>
            <div className="group"><PrivacyRows /></div>
          </div>
          <button className="btn btn-secondary btn-block" onClick={() => setConfirm(true)}>Limpar histórico agora</button>
        </div>
      </div>
      {confirm && (
        <Confirm
          title="Limpar histórico?"
          body="Apaga todos os itens não fixados deste aparelho. Os outros dispositivos mantêm o histórico deles."
          confirmLabel="Limpar"
          danger
          onCancel={() => setConfirm(false)}
          onConfirm={() => { setConfirm(false); void clearHistory(true); }}
        />
      )}
    </>
  );
}
