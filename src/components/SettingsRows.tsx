import { useEffect, useState } from "react";
import { t, useT, type LangPref } from "../i18n";
import { PLATFORM_LABEL } from "../lib/format";
import { api, appVersion } from "../lib/ipc";
import type { Shortcuts, ThemePref } from "../lib/types";
import { useApp } from "../store/app";
import { Toggle } from "./ui";

/** Rótulos lidos na hora (getter): seguem o idioma ativo. */
const TYPES: { id: string; readonly label: string }[] = [
  { id: "text", get label() { return t("history.filterText"); } },
  { id: "link", get label() { return t("history.filterLink"); } },
  { id: "code", get label() { return t("history.filterCode"); } },
  { id: "email", get label() { return t("history.filterEmail"); } },
  { id: "sensitive", get label() { return t("history.filterSensitive"); } },
  { id: "image", get label() { return t("history.filterImage"); } },
];

const ACCEPT_LIMITS = [25, 100, 500, 1024];

const sizeMb = (mb: number) => (mb >= 1024 ? `${mb / 1024} GB` : `${mb} MB`);

function acceptHelp(mode: string, mb: number) {
  if (mode === "auto") return t("settings.acceptAuto");
  if (mode === "ask") return t("settings.acceptAsk");
  return t("settings.acceptLimit", { size: sizeMb(mb) });
}

const expireOptions = () => [
  { v: 1, label: "1 h" }, { v: 6, label: "6 h" }, { v: 24, label: "24 h" }, { v: 168, label: t("settings.days7") }, { v: 0, label: t("common.never") },
];

export function Row({ label, help, stack, children }: { label: string; help: string; stack?: boolean; children: React.ReactNode }) {
  return (
    <div className={stack ? "setting-row stack" : "setting-row"}>
      <div className="info">
        <div className="label">{label}</div>
        <div className="help">{help}</div>
      </div>
      {children}
    </div>
  );
}

const themes = (): { v: ThemePref; label: string }[] => [
  { v: "system", label: t("settings.themeSystem") }, { v: "light", label: t("settings.themeLight") }, { v: "dark", label: t("settings.themeDark") },
];

/** "Português" e "English" ficam no próprio idioma, para quem abriu o app na língua errada achar a saída. */
const languages = (): { v: LangPref; label: string }[] => [
  { v: "auto", label: t("settings.langAuto") }, { v: "pt-BR", label: "Português" }, { v: "en", label: "English" },
];

export function AppearanceRows() {
  const t = useT();
  const s = useApp((st) => st.settings)!;
  const save = useApp((st) => st.saveSettings);
  return (
    <>
      <Row label={t("settings.theme")} help={t("settings.themeHelp")}>
        <select className="select" value={s.theme} onChange={(e) => save({ theme: e.target.value as ThemePref })}>
          {themes().map((o) => <option key={o.v} value={o.v}>{o.label}</option>)}
        </select>
      </Row>
      <Row label={t("settings.language")} help={t("settings.languageHelp")}>
        <select className="select" value={s.language} onChange={(e) => save({ language: e.target.value as LangPref })}>
          {languages().map((o) => <option key={o.v} value={o.v}>{o.label}</option>)}
        </select>
      </Row>
    </>
  );
}

export function SyncRows() {
  const t = useT();
  const s = useApp((st) => st.settings)!;
  const platform = useApp((st) => st.platform);
  const save = useApp((st) => st.saveSettings);
  const me = useApp((st) => st.me);
  const [editingTypes, setEditingTypes] = useState(false);
  const [name, setName] = useState<string | null>(null);

  const typesLabel = s.syncedTypes.length === TYPES.length ? t("settings.typesAll") : t("settings.typesCount", { n: s.syncedTypes.length });
  const typesHelp = TYPES.filter((o) => s.syncedTypes.includes(o.id)).map((o) => o.label).join(", ") || t("common.none");

  return (
    <>
      <Row label={t("settings.deviceName")} help={t("settings.deviceNameHelp")}>
        <input
          className="input"
          value={name ?? (s.deviceName || me?.name || "")}
          onChange={(e) => setName(e.target.value)}
          onBlur={() => { if (name !== null && name.trim() && name !== s.deviceName) void save({ deviceName: name.trim() }); setName(null); }}
          onKeyDown={(e) => { if (e.key === "Enter") (e.target as HTMLInputElement).blur(); }}
        />
      </Row>
      <Row label={t("settings.autoSync")} help={t("settings.autoSyncHelp")}>
        <Toggle on={s.autoSend} onChange={(v) => save({ autoSend: v })} />
      </Row>
      <Row label={t("settings.autoReceive")} help={t("settings.autoReceiveHelp")}>
        <Toggle on={s.autoReceive} onChange={(v) => save({ autoReceive: v })} />
      </Row>
      <Row label={t("settings.filesReceived")} help={acceptHelp(s.fileAccept, s.fileAutoAcceptMb)}>
        <select
          className="select"
          value={s.fileAccept === "limit" ? `limit:${s.fileAutoAcceptMb}` : s.fileAccept}
          onChange={(e) => {
            const [mode, mb] = e.target.value.split(":");
            void save(mode === "limit" ? { fileAccept: "limit", fileAutoAcceptMb: Number(mb) } : { fileAccept: mode as "auto" | "ask" });
          }}
        >
          <option value="auto">{t("settings.acceptAlways")}</option>
          {ACCEPT_LIMITS.map((mb) => <option key={mb} value={`limit:${mb}`}>{t("settings.askAbove", { size: sizeMb(mb) })}</option>)}
          <option value="ask">{t("settings.askAlways")}</option>
        </select>
      </Row>
      {platform === "android" && (
        <Row label={t("settings.keepAlive")} help={t("settings.keepAliveHelp")}>
          <Toggle on={s.keepAlive} onChange={(v) => save({ keepAlive: v })} />
        </Row>
      )}
      {platform === "windows" && (
        <>
          <Row label={t("settings.launchAtLogin")} help={t("settings.launchAtLoginHelp")}>
            <Toggle on={s.launchAtLogin} onChange={(v) => save({ launchAtLogin: v })} />
          </Row>
          <Row label={t("settings.notifications")} help={t("settings.notificationsHelp")}>
            <Toggle on={s.notifications} onChange={(v) => save({ notifications: v })} />
          </Row>
          {s.notifications && (
            <Row label={t("settings.notifyText")} help={t("settings.notifyTextHelp")}>
              <Toggle on={s.notifyText} onChange={(v) => save({ notifyText: v })} />
            </Row>
          )}
        </>
      )}
      <Row label={t("settings.keepFormatting")} help={t("settings.keepFormattingHelp")}>
        <Toggle on={s.keepFormatting} onChange={(v) => save({ keepFormatting: v })} />
      </Row>
      <Row label={t("settings.syncedTypes")} help={typesHelp}>
        {editingTypes ? (
          <div className="type-chips">
            {TYPES.map((o) => (
              <button
                key={o.id}
                className={`chip${s.syncedTypes.includes(o.id) ? " active" : ""}`}
                onClick={() => save({ syncedTypes: s.syncedTypes.includes(o.id) ? s.syncedTypes.filter((x) => x !== o.id) : [...s.syncedTypes, o.id] })}
              >
                {o.label}
              </button>
            ))}
            <button className="chip" onClick={() => setEditingTypes(false)}>{t("common.ok")}</button>
          </div>
        ) : (
          <button className="tag" onClick={() => setEditingTypes(true)}>{typesLabel}</button>
        )}
      </Row>
      <Row label={t("settings.lanOnly")} help={t("settings.lanOnlyHelp")}>
        <Toggle on disabled onChange={() => {}} />
      </Row>
    </>
  );
}

export function PrivacyRows() {
  const t = useT();
  const s = useApp((st) => st.settings)!;
  const save = useApp((st) => st.saveSettings);
  return (
    <>
      <Row label={t("settings.ignorePm")} help={t("settings.ignorePmHelp")}>
        <Toggle on={s.ignorePasswordManagers} onChange={(v) => save({ ignorePasswordManagers: v })} />
      </Row>
      <Row label={t("settings.autoExpire")} help={t("settings.autoExpireHelp")}>
        <select className="select" value={s.autoExpireHours} onChange={(e) => save({ autoExpireHours: Number(e.target.value) })}>
          {expireOptions().map((o) => <option key={o.v} value={o.v}>{o.label}</option>)}
        </select>
      </Row>
      <Row label={t("settings.tokens")} help={s.hideSensitive ? t("settings.tokensHideHelp", { min: s.sensitiveExpireMinutes }) : t("settings.tokensShowHelp")}>
        <select className="select dark" value={s.hideSensitive ? "hide" : "show"} onChange={(e) => save({ hideSensitive: e.target.value === "hide" })}>
          <option value="hide">{t("settings.tokensHide")}</option>
          <option value="show">{t("settings.tokensShow")}</option>
        </select>
      </Row>
    </>
  );
}

/** `label` e `help` são getters: seguem o idioma ativo. */
export const SHORTCUTS: { id: keyof Shortcuts; readonly label: string; readonly help: string }[] = [
  { id: "openPanel", get label() { return t("settings.shortcutOpenPanel"); }, get help() { return t("settings.shortcutOpenPanelHelp"); } },
  { id: "pasteLast", get label() { return t("settings.shortcutPasteLast"); }, get help() { return t("settings.shortcutPasteLastHelp"); } },
  { id: "sendLast", get label() { return t("settings.shortcutSendLast"); }, get help() { return t("settings.shortcutSendLastHelp"); } },
  { id: "togglePause", get label() { return t("settings.shortcutTogglePause"); }, get help() { return t("settings.shortcutTogglePauseHelp"); } },
];

const MODIFIER_CODES = ["ControlLeft", "ControlRight", "AltLeft", "AltRight", "ShiftLeft", "ShiftRight", "MetaLeft", "MetaRight"];

/** Monta "Ctrl+Alt+Shift+V" a partir da tecla; null enquanto só há modificadores. */
function comboFromEvent(e: KeyboardEvent): string | null {
  if (MODIFIER_CODES.includes(e.code)) return null;
  const key = e.code.replace(/^Key/, "").replace(/^Digit/, "");
  const mods = [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Win"].filter(Boolean);
  return [...mods, key].join("+");
}

export function ShortcutRows() {
  const t = useT();
  const s = useApp((st) => st.settings)!;
  const save = useApp((st) => st.saveSettings);
  const [recording, setRecording] = useState<keyof Shortcuts | null>(null);
  const [failed, setFailed] = useState<string[]>([]);

  const refreshFailures = () => void api.shortcutFailures().then(setFailed).catch(() => {});
  useEffect(refreshFailures, []);

  const apply = async (id: keyof Shortcuts, combo: string) => {
    setRecording(null);
    await save({ shortcuts: { ...s.shortcuts, [id]: combo } });
    await api.shortcutsSuspend(false).catch(() => {});
    refreshFailures();
  };

  // Enquanto grava, os atalhos ficam soltos para a combinação atual também chegar aqui.
  useEffect(() => {
    if (!recording) return;
    void api.shortcutsSuspend(true).catch(() => {});
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const plain = !e.ctrlKey && !e.altKey && !e.shiftKey && !e.metaKey;
      if (plain && e.code === "Escape") {
        setRecording(null);
        void api.shortcutsSuspend(false).catch(() => {});
        return;
      }
      if (plain && (e.code === "Backspace" || e.code === "Delete")) {
        void apply(recording, "");
        return;
      }
      const combo = comboFromEvent(e);
      if (combo) void apply(recording, combo);
    };
    const onBlur = () => {
      setRecording(null);
      void api.shortcutsSuspend(false).catch(() => {});
    };
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("blur", onBlur);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("blur", onBlur);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [recording]);

  return (
    <>
      {SHORTCUTS.map((sc) => {
        const combo = s.shortcuts[sc.id];
        const help = recording === sc.id
          ? t("settings.recordHint")
          : failed.includes(sc.id)
            ? t("settings.shortcutTaken")
            : sc.help;
        return (
          <Row key={sc.id} label={sc.label} help={help}>
            <button
              className={`chip shortcut-key${recording === sc.id ? " active" : ""}${failed.includes(sc.id) ? " failed" : ""}`}
              onClick={() => {
                if (recording !== sc.id) return setRecording(sc.id);
                setRecording(null);
                void api.shortcutsSuspend(false).catch(() => {});
              }}
            >
              {recording === sc.id ? t("settings.recording") : combo || t("settings.off")}
            </button>
          </Row>
        );
      })}
    </>
  );
}

export function AboutRows() {
  const t = useT();
  const s = useApp((st) => st.settings)!;
  const save = useApp((st) => st.saveSettings);
  const platform = useApp((st) => st.platform);
  const [version, setVersion] = useState<string | null>(null);
  const [addrs, setAddrs] = useState<string[] | null>(null);
  useEffect(() => { void appVersion().then(setVersion).catch(() => setVersion("?")); }, []);
  useEffect(() => { void api.localAddresses().then(setAddrs).catch(() => setAddrs([])); }, []);
  return (
    <>
      <Row label={t("settings.version")} help={t("settings.versionHelp", { platform: PLATFORM_LABEL[platform] ?? platform })}>
        <span className="tag">{version ?? "…"}</span>
      </Row>
      <Row stack label={t("settings.address")} help={t("settings.addressHelp")}>
        <span className="addr">{addrs === null ? "…" : addrs.length > 0 ? addrs.join(" · ") : t("settings.noNetwork")}</span>
      </Row>
      <Row label={t("settings.checkUpdates")} help={t("settings.checkUpdatesHelp")}>
        <Toggle on={s.checkUpdates} onChange={(v) => save({ checkUpdates: v })} />
      </Row>
      <UpdateRow />
    </>
  );
}

function UpdateRow() {
  const t = useT();
  const update = useApp((st) => st.update);
  const updateBusy = useApp((st) => st.updateBusy);
  const checkUpdate = useApp((st) => st.checkUpdate);
  const installUpdate = useApp((st) => st.installUpdate);
  const openUpdate = useApp((st) => st.openUpdate);
  const platform = useApp((st) => st.platform);
  if (update) {
    // Android não instala por dentro do app: abre a página da release e o usuário baixa o APK.
    if (platform === "android") {
      return (
        <Row label={t("settings.update")} help={t("settings.updateAndroid", { version: update.version })}>
          <button className="chip" onClick={() => void openUpdate()}>{t("settings.download")}</button>
        </Row>
      );
    }
    return (
      <Row label={t("settings.update")} help={t("settings.updateDesktop", { version: update.version })}>
        <button className="chip" disabled={updateBusy !== null} onClick={() => void installUpdate()}>
          {updateBusy === "install" ? t("settings.installing") : t("settings.install")}
        </button>
      </Row>
    );
  }
  return (
    <Row label={t("settings.update")} help={t("settings.updateIdle")}>
      <button className="chip" disabled={updateBusy !== null} onClick={() => void checkUpdate()}>
        {updateBusy === "check" ? t("settings.checking") : t("settings.checkNow")}
      </button>
    </Row>
  );
}
