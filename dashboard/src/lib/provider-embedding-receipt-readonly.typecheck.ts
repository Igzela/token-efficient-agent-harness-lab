import type {
  LocalDashboardState,
  ProviderEmbeddingReceiptEvidence,
} from "./types";

declare const dashboard: LocalDashboardState;
declare const receipt: ProviderEmbeddingReceiptEvidence;

// @ts-expect-error Provider receipt evidence is read-only.
receipt.state = "failed_known_outcome";
// @ts-expect-error Provider receipt collections are read-only.
dashboard.provider_embedding_receipts.push(receipt);
// @ts-expect-error Modified dashboard DTO fields remain read-only.
dashboard.status = "mutated";

