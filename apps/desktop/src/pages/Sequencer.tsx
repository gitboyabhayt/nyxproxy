import { useMemo, useState } from "react";

import { SplitPane } from "@/components/SplitPane";
import { SequencerApi } from "@/tauri/api";
import { useAppStore } from "@/state/store";
import type { SequencerReport } from "@/tauri/types";

export function SequencerPage() {
  const toast = useAppStore((s) => s.toast);
  const [text, setText] = useState<string>(
    [
      "x7f9aaccf",
      "x7f9aaccg",
      "x7f9aacch",
      "x7f9aacci",
      "x7f9aaccj",
    ].join("\n")
  );
  const [report, setReport] = useState<SequencerReport | null>(null);

  const samples = useMemo(
    () =>
      text
        .split(/\r?\n/)
        .map((s) => s.trim())
        .filter((s) => s.length > 0),
    [text]
  );

  const analyze = async () => {
    try {
      const r = await SequencerApi.analyze(samples);
      setReport(r);
    } catch (err) {
      toast("error", `Analysis failed: ${err}`);
    }
  };

  return (
    <SplitPane
      storageKey="sequencer"
      initialSize={0.4}
      first={
        <div className="panel" style={{ height: "100%", border: "none", borderRadius: 0 }}>
          <div className="panel-header">
            Samples ({samples.length})
            <button className="btn primary small" style={{ marginLeft: "auto" }} onClick={analyze}>
              Analyze
            </button>
          </div>
          <div className="panel-body" style={{ padding: 10 }}>
            <textarea
              className="code-input"
              style={{ flex: 1, minHeight: 300 }}
              value={text}
              onChange={(e) => setText(e.target.value)}
              placeholder={"One sample per line — paste cookies, anti-CSRF tokens, session IDs…"}
            />
          </div>
        </div>
      }
      second={
        <div className="panel" style={{ height: "100%", border: "none", borderRadius: 0 }}>
          <div className="panel-header">Report</div>
          <div className="panel-body" style={{ overflow: "auto", padding: 14 }}>
            {!report ? (
              <div className="empty-state">
                <p>Click Analyze to compute Shannon entropy, uniqueness, and per-class byte distribution.</p>
              </div>
            ) : (
              <>
                <div className="cards">
                  <div className="card">
                    <div className="label">Samples</div>
                    <div className="value">{report.samples}</div>
                  </div>
                  <div className="card">
                    <div className="label">Mean length</div>
                    <div className="value">{report.mean_length.toFixed(1)}</div>
                  </div>
                  <div className="card">
                    <div className="label">Shannon entropy</div>
                    <div
                      className="value"
                      style={{
                        color:
                          report.shannon_entropy_bits >= 5
                            ? "var(--success)"
                            : report.shannon_entropy_bits >= 3
                            ? "var(--warning)"
                            : "var(--danger)",
                      }}
                    >
                      {report.shannon_entropy_bits.toFixed(2)} bits
                    </div>
                  </div>
                  <div className="card">
                    <div className="label">Uniqueness</div>
                    <div className="value">{(report.uniqueness_ratio * 100).toFixed(1)}%</div>
                  </div>
                </div>
                <div className="panel" style={{ marginTop: 14 }}>
                  <div className="panel-header">Character classes</div>
                  <div className="panel-body" style={{ padding: 10 }}>
                    <table className="data-table">
                      <thead>
                        <tr>
                          <th>Class</th>
                          <th>Count</th>
                        </tr>
                      </thead>
                      <tbody>
                        {Object.entries(report.character_classes).map(([k, v]) => (
                          <tr key={k}>
                            <td>{k}</td>
                            <td>{v}</td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                </div>
              </>
            )}
          </div>
        </div>
      }
    />
  );
}
