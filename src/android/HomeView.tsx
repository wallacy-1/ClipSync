import { useState } from "react";
import { Clipboard, ClipboardCheck, Copy, Pin, Search, Send } from "../components/Icons";
import { ItemText, TypeTag, useMasked } from "../components/ui";
import { TYPE_LABEL, countdown, deviceName, isMono, relTime } from "../lib/format";
import { usePeers, useApp } from "../store/app";
import type { ViewCtx } from "./AndroidShell";

export function HomeView({ openSend, openDetail, goTab }: ViewCtx) {
  const items = useApp((s) => s.items);
  const devices = useApp((s) => s.devices);
  const peers = usePeers();
  const me = useApp((s) => s.me);
  const sync = useApp((s) => s.sync);
  const now = useApp((s) => s.now);
  const { copy, pin, reveal, hide, setQuery } = useApp.getState();
  const current = items[0] ?? null;
  const rest = items.slice(1, 6);
  const { masked, text } = useMasked(current);
  const [holding, setHolding] = useState(false);

  const connected = peers.filter((d) => d.state === "connected").length;
  const sending = items.some((i) => i.syncState === "sending");
  const status = sync.paused
    ? { cls: "tag-queued", label: "Pausado" }
    : sending
      ? { cls: "tag-sending", label: "Enviando…" }
      : connected > 0
        ? { cls: "tag-synced", label: "Sincronizado" }
        : peers.length === 0
          ? { cls: "", label: "Sem par" }
          : { cls: "tag-queued", label: "Offline" };

  const currentMeta = current
    ? current.syncState === "sending"
      ? `copiado ${relTime(current.createdAt, now)} · enviando`
      : `copiado ${relTime(current.createdAt, now)} · ${deviceName(devices, current.sourceDeviceId, me?.deviceId ?? "")}`
    : "";
  const currentNote = current?.isSensitive
    ? `${TYPE_LABEL[current.contentType]} detectado — ${masked ? "toque e segure para revelar" : "visível"} · expira em ${countdown(current.expiresAt, now) || "—"}`
    : current?.syncState === "sending"
      ? `Enviando para ${peers.filter((d) => d.autoSend).map((d) => d.name).join(" e ")}`
      : current?.syncState === "queued"
        ? "Na fila — envia quando um dispositivo voltar"
        : "";

  // Toque e segure revela o item sensível enquanto o dedo estiver no cartão.
  const holdProps = current?.isSensitive
    ? {
        onPointerDown: () => { setHolding(true); void reveal(current.id); },
        onPointerUp: () => { setHolding(false); hide(current.id); },
        onPointerLeave: () => { if (holding) { setHolding(false); hide(current.id); } },
        onContextMenu: (e: React.MouseEvent) => e.preventDefault(),
      }
    : {};

  return (
    <>
      <div className="and-head">
        <div className="logo"><Clipboard size={17} /></div>
        <span className="brand">ClipSync</span>
        {(peers.length > 0 || sync.paused) && <span className={`tag ${status.cls}`}>{status.label}</span>}
        <button className="icon-btn lg" onClick={() => { setQuery(""); goTab("history"); }}><Search size={20} /></button>
      </div>

      <div className="and-scroll">
        <div className="and-section">
          <div className="section-title">Área atual</div>
          {current ? (
            <div className="current">
              <div className="head">
                {current.isSensitive ? <span className="tag tag-sensitive">Sensível</span> : <TypeTag item={current} />}
                <span>{currentMeta}</span>
              </div>
              <div className={`text${isMono(current.contentType) ? "" : " body"}${masked ? " dim" : ""}`} {...holdProps}>{text}</div>
              {currentNote && <div className="note">{currentNote}</div>}
              <div className="actions">
                {peers.length > 0 && (
                  <button className="btn btn-primary" disabled={masked} onClick={() => openSend(current.id)}>
                    <Send size={17} /> Enviar
                  </button>
                )}
                <button className="btn btn-secondary" aria-label="Copiar" onClick={() => copy(current.id)}><Copy size={19} /></button>
                <button className={`btn btn-secondary${current.isPinned ? " on" : ""}`} aria-label="Fixar" onClick={() => pin(current.id, !current.isPinned)}><Pin size={19} /></button>
              </div>
            </div>
          ) : (
            <div className="current">
              <div className="head"><span className="tag">Vazio</span><span>nada copiado ainda</span></div>
              <div className="text body muted">Copie algo em qualquer app e ele aparece aqui.</div>
            </div>
          )}
        </div>

        <div className="and-section-head">
          <span className="section-title">Histórico</span>
          <button className="link" onClick={() => goTab("history")}>Ver tudo</button>
        </div>

        {rest.length === 0 ? (
          <div className="and-empty">
            <div className="orb"><ClipboardCheck size={36} /></div>
            <h3>Histórico vazio</h3>
            <p>Copie algo no celular ou no PC e ele aparece aqui.</p>
          </div>
        ) : (
          <div className="and-list">
            {rest.map((it) => (
              <div className="and-row" key={it.id} onClick={() => openDetail(it.id)}>
                <div className="body">
                  <ItemText item={it} />
                  <div className="meta">
                    <TypeTag item={it} />
                    <span>{deviceName(devices, it.sourceDeviceId, me?.deviceId ?? "")}</span><span>·</span><span>{relTime(it.createdAt, now)}</span>
                  </div>
                </div>
                <button className="icon-btn lg filled" aria-label="Copiar" onClick={(e) => { e.stopPropagation(); void copy(it.id); }}><Copy size={18} /></button>
              </div>
            ))}
          </div>
        )}
      </div>
    </>
  );
}
