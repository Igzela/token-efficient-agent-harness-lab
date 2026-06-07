import { useEffect, useRef, useState } from "react";

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
  const closeRef = useRef<HTMLButtonElement>(null);
  const cardRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    closeRef.current?.focus();

    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
      if (e.key === "Tab" && cardRef.current) {
        const focusable = cardRef.current.querySelectorAll<HTMLElement>(
          'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
        );
        if (focusable.length === 0) return;
        const first = focusable[0];
        const last = focusable[focusable.length - 1];
        if (e.shiftKey && document.activeElement === first) {
          e.preventDefault();
          last.focus();
        } else if (!e.shiftKey && document.activeElement === last) {
          e.preventDefault();
          first.focus();
        }
      }
    }

    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [onClose]);

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
    <div className="confirm-overlay" onClick={onClose} role="dialog" aria-modal="true" aria-label={label}>
      <div className="confirm-card key-reveal-card" onClick={(e) => e.stopPropagation()} ref={cardRef}>
        <h3 className="modal-title">{label}</h3>
        <p className="key-reveal-warning">This key is shown only once. Copy it now; it cannot be retrieved later.</p>
        <div id="key-reveal-value" className="key-reveal-value">{rawKey}</div>
        <div className="modal-actions">
          <button onClick={onClose} type="button" ref={closeRef}>Close</button>
          <button onClick={handleCopy} type="button" className="button-primary">
            {copied ? "Copied!" : "Copy"}
          </button>
        </div>
      </div>
    </div>
  );
}
