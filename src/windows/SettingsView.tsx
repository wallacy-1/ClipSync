import { useState } from "react";
import { Shield } from "../components/Icons";
import { PrivacyRows, SyncRows } from "../components/SettingsRows";
import { Confirm } from "../components/ui";
import { useApp } from "../store/app";

export function SettingsView() {
  const clearHistory = useApp((s) => s.clearHistory);
  const [confirm, setConfirm] = useState(false);

  return (
    <div className="main">
      <div className="page">
        <div className="settings">
          <div>
            <div className="section-title">Sincronização</div>
            <SyncRows />
          </div>
          <div>
            <div className="section-title">Privacidade</div>
            <PrivacyRows />
          </div>
          <div className="security-note">
            <Shield size={20} style={{ color: "var(--color-accent-2-800)" }} />
            <span>Todo conteúdo trafega cifrado na sua rede local. Nada passa por servidores do ClipSync.</span>
          </div>
          <div>
            <button className="btn btn-secondary" onClick={() => setConfirm(true)}>Limpar histórico agora</button>
          </div>
        </div>
      </div>
      {confirm && (
        <Confirm
          title="Limpar histórico?"
          body="Apaga todos os itens não fixados deste dispositivo. Os outros dispositivos mantêm o histórico deles."
          confirmLabel="Limpar"
          danger
          onCancel={() => setConfirm(false)}
          onConfirm={() => { setConfirm(false); void clearHistory(true); }}
        />
      )}
    </div>
  );
}
