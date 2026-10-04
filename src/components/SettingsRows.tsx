import { useState } from "react";
import { useApp } from "../store/app";
import { Toggle } from "./ui";

const TYPES: { id: string; label: string }[] = [
  { id: "text", label: "Texto" },
  { id: "link", label: "Links" },
  { id: "code", label: "Código" },
  { id: "email", label: "E-mail" },
  { id: "sensitive", label: "Sensíveis" },
  { id: "image", label: "Imagens" },
];

const ACCEPT_LIMITS = [25, 100, 500, 1024];

function acceptHelp(mode: string, mb: number) {
  if (mode === "auto") return "Aceita qualquer arquivo de dispositivos pareados";
  if (mode === "ask") return "Pergunta antes de cada arquivo";
  return `Aceita até ${mb >= 1024 ? `${mb / 1024} GB` : `${mb} MB`} sozinho; acima disso pergunta`;
}

const EXPIRE_OPTIONS = [
  { v: 1, label: "1 h" }, { v: 6, label: "6 h" }, { v: 24, label: "24 h" }, { v: 168, label: "7 dias" }, { v: 0, label: "Nunca" },
];

export function Row({ label, help, children }: { label: string; help: string; children: React.ReactNode }) {
  return (
    <div className="setting-row">
      <div className="info">
        <div className="label">{label}</div>
        <div className="help">{help}</div>
      </div>
      {children}
    </div>
  );
}

export function SyncRows() {
  const s = useApp((st) => st.settings)!;
  const platform = useApp((st) => st.platform);
  const save = useApp((st) => st.saveSettings);
  const me = useApp((st) => st.me);
  const [editingTypes, setEditingTypes] = useState(false);
  const [name, setName] = useState<string | null>(null);

  const typesLabel = s.syncedTypes.length === TYPES.length ? "todos" : `${s.syncedTypes.length} tipos`;
  const typesHelp = TYPES.filter((t) => s.syncedTypes.includes(t.id)).map((t) => t.label).join(", ") || "nenhum";

  return (
    <>
      <Row label="Nome deste dispositivo" help="Como ele aparece nos outros aparelhos">
        <input
          className="input"
          value={name ?? s.deviceName ?? me?.name ?? ""}
          onChange={(e) => setName(e.target.value)}
          onBlur={() => { if (name !== null && name.trim() && name !== s.deviceName) void save({ deviceName: name.trim() }); setName(null); }}
          onKeyDown={(e) => { if (e.key === "Enter") (e.target as HTMLInputElement).blur(); }}
        />
      </Row>
      <Row label="Sync automático" help="Envia tudo que você copia">
        <Toggle on={s.autoSend} onChange={(v) => save({ autoSend: v })} />
      </Row>
      <Row label="Receber automático" help="Aplica na área de transferência ao chegar">
        <Toggle on={s.autoReceive} onChange={(v) => save({ autoReceive: v })} />
      </Row>
      <Row label="Arquivos recebidos" help={acceptHelp(s.fileAccept, s.fileAutoAcceptMb)}>
        <select
          className="select"
          value={s.fileAccept === "limit" ? `limit:${s.fileAutoAcceptMb}` : s.fileAccept}
          onChange={(e) => {
            const [mode, mb] = e.target.value.split(":");
            void save(mode === "limit" ? { fileAccept: "limit", fileAutoAcceptMb: Number(mb) } : { fileAccept: mode as "auto" | "ask" });
          }}
        >
          <option value="auto">Sempre aceitar</option>
          {ACCEPT_LIMITS.map((mb) => <option key={mb} value={`limit:${mb}`}>Perguntar acima de {mb >= 1024 ? `${mb / 1024} GB` : `${mb} MB`}</option>)}
          <option value="ask">Sempre perguntar</option>
        </select>
      </Row>
      {platform === "windows" && (
        <>
          <Row label="Iniciar com o Windows" help="Abre escondido na bandeja ao ligar o PC; sem ele rodando nada sincroniza">
            <Toggle on={s.launchAtLogin} onChange={(v) => save({ launchAtLogin: v })} />
          </Row>
          <Row label="Notificações do Windows" help="Arquivo recebido e pedido de aceite, com a janela fechada">
            <Toggle on={s.notifications} onChange={(v) => save({ notifications: v })} />
          </Row>
          {s.notifications && (
            <Row label="Avisar texto recebido" help="Mostra uma notificação a cada texto que chega">
              <Toggle on={s.notifyText} onChange={(v) => save({ notifyText: v })} />
            </Row>
          )}
        </>
      )}
      <Row label="Tipos sincronizados" help={typesHelp}>
        {editingTypes ? (
          <div className="type-chips">
            {TYPES.map((t) => (
              <button
                key={t.id}
                className={`chip${s.syncedTypes.includes(t.id) ? " active" : ""}`}
                onClick={() => save({ syncedTypes: s.syncedTypes.includes(t.id) ? s.syncedTypes.filter((x) => x !== t.id) : [...s.syncedTypes, t.id] })}
              >
                {t.label}
              </button>
            ))}
            <button className="chip" onClick={() => setEditingTypes(false)}>OK</button>
          </div>
        ) : (
          <button className="tag" onClick={() => setEditingTypes(true)}>{typesLabel}</button>
        )}
      </Row>
      <Row label="Somente na rede local" help="Nunca sai da sua Wi-Fi. Não há servidores.">
        <Toggle on disabled onChange={() => {}} />
      </Row>
    </>
  );
}

export function PrivacyRows() {
  const s = useApp((st) => st.settings)!;
  const save = useApp((st) => st.saveSettings);
  return (
    <>
      <Row label="Ignorar gerenciadores de senha" help="Não captura cópias marcadas como sensíveis pelo app de origem">
        <Toggle on={s.ignorePasswordManagers} onChange={(v) => save({ ignorePasswordManagers: v })} />
      </Row>
      <Row label="Autoexpiração" help="Apaga itens do histórico depois de">
        <select className="select" value={s.autoExpireHours} onChange={(e) => save({ autoExpireHours: Number(e.target.value) })}>
          {EXPIRE_OPTIONS.map((o) => <option key={o.v} value={o.v}>{o.label}</option>)}
        </select>
      </Row>
      <Row label="Tokens e cartões" help={s.hideSensitive ? `Detecta e oculta, expira em ${s.sensitiveExpireMinutes} min` : "Detecta e mostra; expira como os demais"}>
        <select className="select dark" value={s.hideSensitive ? "hide" : "show"} onChange={(e) => save({ hideSensitive: e.target.value === "hide" })}>
          <option value="hide">Ocultar</option>
          <option value="show">Mostrar</option>
        </select>
      </Row>
    </>
  );
}
