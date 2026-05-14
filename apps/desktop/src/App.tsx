import { useCallback, useEffect, useMemo, useState } from "react";
import {
  Activity,
  AlignLeft,
  ArrowLeftRight,
  Brain,
  Code2,
  Crosshair,
  FileCheck2,
  FileJson,
  FileSearch,
  Gauge,
  Globe,
  KeySquare,
  ListTree,
  type LucideIcon,
  Menu,
  Plug,
  Plug2,
  PlaySquare,
  Radio,
  Radar,
  Repeat,
  Settings as SettingsIcon,
  Shield,
  Zap,
} from "lucide-react";

import { CommandPalette, type PaletteCommand } from "@/components/CommandPalette";
import { Toasts } from "@/components/Toasts";
import { AiAssistantPage } from "@/pages/AiAssistant";
import { AiAttackPage } from "@/pages/AiAttack";
import { CollaboratorPage } from "@/pages/Collaborator";
import { ComparerPage } from "@/pages/Comparer";
import { DashboardPage } from "@/pages/Dashboard";
import { DecoderPage } from "@/pages/Decoder";
import { ExtenderPage } from "@/pages/Extender";
import { IntruderPage } from "@/pages/Intruder";
import { LoggerPage } from "@/pages/Logger";
import { MacrosPage } from "@/pages/Macros";
import { CompliancePage } from "@/pages/Compliance";
import { GraphQLPage } from "@/pages/GraphQL";
import { OpenApiTestsPage } from "@/pages/OpenApiTests";
import { ProjectOptionsPage } from "@/pages/ProjectOptions";
import { ProxyPage } from "@/pages/Proxy";
import { RepeaterPage } from "@/pages/Repeater";
import { SequencerPage } from "@/pages/Sequencer";
import { TargetPage } from "@/pages/Target";
import { UserOptionsPage } from "@/pages/UserOptions";
import { WebSocketsPage } from "@/pages/WebSockets";
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
  | "macros"
  | "websockets"
  | "ai"
  | "ai-attack"
  | "openapi-tests"
  | "graphql"
  | "compliance"
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
  { id: "macros", label: "Macros", icon: PlaySquare, group: "tools" },
  { id: "websockets", label: "WebSockets", icon: Plug2, group: "tools" },
  { id: "ai", label: "AI Assistant", icon: Brain, group: "tools" },
  { id: "ai-attack", label: "AI Attack", icon: Zap, group: "tools" },
  { id: "openapi-tests", label: "OpenAPI tests", icon: FileJson, group: "tools" },
  { id: "graphql", label: "GraphQL", icon: Globe, group: "tools" },
  { id: "compliance", label: "Compliance", icon: FileCheck2, group: "tools" },
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
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(false);

  useEffect(() => {
    init();
  }, [init]);

  const running = proxyStatus?.running ?? false;
  const listen = proxyStatus?.listen_addr ?? "—";

  const navigate = useCallback((next: Page) => {
    setPage(next);
    setSidebarOpen(false);
  }, []);

  const tools = useMemo(() => NAV.filter((n) => n.group === "tools"), []);
  const options = useMemo(() => NAV.filter((n) => n.group === "options"), []);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const mod = e.ctrlKey || e.metaKey;
      if (mod && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPaletteOpen((v) => !v);
      } else if (e.key === "Escape" && paletteOpen) {
        setPaletteOpen(false);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [paletteOpen]);

  const paletteCommands = useMemo<PaletteCommand[]>(() => {
    const nav: PaletteCommand[] = NAV.map((n) => ({
      id: `goto-${n.id}`,
      label: `Go to ${n.label}`,
      group: n.group === "tools" ? "Navigate" : "Options",
      keywords: [n.id, n.label],
      run: () => navigate(n.id),
    }));
    const actions: PaletteCommand[] = [
      {
        id: "action-start-proxy",
        label: running ? "Stop proxy" : "Start proxy",
        group: "Proxy",
        shortcut: running ? "" : "",
        run: () => (running ? stopProxy() : startProxy()),
      },
      {
        id: "action-toggle-sidebar",
        label: sidebarOpen ? "Close sidebar" : "Open sidebar",
        group: "View",
        run: () => setSidebarOpen((v) => !v),
      },
    ];
    return [...nav, ...actions];
  }, [navigate, running, startProxy, stopProxy, sidebarOpen]);

  const body = useMemo(() => {
    switch (page) {
      case "dashboard":
        return <DashboardPage onNavigate={navigate} />;
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
      case "macros":
        return <MacrosPage />;
      case "websockets":
        return <WebSocketsPage />;
      case "ai":
        return <AiAssistantPage />;
      case "ai-attack":
        return <AiAttackPage />;
      case "openapi-tests":
        return <OpenApiTestsPage />;
      case "graphql":
        return <GraphQLPage />;
      case "compliance":
        return <CompliancePage />;
      case "project-options":
        return <ProjectOptionsPage />;
      case "user-options":
        return <UserOptionsPage />;
      default:
        return null;
    }
  }, [page, navigate]);

  return (
    <div className={`app ${sidebarOpen ? "sidebar-open" : ""}`}>
      <div className="title-bar">
        <button
          className="menu-toggle"
          aria-label="Toggle navigation"
          onClick={() => setSidebarOpen((v) => !v)}
        >
          <Menu size={16} />
        </button>
        <div className="brand">
          <div className="logo" />
          <span className="brand-text">NyxProxy</span>
          <span style={{ color: "var(--text-muted)", fontSize: 11 }}>0.1.0 · Phase 1-5</span>
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

      <div
        className="sidebar-scrim"
        onClick={() => setSidebarOpen(false)}
        aria-hidden="true"
      />

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
              onClick={() => navigate(n.id)}
              title={n.label}
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
              onClick={() => navigate(n.id)}
              title={n.label}
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
        <span>
          Phase 1–5 build · proxy · scanner · intruder · macros · plugins · AI
        </span>
      </div>

      <Toasts />
      <CommandPalette
        open={paletteOpen}
        commands={paletteCommands}
        onClose={() => setPaletteOpen(false)}
      />
    </div>
  );
}
