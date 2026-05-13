import { useCallback, useEffect, useRef, useState } from "react";

interface SplitPaneProps {
  direction?: "horizontal" | "vertical";
  initialSize?: number;
  minFirst?: number;
  minSecond?: number;
  first: React.ReactNode;
  second: React.ReactNode;
  storageKey?: string;
}

export function SplitPane({
  direction = "horizontal",
  initialSize = 0.5,
  minFirst = 120,
  minSecond = 120,
  first,
  second,
  storageKey,
}: SplitPaneProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [ratio, setRatio] = useState<number>(() => {
    if (storageKey) {
      const stored = localStorage.getItem(`split:${storageKey}`);
      if (stored) {
        const parsed = parseFloat(stored);
        if (!Number.isNaN(parsed) && parsed > 0 && parsed < 1) return parsed;
      }
    }
    return initialSize;
  });

  useEffect(() => {
    if (storageKey) localStorage.setItem(`split:${storageKey}`, ratio.toString());
  }, [ratio, storageKey]);

  const dragging = useRef(false);

  const onMouseDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    dragging.current = true;
    document.body.style.userSelect = "none";
  }, []);

  useEffect(() => {
    function onMove(e: MouseEvent) {
      if (!dragging.current || !containerRef.current) return;
      const rect = containerRef.current.getBoundingClientRect();
      const total = direction === "horizontal" ? rect.width : rect.height;
      const offset =
        direction === "horizontal" ? e.clientX - rect.left : e.clientY - rect.top;
      const minStart = minFirst / total;
      const minEnd = 1 - minSecond / total;
      const next = Math.max(minStart, Math.min(minEnd, offset / total));
      setRatio(next);
    }
    function onUp() {
      dragging.current = false;
      document.body.style.userSelect = "";
    }
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    return () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
  }, [direction, minFirst, minSecond]);

  const firstStyle: React.CSSProperties = {
    flexBasis: `${ratio * 100}%`,
  };
  const secondStyle: React.CSSProperties = {
    flexBasis: `${(1 - ratio) * 100}%`,
  };

  return (
    <div
      ref={containerRef}
      className={`split ${direction === "vertical" ? "vertical" : ""}`}
    >
      <div className="split-pane" style={firstStyle}>
        {first}
      </div>
      <div className="split-divider" onMouseDown={onMouseDown} />
      <div className="split-pane" style={secondStyle}>
        {second}
      </div>
    </div>
  );
}
