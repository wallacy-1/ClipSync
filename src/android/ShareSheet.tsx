import { useEffect, useState } from "react";
import { Shield } from "../components/Icons";
import { Avatar } from "../components/ui";
import { bytes, pluralize } from "../lib/format";
import { useApp } from "../store/app";
import { useFileTargets } from "../windows/SendFileDialog";

/** "Compartilhar → ClipSync": escolher para qual dispositivo vai o que outro app compartilhou. */
export function ShareSheet() {
  const share = useApp((s) => s.pendingShare);
  const sendShare = useApp((s) => s.sendShare);
  const dismiss = useApp((s) => s.dismissShare);
  const { targets, loading } = useFileTargets();
  const [selected, setSelected] = useState<string | null>(null);
  const [sending, setSending] = useState(false);

  // Offline também serve: arquivo e texto entram na fila e saem ao reconectar.
  const options = targets.map((t) => ({ ...t, selectable: t.selectable || t.cap === "Offline" }));
  const available = options.filter((t) => t.selectable);
  useEffect(() => {
    if (selected === null && available.length > 0) setSelected(available[0].device.deviceId);
  }, [available, selected]);
  if (!share) return null;

  const chosen = options.find((t) => t.device.deviceId === selected);
  const files = share.files;
  const total = files.reduce((n, f) => n + Math.max(0, f.size), 0);
  const title = files.length > 0 ? (files.length === 1 ? "Enviar arquivo" : `Enviar ${files.length} arquivos`) : "Enviar texto";
  const preview = files.length === 0
    ? share.text
    : files.length === 1
      ? `${files[0].name}${files[0].size > 0 ? ` · ${bytes(files[0].size)}` : ""}`
      : `${files.slice(0, 3).map((f) => f.name).join(", ")}${files.length > 3 ? ` e mais ${files.length - 3}` : ""}${total > 0 ? ` · ${bytes(total)}` : ""}`;

  return (
    <div className="sheet-backdrop" onClick={dismiss}>
      <div className="sheet" onClick={(e) => e.stopPropagation()}>
        <div className="handle" />
        <h3>{title}</h3>
        <div className="sub">Compartilhado com o ClipSync. Escolha quem recebe.</div>
        <div className={`preview${files.length === 0 ? " body" : ""}`}>{preview}</div>
        <div className="targets">
          {!loading && available.length === 0 && (
            <div className="muted" style={{ fontSize: 13.5 }}>Nenhum dispositivo disponível. Vá em Dispositivos para parear.</div>
          )}
          {options.map((t) => (
            <button
              key={t.device.deviceId}
              className={`target-row2${selected === t.device.deviceId ? " selected" : ""}${!t.selectable ? " blocked" : ""}`}
              disabled={!t.selectable}
              onClick={() => setSelected(t.device.deviceId)}
            >
              <Avatar device={t.device} />
              <div className="info">
                <div className="name">{t.device.name}</div>
                <div className="detail">{t.cap === "Offline" ? "Offline · entra na fila" : t.detail}</div>
              </div>
              {t.selectable ? <span className={`radio${selected === t.device.deviceId ? " on" : ""}`} /> : <span className="tag tag-queued">{t.cap}</span>}
            </button>
          ))}
        </div>
        <button
          className="btn btn-primary btn-block cta"
          disabled={!chosen?.selectable || sending}
          onClick={async () => {
            if (!chosen) return;
            setSending(true);
            await sendShare(chosen.device.deviceId);
            setSending(false);
          }}
        >
          {chosen ? `Enviar para ${chosen.device.name}` : "Enviar"}
        </button>
        <div className="foot"><Shield size={15} /> {files.length > 0 ? `${pluralize(files.length, "arquivo", "arquivos")} direto pela rede local, cifrado ponta a ponta.` : "Direto pela rede local, cifrado ponta a ponta."}</div>
      </div>
    </div>
  );
}
