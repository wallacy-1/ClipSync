import { useState } from "react";
import { Clock } from "../components/Icons";
import { Avatar, CheckBox, useMasked } from "../components/ui";
import { PLATFORM_LABEL, isMono, pluralize } from "../lib/format";
import { usePeers, useApp } from "../store/app";

export function SendSheet({ itemId, onClose }: { itemId: string; onClose: () => void }) {
  const item = useApp((s) => s.items.find((i) => i.id === itemId) ?? null);
  const peers = usePeers();
  const settings = useApp((s) => s.settings);
  const send = useApp((s) => s.send);
  const [targets, setTargets] = useState<string[]>(peers.filter((d) => d.state === "connected" || d.autoSend).map((d) => d.deviceId));
  const { text } = useMasked(item);
  if (!item) return null;

  const toggle = (id: string) => setTargets((t) => (t.includes(id) ? t.filter((x) => x !== id) : [...t, id]));
  const expireText = item.isSensitive
    ? `Apaga dos outros dispositivos em ${settings?.sensitiveExpireMinutes ?? 2} min`
    : settings?.autoExpireHours
      ? `Apaga dos outros dispositivos em ${settings.autoExpireHours >= 24 ? `${settings.autoExpireHours / 24} dia${settings.autoExpireHours > 24 ? "s" : ""}` : `${settings.autoExpireHours} hora${settings.autoExpireHours > 1 ? "s" : ""}`}`
      : "Fica no histórico dos outros dispositivos";

  return (
    <div className="sheet-backdrop" onClick={onClose}>
      <div className="sheet" onClick={(e) => e.stopPropagation()}>
        <div className="handle" />
        <h3>Enviar para</h3>
        <div className="sub">Escolha os dispositivos que recebem este item.</div>
        <div className={`preview${isMono(item.contentType) ? "" : " body"}`}>{text}</div>
        <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
          {peers.length === 0 && <div className="muted" style={{ fontSize: 13.5 }}>Nenhum dispositivo pareado. Vá em Dispositivos para adicionar.</div>}
          {peers.map((d) => (
            <button className="target-card" key={d.deviceId} onClick={() => toggle(d.deviceId)} disabled={d.state === "blocked"}>
              <Avatar device={d} />
              <div className="info">
                <div className="name">{d.name}</div>
                <div className="detail">
                  {d.state === "connected" ? `${PLATFORM_LABEL[d.platform] ?? d.platform} · conectado` : d.state === "blocked" ? "Bloqueado" : "Offline · entra na fila"}
                </div>
              </div>
              <CheckBox on={targets.includes(d.deviceId)} onChange={() => toggle(d.deviceId)} />
            </button>
          ))}
        </div>
        <button className="btn btn-primary btn-block cta" disabled={targets.length === 0} onClick={async () => { await send(item.id, targets); onClose(); }}>
          Enviar para {pluralize(targets.length, "dispositivo", "dispositivos")}
        </button>
        <div className="foot"><Clock size={15} /> {expireText}</div>
      </div>
    </div>
  );
}
