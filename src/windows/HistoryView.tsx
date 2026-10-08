import { useEffect, useMemo, useRef } from "react";
import { Clipboard, Copy, Pause, Play, Search, Send, Trash, WifiOff } from "../components/Icons";
import { SHORTCUTS } from "../components/SettingsRows";
import { CheckBox, Dot, ItemText, StateTag, TypeTag, useMasked } from "../components/ui";
import { TYPE_LABEL, countdown, deviceName, isMono, pluralize, relTime } from "../lib/format";
import type { Filter } from "../lib/types";
import { usePeers, useVisibleItems, useApp } from "../store/app";

const FILTERS: { id: Filter; label: string }[] = [
  { id: "all", label: "Tudo" },
  { id: "text", label: "Texto" },
  { id: "code", label: "Código" },
  { id: "link", label: "Links" },
  { id: "email", label: "E-mail" },
  { id: "sensitive", label: "Sensíveis" },
];

export function HistoryView() {
  const view = useApp((s) => s.view);
  const query = useApp((s) => s.query);
  const setQuery = useApp((s) => s.setQuery);
  const filter = useApp((s) => s.filter);
  const setFilter = useApp((s) => s.setFilter);
  const items = useVisibleItems();
  const total = useApp((s) => s.totalItems);
  const pinned = useApp((s) => s.pinnedItems);
  const sync = useApp((s) => s.sync);
  const peers = usePeers();
  const selectedId = useApp((s) => s.selectedId);
  const select = useApp((s) => s.select);
  const sendTargets = useApp((s) => s.sendTargets);
  const setPaused = useApp((s) => s.setPaused);
  const retry = useApp((s) => s.retry);
  const send = useApp((s) => s.send);
  const searchRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.key.toLowerCase() === "f") { e.preventDefault(); searchRef.current?.focus(); }
      if (e.key === "Escape" && document.activeElement === searchRef.current) { setQuery(""); searchRef.current?.blur(); }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [setQuery]);

  const selected = items.find((i) => i.id === selectedId) ?? items[0] ?? null;
  useEffect(() => {
    if (selected && selected.id !== selectedId) select(selected.id);
  }, [selected, selectedId, select]);

  const connected = peers.filter((d) => d.state === "connected");
  const offlineQueued = sync.queuedCount > 0 && connected.length === 0 && peers.length > 0;
  const queuedFor = peers.filter((d) => d.queued > 0).map((d) => d.name);

  const count = query.trim()
    ? `${pluralize(items.length, "resultado", "resultados")} de ${total}`
    : view === "pinned"
      ? pluralize(pinned, "item fixado", "itens fixados")
      : total === 0
        ? "nenhum item"
        : `${pluralize(total, "item", "itens")} · ${pinned} fixados`;

  return (
    <>
      <div className="main">
        <div className="toolbar">
          <div className="toolbar-row">
            <label className="search">
              <Search size={16} style={{ color: "var(--color-neutral-600)" }} />
              <input ref={searchRef} value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Buscar por conteúdo, tipo ou dispositivo…" />
              <span className="kbd">Ctrl+F</span>
            </label>
            <button className="btn btn-secondary" disabled={!selected} onClick={() => selected && send(selected.id, sendTargets)}>
              <Send size={15} /> Enviar para
            </button>
            {sync.paused ? (
              <button className="btn btn-primary" onClick={() => setPaused(false)}>
                <Play size={15} /> Retomar sync
              </button>
            ) : (
              <button className="btn btn-primary" onClick={() => setPaused(true)}>
                <Pause size={15} /> Pausar sync
              </button>
            )}
          </div>
          {view !== "pinned" && (
            <div className="filters">
              {FILTERS.map((f) => (
                <button key={f.id} className={`chip${filter === f.id ? " active" : ""}`} onClick={() => setFilter(f.id)}>{f.label}</button>
              ))}
              <span className="count">{count}</span>
            </div>
          )}
          {view === "pinned" && (
            <div className="filters"><span className="chip active">Fixados</span><span className="count">{count}</span></div>
          )}
        </div>

        {sync.paused && (
          <div className="banner paused">
            <Pause size={18} />
            <span>Sincronização pausada{sync.pausedUntil ? ` até ${new Date(sync.pausedUntil).toLocaleTimeString("pt-BR", { hour: "2-digit", minute: "2-digit" })}` : ""}. Você continua copiando localmente.</span>
            <button className="btn btn-ghost" onClick={() => setPaused(false)}>Retomar</button>
          </div>
        )}
        {!sync.paused && offlineQueued && (
          <div className="banner offline">
            <WifiOff size={18} style={{ color: "var(--color-accent-800)" }} />
            <span>
              Sem conexão com {queuedFor.length ? queuedFor.join(", ") : "seus dispositivos"}. {pluralize(sync.queuedCount, "item na fila", "itens na fila")} — serão enviados quando voltarem.
            </span>
            <button className="btn btn-ghost" onClick={retry}>Tentar agora</button>
          </div>
        )}

        {items.length === 0 ? (
          <div className="empty">
            <div className="orb"><Clipboard size={40} /></div>
            <h3>{query.trim() ? "Nada encontrado" : view === "pinned" ? "Nenhum item fixado" : "Nada no histórico ainda"}</h3>
            <p>
              {query.trim()
                ? "Tente outro termo, tipo ou nome de dispositivo."
                : view === "pinned"
                  ? "Fixe um item pelo painel à direita e ele fica aqui, sem expirar."
                  : "Copie algo com Ctrl+C e o item aparece aqui, já sincronizado com seus dispositivos."}
            </p>
            {!query.trim() && view !== "pinned" && (
              <button className="btn btn-secondary" onClick={() => useApp.getState().setView("devices")}>Ver como funciona</button>
            )}
          </div>
        ) : (
          <div className="list">
            {items.map((it) => <Row key={it.id} id={it.id} selected={selected?.id === it.id} />)}
          </div>
        )}
      </div>

      <Rail />
    </>
  );
}

function Row({ id, selected }: { id: string; selected: boolean }) {
  const it = useApp((s) => s.items.find((i) => i.id === id))!;
  const devices = useApp((s) => s.devices);
  const me = useApp((s) => s.me);
  const now = useApp((s) => s.now);
  const revealed = useApp((s) => !!s.revealed[id]);
  const hideSensitive = useApp((s) => s.settings?.hideSensitive ?? true);
  const { select, copy, remove, reveal, hide, send, sendTargets } = useApp.getState();
  if (!it) return null;
  const masked = it.isSensitive && hideSensitive && !revealed;
  const note = it.isSensitive
    ? `${masked ? "oculto" : "visível"} · expira em ${countdown(it.expiresAt, now) || "—"}`
    : it.isPinned ? "fixado" : "";

  return (
    <div className={`row${selected ? " selected" : ""}`} onClick={() => select(it.id)} onDoubleClick={() => copy(it.id)}>
      <TypeTag item={it} />
      <div className="body">
        <ItemText item={it} />
        <div className="meta">
          <span>{deviceName(devices, it.sourceDeviceId, me?.deviceId ?? "")}</span>
          <span>·</span>
          <span>{relTime(it.createdAt, now)}</span>
          {note && <span className="note">· {note}</span>}
        </div>
      </div>
      {it.isSensitive && hideSensitive && (
        <button className="btn btn-secondary sm" onClick={(e) => { e.stopPropagation(); masked ? void reveal(it.id) : hide(it.id); }}>
          {masked ? "Mostrar" : "Ocultar"}
        </button>
      )}
      <StateTag item={it} />
      <div className="actions">
        <button className="icon-btn" title="Copiar" onClick={(e) => { e.stopPropagation(); void copy(it.id); }}><Copy size={15} /></button>
        <button className="icon-btn" title="Enviar para os dispositivos marcados" onClick={(e) => { e.stopPropagation(); void send(it.id, sendTargets); }}><Send size={15} /></button>
        <button className="icon-btn" title="Apagar" onClick={(e) => { e.stopPropagation(); void remove(it.id); }}><Trash size={15} /></button>
      </div>
    </div>
  );
}

function Rail() {
  const selectedId = useApp((s) => s.selectedId);
  const item = useApp((s) => s.items.find((i) => i.id === selectedId) ?? null);
  const devices = useApp((s) => s.devices);
  const peers = usePeers();
  const me = useApp((s) => s.me);
  const now = useApp((s) => s.now);
  const sendTargets = useApp((s) => s.sendTargets);
  const toggleTarget = useApp((s) => s.toggleTarget);
  const shortcuts = useApp((s) => s.settings?.shortcuts);
  const { copy, pin, send } = useApp.getState();
  const { masked, text } = useMasked(item);

  const syncedWith = useMemo(() => {
    if (!item) return "";
    if (item.sourceDeviceId !== me?.deviceId) return "recebido";
    if (item.syncState === "synced") return pluralize(peers.filter((d) => d.autoSend).length, "dispositivo", "dispositivos");
    if (item.syncState === "queued") return "na fila";
    if (item.syncState === "sending") return "enviando…";
    return "só aqui";
  }, [item, me, peers]);

  return (
    <aside className="rail">
      <div className="section-title">Item selecionado</div>
      {item ? (
        <>
          <div className={`content${isMono(item.contentType) ? "" : " text"}`}>{text}</div>
          <div className="kv">
            <div><span>Origem</span><span>{deviceName(devices, item.sourceDeviceId, me?.deviceId ?? "")}</span></div>
            <div><span>Copiado</span><span>{relTime(item.createdAt, now)}</span></div>
            <div><span>Tipo</span><span>{TYPE_LABEL[item.contentType]}</span></div>
            {item.expiresAt && <div><span>Expira</span><span>em {countdown(item.expiresAt, now)}</span></div>}
            <div><span>Sincronizado com</span><span>{syncedWith}</span></div>
          </div>
          <div className="actions">
            <button className="btn btn-primary btn-block" onClick={() => copy(item.id)}>Copiar agora</button>
            <div className="pair">
              <button className="btn btn-secondary" onClick={() => pin(item.id, !item.isPinned)}>{item.isPinned ? "Desafixar" : "Fixar"}</button>
              <button className="btn btn-secondary" disabled={masked} onClick={() => send(item.id, sendTargets)}>Enviar</button>
            </div>
          </div>
        </>
      ) : (
        <div className="muted" style={{ fontSize: 13.5 }}>Selecione um item para ver detalhes.</div>
      )}

      <div className="targets">
        <div className="section-title">Enviar para</div>
        {peers.length === 0 && <div className="muted" style={{ fontSize: 13 }}>Nenhum dispositivo pareado ainda.</div>}
        {peers.map((d) => (
          <div className="target-row" key={d.deviceId}>
            <Dot device={d} />
            <span className="name">{d.name}</span>
            <CheckBox small on={sendTargets.includes(d.deviceId)} onChange={() => toggleTarget(d.deviceId)} disabled={d.state === "blocked"} />
          </div>
        ))}
      </div>

      <div className="shortcuts">
        <div className="section-title">Atalhos</div>
        <div className="lines">
          {SHORTCUTS.map((sc) => (
            <div key={sc.id}><span>{sc.label}</span><span>{shortcuts?.[sc.id] || "—"}</span></div>
          ))}
        </div>
      </div>
    </aside>
  );
}
