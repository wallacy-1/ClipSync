import { ClipboardCheck, Copy, Search } from "../components/Icons";
import { ItemText, TypeTag } from "../components/ui";
import { deviceName, relTime } from "../lib/format";
import type { Filter } from "../lib/types";
import { useVisibleItems, useApp } from "../store/app";
import type { ViewCtx } from "./AndroidShell";

const FILTERS: { id: Filter; label: string }[] = [
  { id: "all", label: "Tudo" },
  { id: "pinned", label: "Fixados" },
  { id: "text", label: "Texto" },
  { id: "code", label: "Código" },
  { id: "link", label: "Links" },
  { id: "email", label: "E-mail" },
  { id: "sensitive", label: "Sensíveis" },
];

export function HistoryView({ openDetail }: ViewCtx) {
  const query = useApp((s) => s.query);
  const setQuery = useApp((s) => s.setQuery);
  const filter = useApp((s) => s.filter);
  const setFilter = useApp((s) => s.setFilter);
  const items = useVisibleItems();
  const devices = useApp((s) => s.devices);
  const me = useApp((s) => s.me);
  const now = useApp((s) => s.now);
  const copy = useApp((s) => s.copy);

  return (
    <>
      <div className="and-head">
        <span className="page-title">Histórico</span>
      </div>
      <label className="and-search">
        <Search size={18} style={{ color: "var(--color-neutral-600)" }} />
        <input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Buscar por conteúdo ou tipo…" />
      </label>
      <div className="and-filters">
        {FILTERS.map((f) => (
          <button key={f.id} className={`chip${filter === f.id ? " active" : ""}`} onClick={() => setFilter(f.id)}>{f.label}</button>
        ))}
      </div>
      <div className="and-scroll">
        {items.length === 0 ? (
          <div className="and-empty">
            <div className="orb"><ClipboardCheck size={36} /></div>
            <h3>{query ? "Nada encontrado" : "Histórico vazio"}</h3>
            <p>{query ? "Tente outro termo ou filtro." : "Copie algo no celular ou no PC e ele aparece aqui."}</p>
          </div>
        ) : (
          <div className="and-list">
            {items.map((it) => (
              <div className="and-row" key={it.id} onClick={() => openDetail(it.id)}>
                <div className="body">
                  <ItemText item={it} />
                  <div className="meta">
                    <TypeTag item={it} />
                    <span>{deviceName(devices, it.sourceDeviceId, me?.deviceId ?? "")}</span><span>·</span><span>{relTime(it.createdAt, now)}</span>
                    {it.isPinned && <span>· fixado</span>}
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
