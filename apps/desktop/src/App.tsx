import { useEffect, useMemo, useState } from "react";
import {
  Activity,
  AlignLeft,
  ArrowLeftRight,
  Brain,
  Code2,
  Crosshair,
  FileSearch,
  Gauge,
  KeySquare,
  ListTree,
  type LucideIcon,
  Plug,
  Radio,
  Radar,
  Repeat,
  Settings as SettingsIcon,
  Shield,
} from "lucide-react";

import { Toasts } from "@/components/Toasts";
import { AiAssistantPage } from "@/pages/AiAssistant";
import { CollaboratorPage } from "@/pages/Collaborator";
import { ComparerPage } from "@/pages/Comparer";
import { DashboardPage } from "@/pages/Dashboard";
import { DecoderPage } from "@/pages/Decoder";
import { ExtenderPage } from "@/pages/Extender";
import { IntruderPage } from "@/pages/Intruder";
import { LoggerPage } from "@/pages/Logger";
import { ProjectOptionsPage } from "@/pages/ProjectOptions";
import { ProxyPage } from "@/pages/Proxy";
import { RepeaterPage } from "@/pages/Repeater";
import { SequencerPage } from "@/pages/Sequencer";
import { TargetPage } from "@/pages/Target";
import { UserOptionsPage } from "@/pages/UserOptions";
import { useAppStore } from "@/state/store";

type Page =
  | "dashboard"
  | "target"
  | "proxy"
  | "intruder"
  | "repeater"
  | "sequencer"
  | "decoder"
  | "comparer"
  | "logger"
  | "extender"
  | "collaborator"
  | "ai"
  | "project-options"
  | "user-options";

interface NavEntry {
  id: Page;
  label: string;
  icon: LucideIcon;
  group: "tools" | "options";
  badge?: () => string | undefined;
}

const NAV: NavEntry[] = [
  { id: "dashboard", label: "Dashboard", icon: Gauge, group: "tools" },
  { id: "target", label: "Target", icon: ListTree, group: "tools" },
  { id: "proxy", label: "Proxy", icon: Shield, group: "tools" },
  { id: "intruder", label: "Intruder", icon: Crosshair, group: "tools" },
  { id: "repeater", label: "Repeater", icon: Repeat, group: "tools" },
  { id: "sequencer", label: "Sequencer", icon: Activity, group: "tools" },
  { id: "decoder", label: "Decoder", icon: Code2, group: "tools" },
  { id: "comparer", label: "Comparer", icon: ArrowLeftRight, group: "tools" },
  { id: "logger", label: "Logger", icon: AlignLeft, group: "tools" },
  { id: "extender", label: "Extender", icon: Plug, group: "tools" },
  { id: "collaborator", label: "Collaborator", icon: Radar, group: "tools" },
  { id: "ai", label: "AI Assistant", icon: Brain, group: "tools" },
  { id: "project-options", label: "Project options", icon: SettingsIcon, group: "options" },
  { id: "user-options", label: "User options", icon: KeySquare, group: "options" },
];

export function App() {
  const ready = useAppStore((s) => s.ready);
  const initError = useAppStore((s) => s.initError);
  const proxyStatus = useAppStore((s) => s.proxy.status);
  const init = useAppStore((s) => s.init);
  const startProxy = useAppStore((s) => s.startProxy);
  const stopProxy = useAppStore((s) => s.stopProxy);
  const historyCount = useAppStore((s) => s.history.length);

  const [page, setPage] = useState<Page>("dashboard");

  useEffect(() => {
    init();
  }, [init]);

  const running = proxyStatus?.running ?? false;
  const listen = proxyStatus?.listen_addr ?? "—";

  const tools = useMemo(() => NAV.filter((n) => n.group === "tools"), []);
  const options = useMemo(() => NAV.filter((n) => n.group === "options"), []);

  const body = useMemo(() => {
    switch (page) {
      case "dashboard":
        return <DashboardPage onNavigate={setPage} />;
      case "target":
        return <TargetPage />;
      case "proxy":
        return <ProxyPage />;
      case "intruder":
        return <IntruderPage />;
      case "repeater":
        return <RepeaterPage />;
      case "sequencer":
        return <SequencerPage />;
      case "decoder":
        return <DecoderPage />;
      case "comparer":
        return <ComparerPage />;
      case "logger":
        return <LoggerPage />;
      case "extender":
        return <ExtenderPage />;
      case "collaborator":
        return <CollaboratorPage />;
      case "ai":
        return <AiAssistantPage />;
      case "project-options":
        return <ProjectOptionsPage />;
      case "user-options":
        return <UserOptionsPage />;
      default:
        return null;
    }
  }, [page]);

  return (
    <div className="app">
      <div className="title-bar">
        <div className="brand">
          <div className="logo" />
          <span>NyxProxy</span>
          <span style={{ color: "var(--text-muted)", fontSize: 11 }}>0.1.0 · Phase 1-3</span>
        </div>
        <div className={`status-pill ${running ? "running" : ""}`}>
          <span className="dot" />
          <span>{running ? `Listening · ${listen}` : "Proxy stopped"}</span>
        </div>
        <div className="filler" />
        <button
          className={`btn small ${running ? "danger" : "primary"}`}
          onClick={() => (running ? stopProxy() : startProxy())}
        >
          {running ? "Stop proxy" : "Start proxy"}
        </button>
      </div>

      <div className="sidebar">
        <div className="nav-section">Tools</div>
        {tools.map((n) => {
          const Icon = n.icon;
          const active = page === n.id;
          let badge: string | undefined;
          if (n.id === "proxy" || n.id === "logger") badge = String(historyCount);
          return (
            <div
              key={n.id}
              className={`nav-item ${active ? "active" : ""}`}
              onClick={() => setPage(n.id)}
            >
              <Icon size={16} />
              <span>{n.label}</span>
              {badge && <span className="badge">{badge}</span>}
              {n.id === "ai" && (
                <span className="badge" style={{ background: "var(--accent-soft)", color: "var(--accent)" }}>
                  AI
                </span>
              )}
            </div>
          );
        })}
        <div className="nav-section">Options</div>
        {options.map((n) => {
          const Icon = n.icon;
          const active = page === n.id;
          return (
            <div
              key={n.id}
              className={`nav-item ${active ? "active" : ""}`}
              onClick={() => setPage(n.id)}
            >
              <Icon size={16} />
              <span>{n.label}</span>
            </div>
          );
        })}
        <div className="nav-section" style={{ marginTop: "auto", padding: "8px 12px" }}>
          <div style={{ fontSize: 10, color: "var(--text-muted)" }}>
            <Radio size={11} style={{ verticalAlign: "middle" }} /> github.com/gitboyabhayt/nyxproxy
          </div>
        </div>
      </div>

      <div className="main">
        <div className="main-content">
          {!ready && (
            <div className="banner warning">
              Initializing engine… {initError ? `Error: ${initError}` : ""}
            </div>
          )}
          {body}
        </div>
      </div>

      <div className="status-bar">
        <FileSearch size={12} /> {historyCount} flows captured
        <span style={{ color: "var(--border-strong)" }}>·</span>
        <span>{running ? `Running on ${listen}` : "Idle"}</span>
        <div style={{ flex: 1 }} />
        <span>Phase 1 build — proxy core, AI gateway, full GUI</span>
      </div>

      <Toasts />
    </div>
  );
}
