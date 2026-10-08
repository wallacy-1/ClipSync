import { useEffect, useState } from "react";
import { Clipboard, Clock, Monitor, Sliders, Transfers } from "../components/Icons";
import { NetworkWarning } from "../components/NetworkWarning";
import { PairPrompts } from "../components/PairPrompts";
import { Toast } from "../components/ui";
import { api, inTauri } from "../lib/ipc";
import { useApp } from "../store/app";
import { HomeView } from "./HomeView";
import { HistoryView } from "./HistoryView";
import { DevicesView } from "./DevicesView";
import { TransfersView } from "./TransfersView";
import { SettingsView } from "./SettingsView";
import { Onboarding } from "./Onboarding";
import { SendSheet } from "./SendSheet";
import { ShareSheet } from "./ShareSheet";
import { AskPrompt } from "../components/AskPrompt";
import { ItemSheet } from "./ItemSheet";
import { KeyboardPrompt } from "./KeyboardPrompt";

export type Tab = "home" | "history" | "transfers" | "devices" | "settings";

export function AndroidShell() {
  const onboarded = useApp((s) => s.onboarded);
  const platform = useApp((s) => s.platform);
  const keyboardPromptSeen = useApp((s) => s.keyboardPromptSeen);
  const [settingsSub, setSettingsSub] = useState<"keyboard" | null>(null);
  const [tab, setTab] = useState<Tab>("home");
  const [sendId, setSendId] = useState<string | null>(null);
  const [detailId, setDetailId] = useState<string | null>(null);
  const [startScan, setStartScan] = useState(false);

  // Android só deixa ler o clipboard em primeiro plano: ao voltar, lê agora.
  useEffect(() => {
    void useApp.getState().refreshNotifyPerm();
    const onVis = () => {
      if (document.visibilityState !== "visible") return;
      if (inTauri) void api.clipboardPollNow();
      void useApp.getState().refreshNotifyPerm();
    };
    document.addEventListener("visibilitychange", onVis);
    return () => document.removeEventListener("visibilitychange", onVis);
  }, []);

  // "Compartilhar → ClipSync": ao abrir e sempre que outro app nos traz para a frente.
  const pendingShare = useApp((s) => s.pendingShare);
  useEffect(() => {
    void useApp.getState().checkShare();
    const check = () => { if (document.visibilityState === "visible") void useApp.getState().checkShare(); };
    document.addEventListener("visibilitychange", check);
    return () => document.removeEventListener("visibilitychange", check);
  }, []);

  if (!onboarded) {
    return (
      <div className="and">
        <Onboarding onPair={() => { setStartScan(true); setTab("devices"); void useApp.getState().finishOnboarding(); }} />
        {pendingShare && <ShareSheet />}
        {!pendingShare && <AskPrompt variant="android" />}
        <PairPrompts />
        <NetworkWarning />
        <Toast />
      </div>
    );
  }

  // Teclado opcional: apresentado uma vez, na tela inicial (nunca no meio do pareamento).
  if (platform === "android" && !keyboardPromptSeen && tab === "home") {
    return (
      <div className="and">
        <KeyboardPrompt onConfigure={() => { setSettingsSub("keyboard"); setTab("settings"); }} />
        {pendingShare && <ShareSheet />}
        {!pendingShare && <AskPrompt variant="android" />}
        <PairPrompts />
        <NetworkWarning />
        <Toast />
      </div>
    );
  }

  const ctx = { openSend: setSendId, openDetail: setDetailId, goTab: setTab };

  return (
    <div className="and">
      {tab === "home" && <HomeView {...ctx} />}
      {tab === "history" && <HistoryView {...ctx} />}
      {tab === "transfers" && <TransfersView />}
      {tab === "devices" && <DevicesView autoScan={startScan} onScanHandled={() => setStartScan(false)} />}
      {tab === "settings" && <SettingsView key={settingsSub ?? "root"} initialSub={settingsSub} />}

      <nav className="and-tabs">
        <TabBtn active={tab === "home"} onClick={() => setTab("home")} label="Início" Icon={Clipboard} />
        <TabBtn active={tab === "history"} onClick={() => setTab("history")} label="Histórico" Icon={Clock} />
        <TabBtn active={tab === "transfers"} onClick={() => setTab("transfers")} label="Arquivos" Icon={Transfers} />
        <TabBtn active={tab === "devices"} onClick={() => setTab("devices")} label="Dispositivos" Icon={Monitor} />
        <TabBtn active={tab === "settings"} onClick={() => { setSettingsSub(null); setTab("settings"); }} label="Ajustes" Icon={Sliders} />
      </nav>

      {sendId && <SendSheet itemId={sendId} onClose={() => setSendId(null)} />}
      {pendingShare && <ShareSheet />}
      {!pendingShare && <AskPrompt variant="android" />}
      {detailId && <ItemSheet itemId={detailId} onClose={() => setDetailId(null)} onSend={(id) => { setDetailId(null); setSendId(id); }} />}
      <PairPrompts />
      <NetworkWarning />
      <Toast />
    </div>
  );
}

function TabBtn({ active, onClick, label, Icon }: { active: boolean; onClick: () => void; label: string; Icon: typeof Clipboard }) {
  return (
    <button className={`and-tab${active ? " active" : ""}`} onClick={onClick}>
      <Icon size={22} />
      <span>{label}</span>
    </button>
  );
}

export interface ViewCtx {
  openSend: (id: string) => void;
  openDetail: (id: string) => void;
  goTab: (t: Tab) => void;
}
