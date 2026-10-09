import { useT, type Key } from "../i18n";
import { ClipboardCheck, Copy, Search } from "../components/Icons";
import { ItemText, TypeTag } from "../components/ui";
import { deviceName, relTime } from "../lib/format";
import type { Filter } from "../lib/types";
import { useVisibleItems, useApp } from "../store/app";
import type { ViewCtx } from "./AndroidShell";

const FILTERS: { id: Filter; label: Key }[] = [
  { id: "all", label: "history.filterAll" },
  { id: "pinned", label: "nav.pinned" },
  { id: "text", label: "history.filterText" },
  { id: "code", label: "history.filterCode" },
  { id: "link", label: "history.filterLink" },
  { id: "email", label: "history.filterEmail" },
  { id: "image", label: "history.filterImage" },
  { id: "sensitive", label: "history.filterSensitive" },
];

export function HistoryView({ openDetail }: ViewCtx) {
  const t = useT();
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
        <span className="page-title">{t("nav.history")}</span>
      </div>
      <label className="and-search">
        <Search size={18} style={{ color: "var(--color-neutral-600)" }} />
        <input value={query} onChange={(e) => setQuery(e.target.value)} placeholder={t("android.history.searchPlaceholder")} />
      </label>
      <div className="and-filters">
        {FILTERS.map((f) => (
          <button key={f.id} className={`chip${filter === f.id ? " active" : ""}`} onClick={() => setFilter(f.id)}>{t(f.label)}</button>
        ))}
      </div>
      <div className="and-scroll">
        {items.length === 0 ? (
          <div className="and-empty">
            <div className="orb"><ClipboardCheck size={36} /></div>
            <h3>{query ? t("history.emptySearchTitle") : t("android.history.emptyTitle")}</h3>
            <p>{query ? t("android.history.noResultsBody") : t("android.history.emptyBody")}</p>
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
                    {it.isPinned && <span>· {t("item.pinned")}</span>}
                  </div>
                </div>
                <button className="icon-btn lg filled" aria-label={t("history.copy")} onClick={(e) => { e.stopPropagation(); void copy(it.id); }}><Copy size={18} /></button>
              </div>
            ))}
          </div>
        )}
      </div>
    </>
  );
}
