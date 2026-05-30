import { useState } from "react";

export function KeyRevealModal({
  rawKey,
  label,
  onClose,
}: {
  rawKey: string;
  label: string;
  onClose: () => void;
}) {
  const [copied, setCopied] = useState(false);

  async function handleCopy() {
    try {
      await navigator.clipboard.writeText(rawKey);
      setCopied(true);
    } catch {
      const el = document.getElementById("key-reveal-value");
      if (el) {
        const range = document.createRange();
        range.selectNodeContents(el);
        const sel = window.getSelection();
        sel?.removeAllRanges();
        sel?.addRange(range);
      }
    }
  }

  return (
    <div className="confirm-overlay" onClick={onClose}>
      <div className="confirm-card key-reveal-card" onClick={(e) => e.stopPropagation()}>
        <h3 style={{ margin: "0 0 12px" }}>{label}</h3>
        <p className="key-reveal-warning">This key is shown only once. Copy it now — it cannot be retrieved later.</p>
        <div id="key-reveal-value" className="key-reveal-value">{rawKey}</div>
        <div style={{ display: "flex", gap: 8, justifyContent: "flex-end", marginTop: 16 }}>
          <button onClick={onClose} type="button">Close</button>
          <button onClick={handleCopy} type="button">
            {copied ? "Copied!" : "Copy"}
          </button>
        </div>
      </div>
    </div>
  );
}
