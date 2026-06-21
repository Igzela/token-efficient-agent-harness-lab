import { useState } from "react";
import type { AdaptivePolicyPromotionRequest } from "@/lib/types";

const promotionTemplate = JSON.stringify(
  {
    actor: "operator",
    promotion: {
      task_class: "docs cleanup",
      objective: "efficient",
      candidate_id: "candidate-fast",
      baseline_candidate_id: "candidate-safe",
      sample_count: 30,
      confidence: 0.9,
      mean_quality_delta: 0,
      mean_cost_reduction: 0.2,
      failure_rate_delta: -0.01,
      evidence_run_ids: ["dispatch-0001"],
      risk_level: "low",
      confirm_adaptive_policy_promotion: true,
    },
  },
  null,
  2,
);

function parsePromotionRequest(raw: string): AdaptivePolicyPromotionRequest {
  const parsed = JSON.parse(raw) as unknown;
  if (!parsed || typeof parsed !== "object") {
    throw new Error("Promotion request must be a JSON object.");
  }
  const request = parsed as AdaptivePolicyPromotionRequest;
  if (!request.promotion || typeof request.promotion !== "object") {
    throw new Error("Promotion request must include a promotion object.");
  }
  if (request.promotion.confirm_adaptive_policy_promotion !== true) {
    throw new Error("promotion.confirm_adaptive_policy_promotion must be true.");
  }
  return request;
}

export function AdaptiveFusionPromotionForm({
  busy,
  onSubmit,
}: {
  busy: boolean;
  onSubmit: (request: AdaptivePolicyPromotionRequest) => Promise<void>;
}) {
  const [promotionJson, setPromotionJson] = useState(promotionTemplate);

  async function submitPromotion() {
    const request = parsePromotionRequest(promotionJson);
    await onSubmit(request);
  }

  return (
    <div className="subcard stack">
      <h3>Promote Policy</h3>
      <label className="muted" htmlFor="adaptive-promotion-json">
        Promotion request JSON
      </label>
      <textarea
        id="adaptive-promotion-json"
        onChange={(event) => setPromotionJson(event.target.value)}
        rows={16}
        spellCheck={false}
        value={promotionJson}
      />
      <button disabled={busy} onClick={submitPromotion} type="button">
        Submit Promotion
      </button>
    </div>
  );
}
