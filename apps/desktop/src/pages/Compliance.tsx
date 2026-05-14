import { useEffect, useState } from "react";

import {
  ComplianceApi,
  type ComplianceFramework,
  type ComplianceReport,
} from "@/tauri/api";
import type { Issue } from "@/tauri/types";
import { invoke } from "@/tauri/api";
import { useAppStore } from "@/state/store";

const ALL_FRAMEWORKS: { id: ComplianceFramework; label: string }[] = [
  { id: "pci-dss", label: "PCI-DSS v4.0" },
  { id: "iso27001", label: "ISO/IEC 27001:2022" },
  { id: "soc2", label: "SOC 2 (TSC 2017)" },
  { id: "hipaa", label: "HIPAA Security Rule" },
  { id: "gdpr", label: "GDPR" },
];

export function CompliancePage() {
  const toast = useAppStore((s) => s.toast);
  const [issues, setIssues] = useState<Issue[]>([]);
  const [frameworks, setFrameworks] = useState<ComplianceFramework[]>([
    "pci-dss",
    "iso27001",
    "soc2",
  ]);
  const [report, setReport] = useState<ComplianceReport | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    void (async () => {
      try {
        const all = await invoke<Issue[]>("scanner_scan_history");
        setIssues(all);
      } catch (err) {
        toast("error", `Could not load findings: ${err}`);
      }
    })();
  }, [toast]);

  function toggle(framework: ComplianceFramework): void {
    setFrameworks((prev) =>
      prev.includes(framework)
        ? prev.filter((f) => f !== framework)
        : [...prev, framework],
    );
  }

  async function build(): Promise<void> {
    if (frameworks.length === 0) {
      toast("error", "Pick at least one framework");
      return;
    }
    setBusy(true);
    try {
      const r = await ComplianceApi.build(issues, frameworks);
      setReport(r);
      toast(
        "info",
        `Report built — ${r.findings.length} findings across ${r.frameworks.length} frameworks`,
      );
    } catch (err) {
      toast("error", `Report build failed: ${err}`);
    } finally {
      setBusy(false);
    }
  }

  async function exportHtml(): Promise<void> {
    if (!report) return;
    try {
      const html = await ComplianceApi.renderHtml(report);
      const blob = new Blob([html], { type: "text/html" });
      const url = URL.createObjectURL(blob);
      const win = window.open(url, "_blank");
      if (!win) {
        // Browser dev fallback: download
        const a = document.createElement("a");
        a.href = url;
        a.download = "nyxproxy-compliance.html";
        a.click();
      }
    } catch (err) {
      toast("error", `HTML render failed: ${err}`);
    }
  }

  async function exportMarkdown(): Promise<void> {
    if (!report) return;
    try {
      const md = await ComplianceApi.renderMarkdown(report);
      await navigator.clipboard.writeText(md);
      toast("info", "Markdown copied to clipboard");
    } catch (err) {
      toast("error", `Markdown render failed: ${err}`);
    }
  }

  return (
    <div className="flex h-full flex-col gap-4 p-4">
      <header className="flex items-baseline justify-between">
        <h1 className="text-xl font-semibold">Compliance reports</h1>
        <span className="text-xs text-muted-foreground">Feature II</span>
      </header>

      <section className="rounded border border-border bg-card p-3">
        <h2 className="mb-2 text-sm font-medium">Frameworks</h2>
        <div className="flex flex-wrap gap-2">
          {ALL_FRAMEWORKS.map((f) => (
            <label
              key={f.id}
              className="flex items-center gap-2 rounded border border-border px-2 py-1 text-xs"
            >
              <input
                type="checkbox"
                checked={frameworks.includes(f.id)}
                onChange={() => toggle(f.id)}
              />
              {f.label}
            </label>
          ))}
        </div>
        <div className="mt-3 flex items-center gap-2">
          <button
            type="button"
            className="rounded bg-primary px-3 py-1 text-xs font-medium text-primary-foreground disabled:opacity-50"
            onClick={() => void build()}
            disabled={busy}
          >
            Build report
          </button>
          <button
            type="button"
            className="rounded border border-border px-3 py-1 text-xs disabled:opacity-50"
            onClick={() => void exportHtml()}
            disabled={!report}
          >
            View HTML
          </button>
          <button
            type="button"
            className="rounded border border-border px-3 py-1 text-xs disabled:opacity-50"
            onClick={() => void exportMarkdown()}
            disabled={!report}
          >
            Copy Markdown
          </button>
          <span className="text-xs text-muted-foreground">
            {issues.length} findings will be mapped.
          </span>
        </div>
      </section>

      {report ? (
        <section className="flex-1 overflow-auto rounded border border-border bg-card p-3">
          <h2 className="mb-2 text-sm font-medium">Per-control coverage</h2>
          <table className="w-full text-xs">
            <thead>
              <tr className="text-left text-muted-foreground">
                <th className="pb-1">Framework</th>
                <th className="pb-1">Control</th>
                <th className="pb-1">Title</th>
                <th className="pb-1 text-right">Findings</th>
              </tr>
            </thead>
            <tbody>
              {report.coverage.map((c) => (
                <tr
                  key={`${c.framework}-${c.control_id}`}
                  className="border-t border-border"
                >
                  <td className="py-1 font-mono">{c.framework}</td>
                  <td className="py-1 font-mono">{c.control_id}</td>
                  <td className="py-1">{c.control_title}</td>
                  <td className="py-1 text-right">{c.finding_count}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </section>
      ) : null}
    </div>
  );
}
