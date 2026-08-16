# Current Status

Last updated: 2026-08-16.

This document owns accepted repository truth and confirmed capability gaps only. It separates two states that must not be conflated:

1. **Merged and accepted truth** — code and governing documents on remote `main` that passed their required checks.
2. **Confirmed gaps** — capabilities or evidence not yet accepted on `main`.

Open PR heads, Draft/Ready state, CI, reviews, mergeability, and the next permitted action are live observations and must come from a fresh context capsule. Current execution routing belongs in `docs/NEXT_DECISION.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`. Historical packet detail remains in Git history and merged PRs; do not append stale chronology here.

## Verified Repository State

- Repository: `Igzela/token-efficient-agent-harness-lab`.
- Active runtime/code baseline: PR #457 exact head `6525907a7752e90dd31469aa8fd8bbfd08f01209` squash-merged as `e6ab1a1f5516ad52c0d1b431a5b1d52e990f90d7`; it contains the bounded RWE journal-accounting, strict managed-action parsing, and fail-closed ProductTask cleanup repair. Always refresh remote `main` for the current canonical-document head.
- PR #368 accepted the provider timeout-ownership repair from exact head `17cc5d03…`; its exact-head review receipt reports `PASS` with no open findings, and its canonical exact-head workflow completed successfully before merge.
- PR #369 accepted the operator-gated compatibility-calibration mechanism from exact head `b571c95a75a7c8eacda99a8f586d8f2360868ab7`; canonical exact-head workflow `31172449577` completed successfully and the exact-head review receipt reports `PASS` with no open findings.
- PR #370 accepted the versioned v2 RWE refreeze from exact head `36c92b93975366c3f85471f247a3afb128e5351c`; exact-head review reports `PASS` with no open objections, canonical workflow `31312135471` completed every required job successfully, and the merged v2 corpus/protocol/schedule hashes are `044fcd7b…`, `bc68bfb3…`, and `6a729f12…`.
- PR #375 accepted the bounded autonomous worker executable session routing and fail-closed recovery repair (superseding rejected #374) from exact head `2aea374e2ff26b798a104884658e3af5c6a378e4`; merge `63fbc4e264d2a1f2250299e25dcf168d71376aef`; exact-head `PASS` receipts on both review axes with no open objections; canonical exact-head workflow completed every required job including `pg-integration-tests` and the terminal context capsule. The same PR carried a bounded repair de-rotting 16 hard-coded `2026-07` timestamps in `engine/tests/test_pg_integration.rs` that crossed the strict 30-day operator-decision freshness bound on 2026-08-10 (pre-existing `main` calendar rot, reproduced on a pristine `origin/main` worktree before the PR was touched); test-data only, zero production code.
- PR #380 accepted the repository-maintenance route contract from exact head `e905cf6ec7a989b54e60f913657ca306f33ebf49`; merge `546cabc1ceb98b49b543d0bd90a62fc228e67338`; exact-head `PASS` receipts on both review axes with no open objections; canonical workflow `31386777810` completed every required job successfully. It established the single route-controller boundary without activating Plan execution.
- PR #408 accepted the bounded control-binding integrity repair from exact head `4a2dcf42728ae53f7daaec73e15310e8b0d67b59`; merge `57a86c78c3f9611ce48c5bce249721af23db5532`; both independent review axes returned exact `PASS`; canonical workflow `31593460813` completed every required job successfully. Authority was accepted separately through PR #406 merge `83d735feb157b1ef60501cdfea1ecf5b7f3d05ef`, PR #407 merge `a4b33f942fcc3515a1a32916daa2044ca9fbf54e`, and the planning-only closeout amendment PR #409 merge `d278563b4694c629770a3a7673b1283c6e58568d`.
- The PR #370 evidence binds the external calibration to accepted checkout `ee43eac…`, first-viable 8,192 output tokens, one of at most two requests, and SHA-256 digests for both the restricted raw bundle and redacted receipt. Those digests are evidence references, not permission to expose or commit the restricted bundle.
- A new `main`, PR head, CI result, review receipt, or canonical-document change invalidates older context capsules and branch-local status prose.

## Accepted Packet Receipts

This table is the durable cross-document prerequisite index. A packet may appear here only after merge, exact-head review, canonical CI, and canonical-document synchronization have all been established; live PR state still comes from a fresh capsule.

| Packet | State | Accepted evidence |
|---|---|---|
| `PE7-AC0-DATA-CONTRACT-INVENTORY-1` | `COMPLETE` | PR #466 exact head `1662bde29a53d942a28a9982cc5e9a999ff44c12`; merge `e17767e6ebe1a0d6c6031dec61349deeb3ef9585`; exact-head `PASS`; canonical workflow `31868206197` |
| `PE7-AC0-TRACE-ORDER-FREEZE-1` | `COMPLETE` | PR #467 exact head `19cc238fec27236873262c12998eabe2eda26ac4`; merge `a4879fc60f1c080579df7ba942793a4c94367ff5`; exact-head `PASS`; canonical workflow `31869014363` |
| `PE7-AC2-CONTRACT-1` | `COMPLETE` | PR #469 exact head `142fad048f1d9e8dfb40aa61145108a2fe48f871`; merge `591f8c607804813fe0b809f92f494cb6bcee7820`; exact-head `PASS`; canonical workflow `31871125792` |
| `PE7-AC2-BOUNDARY-REPAIR-1` | `COMPLETE` | PR #475 exact head `e89a24bd6776282bbd52ee72cc5be8ecc66acbc2`; merge `b75cd81620ed51aefce5d245855cf00f1bb6385b`; exact-head `PASS` (receipt comment `5302063445`); canonical workflow `31882791484` |
| `PE7-AC2-CALLER-MIGRATION-1` | `COMPLETE` | PR #478 exact head `4c748ce5f7988da9f61dd1e4650351b5d6c8bf72`; merge `36d7b33a5483cff63715b7981794aff1de614ae2`; exact-head `PASS`; canonical workflow `31886172712` |
| `PE7-RWE-V2-REFREEZE-1` | `COMPLETE` | PR #370 exact head `36c92b93975366c3f85471f247a3afb128e5351c`; merge `3b4afb3e5ab4254904aa5a63473ab6ae0eac1e82`; exact-head `PASS`; canonical workflow `31312135471`; redacted calibration and restricted-bundle digests bound in the PR evidence |
| `PE7-CTRL-ROUTE-CONTRACT-1` | `COMPLETE` | PR #380 exact head `e905cf6ec7a989b54e60f913657ca306f33ebf49`; merge `546cabc1ceb98b49b543d0bd90a62fc228e67338`; exact-head `PASS`; canonical workflow `31386777810`; route-contract receipt bound to the accepted main merge |
| `PE7-PLAN-LANE-ACTIVATION-1` | `COMPLETE` | PR #382 exact head `dde26f884ce8a85b776b5933c84c4e6cfd73cb19`; merge `e55e19f1b7c353b4baa2b40ee7b5b16af8918a6c`; exact-head `PASS`; canonical workflow `31395404498` (native-runtime rerun after a confirmed infra-only OpenSSL linker flake); Plan lane active behind real terminal-owner readiness; Plan Execution Ledger Issue #383 provisioned |
| `PE7-LIFECYCLE-CONTROLLER-1` | `COMPLETE` | PR #385 exact head `5867eb9e35151c8252cda26bb6a956dfe80252b0`; merge `ca7e4585c594a5c9820c8d1267858780c28503ac`; exact-head `PASS`; canonical workflow `31401184171`; plan-packet CI/review/merge/closeout receipts recorded on the ledger as controller-owned transitions with idempotent readback |
| `PE7-SUCCESSOR-PROMOTION-ESCALATION-1` | `COMPLETE` | PR #387 exact head `5fe3d55af19aa7a081115637f8f8a7aa63b581af`; merge `597f90282fb6ca72472b890b825684bf54486709`; exact-head `PASS`; canonical workflow `31403849100`; exactly-one successor-promotion receipts (packet id, accepted-main SHA, capsule digest) and bounded pause escalation are controller-owned on the Plan Execution Ledger |
| `PE7-ROUTE-AUTOMATION-1` | `COMPLETE` | PR #390 exact head `24618e52c969adc93e7bc092c51dde6b2d0ffea9`; merge `5481053c736e7db8481cabd9316741f2a5cd6c7a`; exact-head `PASS`; canonical workflow `31467821768` |
| `PE7-CONTROL-BINDING-INTEGRITY-REPAIR-1` | `COMPLETE` | Authority PRs #406/#407/#409; implementation PR #408 exact head `4a2dcf42728ae53f7daaec73e15310e8b0d67b59`; merge `57a86c78c3f9611ce48c5bce249721af23db5532`; exact-head `PASS` on both review axes; canonical workflow `31593460813`; #405 retrospective correction workflow `31594277043` and production readback bind actual head `e68ec0b3a7b78d3ca241922bf3995c2f3ba4ecfa` while retaining `historical_merge_compliant=false` |
| `PE7-WORKSPACE-PREP-RECEIPT-RACE-REPAIR-1` | `COMPLETE` | PR #413 base `59cec5745ddd7f89ce8c099a5de2c7e3c3ec3a1e`; exact head `fc8c005981d2fa12f0f494a131b839d65a46a8ba`; exact-head `PASS` receipt comment `5268787985`; canonical workflow `31611860646`; merge `9cc118fa72d9d13a24cdf968cc5fc20dbe80b28f`; deterministic production-path concurrent-winner receipt reuse and genuine missing-receipt rejection |
| `PE7-ROUTE-AUTONOMY-STABILIZATION-1` | `COMPLETE` | PR #416 exact head `9ce548f620314303b37753a18539c17b5daa6698`; merge `306b500c43270ca83d7cb9defd365140b525187c`; exact-head `PASS`; canonical workflow `31630036965` |
| `PE7-ROUTE-AUTOPILOT-SOAK-1` | `COMPLETE` | Closeout PR #429 exact head `92e9b49c13b51ee9c471a6acc2181c37d8084029`; merge `d40c8ce82101922e7270f30bd6da592d72354ffe`; exact-head `PASS`; canonical workflow `31681024633`; OpenCode worker PR #426 exact head `c54860674fbf5045239469c2a842ec88002bb3df`; merge `f02d58b5d1fb8d74dd1c68349e4075eb7641879e`; ledger #383 trusted CI/review/merge/closeout; canonical workflow `31664342318` |
| `PE7-RWE-V2-PREFLIGHT-GATE-CONTRACT-1` | `COMPLETE` | PR #432 exact head `f31ba002720424deb003728eec52aa9ceae35e33`; merge `710ce06fee68fb75889aa5fa3b9e031b4fdc3a50`; exact-head `PASS`; canonical workflow `31686429471`; contract digest `c8ea4c802e2554b1fa5d0b2f247879ba758d67e4d5df23ed43f1eddadf8aef74` |
| `PE7-RWE-V2-PREFLIGHT-GATE-REPAIR-1` | `COMPLETE` | PR #434 exact head `9fdd1045928f862a5b1c1017bc0e9d73e5d50966`; merge `e311db76bf4d2a3a407213b8129a600bc447fd56`; exact-head `PASS`; canonical workflow `31690000442`; durable B2 rule caller-supplied finite expires_at |
| `PE7-RWE-V2-VIABILITY-PREFLIGHT-1` | `COMPLETE` | PR #437 exact head `4bf6f33c9318369c99a0920eac2048527bea2e83`; merge `97ca257345460e1939662b8ffaf602c0a668028a`; exact-head `PASS`; canonical workflow `31698417170`; unissued request sha256 `015c94e9d65a902f3aba5eae4f3da6cba6d534cc3c57af3a6faf89125663469a` |
| `PE7-RWE-V2-VIABILITY-RUN-1` | `COMPLETE` | PR #441 exact head `ba47462d6cd200d28cb55b1f547924b52afa0584`; merge `2933ba1353f1cda3fc82209b6025094afb79b29e`; exact-head `PASS`; canonical workflow `31704360890`; 4/4 controlled_failure run-live-20260813-v2c auth-live-v2-003 |
| `PE7-RWE-V2-VIABILITY-CLOSEOUT-1` | `COMPLETE` | Disposition `CONTROLLED_FAILURE`; run `run-live-20260813-v2c`; authorization `auth-live-v2-003`; cells `cell-rwe-minimum-t1-fix_flow_linkage-r1-bp-standard-s2026080601`, `cell-rwe-minimum-t1-fix_flow_linkage-r2-bp-standard-s2026080602`, `cell-rwe-minimum-t2-draft_contract_tests-r1-bp-standard-s2026080601`, `cell-rwe-minimum-t2-draft_contract_tests-r2-bp-standard-s2026080602`; restricted-bundle sha256 `9b345faf744c14d67157856a512b39d90c6e03ff1081783c793b987d6f93bf82`; redacted-bundle sha256 `e2eafa226700061cb000b35dec776ef0b49417aa5faece0b065923b49ee83d3f`; promotion PR #442 exact head `50e18540f40a8d47c384f2cac74683618f93c273`; merge `8c5c2f85bc5d66c08d730b7d0c69d914af19540c`; canonical workflow `31710478692` |
| `PE7-RWE-MR-ESTIMANDS-1` | `COMPLETE` | PR #444 exact head `c3b61d1ecd898abfab910f0c2f5c33fa6692acef`; merge `4a0048fcb6785adfb3614769298519c95a01de2f`; exact-head `PASS`; canonical workflow `31765676789`; exact-head check `31765676776` |
| `PE7-RWE-MR-CORPUS-SAMPLING-1` | `COMPLETE` | PR #445 exact head `2ddec8d5e2afce104ee718d64eb517219ecdf888`; merge `3f88d985af3f7701ab9f3c382becb84f73364c9b`; exact-head `PASS`; canonical workflow `31766911605`; exact-head check `31766911606` |
| `PE7-RWE-MR-OPERATIONS-EVIDENCE-1` | `COMPLETE` | PR #446 exact head `34c68d94c1737769c60fb7ea1722b464a5d764aa`; merge `e34d1ae3c3ecf5e6c919c71a3d26d6690a66444`; exact-head review receipt comment `5289427966`; canonical workflow `31769511015`; exact-head check `31769511065` |
| `PE7-RWE-MR-PROTOCOL-FREEZE-1` | `COMPLETE` | PR #447 exact head `00c8592676c5f73447f94b3abc1361087b371196`; merge `f575b10a6de617bf3dab5611900bf0a48727c0c6`; exact-head review receipt comment `5289552091`; canonical workflow `31770551762`; exact-head check `31770551749`; manifest sha256 `b5e37c7c2419a3acb42a8f21dbf2ba56aa8ddabb995b84b644f1b116a3321c12` |
| `PE7-RWE-DB-SNAPSHOT-CORPUS-1` | `COMPLETE` | PR #448 exact head `923d9f750c652a268b3d7944be35f34c2a2f9fac`; merge `a4472b9a0aa9c78d1616e9d22c88c2f6a6405cb8`; exact-head review receipt comment `5289908799`; canonical workflow `31773697000`; exact-head check `31773696854`; manifest sha256 `d13834c8ad41376f2884c906b335dce3a397fa0464ba83da0af6310fe2837ce2`; initial disposition `UNAVAILABLE_NOW`, `reconstructable=false` |
| `PE7-RWE-DB-SNAPSHOT-RECONSTRUCT-1` | `COMPLETE` | PR #451 exact head `d48e9853856714a964709956651fc0ac0961315c`; squash merge `e1ff80b7599d8aec8d64909f937f79c948010392`; exact-head review receipt comment `5292004476`; canonical workflow `31790256137`; exact-head checks `31790143998` and `31790256110`; manifest sha256 `a423ea9889dfc32680f660312bf61d95e5c2a26c49fc52143b26b8d9847c9c8c`; `RECONSTRUCTABLE=true` |
| `PE7-RWE-DB-PREFLIGHT-1` | `COMPLETE` | Real same-tenant Store-owned Golden Path prerequisite completed by ProductTask `ptask-20260814135947-18cbb0b9731e62bf`, run `run-0007`, terminal evidence `product-terminal-ptask-20260814135947-18cbb0b9731e62bf-9-44d49301c781`; Draft PR-only output `Igzela/alters-lab#47`, target main `6240768506320a324d68787b9eaa86971c8c930c`; provider-free `rwe_operator_preflight.v1` `ready=true`, zero blockers, no authority/provider/target effect; projection sha256 `b8c35d4060d98598ce3e3bc3977a84d125b1df09ff66b2b9f6d9aa4303c03954` |

| `PE7-AC3-CONTRACT-1` | `COMPLETE` | PR #486 exact head `9487a73ab9e00018103193d18c848e375b215a1b`; merge `6b2a6c46d30089800394ee82edd21075a2ef0d86`; exact-head `PASS`; canonical workflow `31922547776` |
| `PE7-AC3-ORCHESTRATOR-CORE-1` | `COMPLETE` | PR #488 exact head `d8d1ac7c7ce2911486c7737de63647e1ddf7a476`; merge `650a9102faf183d46064c869dd5d76db8e44b64f`; exact-head `PASS`; canonical workflow `31927020088` |
| `PE7-AC3-PORT-MIGRATION-1` | `COMPLETE` | PR #489 exact head `9495baee75a53f4908acdad44f804f2b93596672`; merge `02dcfb7d094e0ef9c6317ad800eab1ca1c957dd9`; exact-head `PASS`; canonical workflow `31927787705` |
| `PE7-AC4-CONTRACT-1` | `COMPLETE` | PR #490 exact head `61d702dd166b430bdc64634e94d8157443f2ac25`; merge `40c648a6de7e2288485af936d6274068a3d1b394`; exact-head `PASS`; canonical workflow `31928462592` |
| `PE7-AC4-VIEWS-CORE-1` | `COMPLETE` | PR #491 exact head `ec91c8f7fe181c9a49c4a7c9146654bfb1956971`; merge `7c6b5a3115febd76906f04dcc14d5b9d0dc96f77`; exact-head `PASS`; canonical workflow `31928817866` |
| `PE7-AC4-CALLER-MIGRATION-1` | `COMPLETE` | PR #493 exact head `e7ba6ac3fffd28fc4456466ce4387d062ca6a882`; merge `48f85425477bd141f1237bf9f9a62fd9ceaa5602`; exact-head `PASS`; canonical workflow `31929528758` |
| `PE7-AC5-CONTRACT-1` | `COMPLETE` | PR #494 exact head `d35df3ae6c9be98678eacbee1a42f96087d375c0`; merge `437f2b8533d557fc6ae87ebe0840aaeb30489689`; exact-head `PASS`; canonical workflow `31930174257` |
| `PE7-AC5-ROOT-CORE-1` | `COMPLETE` | PR #495 exact head `edb34b0482b142614d100f532c64ce7ac6481f2b`; merge `2453ec1adcb7a252d61b1234ab7da36678caf640`; exact-head `PASS`; canonical workflow `31930486388` |
| `PE7-AC5-MODULE-MIGRATION-1` | `COMPLETE` | PR #497 exact head `b493e784ad53af8e5fccd519718a4f492da30c95`; merge `79cb356e371cc7f243a7ccceed3b3b3e90323689`; exact-head `PASS`; canonical workflow `31931211944` |
| `PE7-AC6-CONTRACT-1` | `COMPLETE` | PR #499 exact head `4cc7f48589ff98706cc66f002b017b7cf6e73748`; merge `9a7dbb2ddb7887b62904b13988cd6217d504d31b`; exact-head `PASS`; canonical workflow `31931854403` |
| `PE7-AC6-RUST-CODEGEN-1` | `COMPLETE` | PR #500 exact head `9e4e2a1cc81b5f8ffa434d734e57dc083d8b395f`; merge `516418883104db21449d0d4fdba63e5b837a463d`; exact-head `PASS`; canonical workflow `31932059394` |
| `PE7-AC6-SDK-MIGRATION-1` | `COMPLETE` | PR #501 exact head `06543f88768e40b1670eeace8ab277aef495ca8e`; merge `3d680b21fa3b007424dd1104dda28a1fe01c9862`; exact-head `PASS`; canonical workflow `31932273685` |
| `PE7-AC6-DASHBOARD-MIGRATION-1` | `COMPLETE` | PR #503 exact head `8f20d7ba21a45f059335306507cd52dc58f46722`; merge `33d9b8b6ac6055d0c2d9236d8eafa4f8a952b4d6`; exact-head `PASS`; canonical workflow `31932913780` |
| `PE7-AC6-COMPATIBILITY-CLOSEOUT-1` | `COMPLETE` | PR #504 exact head `d66041149a32323d99b60afcfb21bd049735e667`; merge `c52fdcb18caa4787bfb10689cd32353c88461833`; exact-head `PASS`; canonical workflow `31933082911` |
| `PE7-AC7-REMOVAL-MANIFEST-1` | `COMPLETE` | PR #505 exact head `fed3a67597d47d5b48291cb11de3c45ed742afb1`; merge `83519bc316e2765f47fa1c0597b75b428be372bb`; exact-head `PASS`; canonical workflow `31933252024` |
| `PE7-AC7-CLEANUP-1` | `COMPLETE` | PR #506 exact head `e951f3ea9bbf52c22d9c1bbe749cbcbbfd16a71a`; merge `307b3070de5ba86b354072867835e75bd04fbe75`; exact-head `PASS`; canonical workflow `31933456371` |
| `PE7-AC7-CLOSEOUT-1` | `COMPLETE` | PR #507 exact head `8b47cc6f84a195ce883476f1b28ed5427b67f2a9`; merge `c3aa5f7c6b03944ca0b652af40402631877eacbb`; exact-head `PASS`; canonical workflow `31933628519` |
| `PE7-RWE-CR-RECONSTRUCTION-1` | `COMPLETE` | PR #508 exact head `1527fc5fef837c237ccf113c07f9e4ee898c9848`; merge `14c0d20b9e62c0e2fb6792400262c5b840a6d0aa`; exact-head `PASS`; canonical workflow `31933837329` |
| `PE7-RWE-CR-PROTOCOL-PREFLIGHT-1` | `COMPLETE` | PR #509 exact head `35e6d73e91a797255ee077481f19f63b0be82a35`; merge `cce8f91caaf1bc1a90259b717ef586820ff47293`; exact-head `PASS`; canonical workflow `31934003226` |
| `PE7-RWE-CR-RUN-1` | `COMPLETE` | PR #510 exact head `c5c68723025087559ff72ddd42a6d3e6760b6c57`; merge `fdb100a128428ea4d8826e338936f55057a1f989`; exact-head `PASS`; canonical workflow `31934598861` |
| `PE7-RWE-CR-ANALYSIS-1` | `COMPLETE` | PR #511 exact head `247a88c7bd795cd7a7de87e453fba297131b9e0f`; merge `6669081758b500480d3c378c085c1b0a15145602`; exact-head `PASS`; canonical workflow `31935251070` |
| `PE7-CWS-INGRESS-INVENTORY-1` | `COMPLETE` | PR #512 exact head `6b8abf898348f2680324dbbf4e023ef67280bf59`; merge `6f9228faea04be5f3999edebfb59a36f11db72d2`; exact-head `PASS`; canonical workflow `31935444988` |
| `PE7-CWS-PROJECTION-CONTRACT-1` | `COMPLETE` | PR #513 exact head `bccc9aaa032ab601735ba249b19c6a79c177159d`; merge `e9743573a6deb53b6381019a1dabc46e51aff333`; exact-head `PASS`; canonical workflow `31935626991` |
| `PE7-CWS-REHYDRATION-CONTRACT-1` | `COMPLETE` | PR #514 exact head `2a67606fb56e56ab71dc818a2830f6f485ffadc7`; merge `284b7f3eaecc10c9ece7e92b23ce6f83dde446e5`; exact-head `PASS`; canonical workflow `31935808699` |
| `PE7-CWS-PROJECTOR-CORE-1` | `COMPLETE` | PR #515 exact head `9762d47cc6295bfa070236f0df508b6fafe30f00`; merge `2ee2157f1a5eb0506319e86385b62f40ba16e3f4`; exact-head `PASS`; canonical workflow `31936004194` |
| `PE7-CWS-TOOL-RESULT-REDUCTION-1` | `COMPLETE` | PR #516 exact head `2904100f91cb9c454a228429f1d39d669dbbcce2`; merge `beb5584103fb12eba98754d7d40f9a74851bfa7c`; exact-head `PASS`; canonical workflow `31936178506` |
| `PE7-CWS-REPOSITORY-INTEGRATION-1` | `COMPLETE` | PR #517 exact head `f922bf2e3b8662b5ff7d32f51e5eadf63e7f2624`; merge `a8a729379aa5b85e963335954e13ae28e05f347c`; exact-head `PASS`; canonical workflow `31936366545` |
| `PE7-CWS-RUNTIME-INTEGRATION-1` | `COMPLETE` | PR #518 exact head `addc3a1b55f3c326956f12a437338204f9a360db`; merge `ff7b7084866f003ae95faa7ade6295a9d7202629`; exact-head `PASS`; canonical workflow `31936548122` |
| `PE7-CWS-CACHE-PARTITION-1` | `COMPLETE` | PR #519 exact head `99eadbfcb2e6416f6864c8a2edc2fbead3cc4801`; merge `8ff33f7c9ebce55b94bb9c0e813c12c7a8d40c26`; exact-head `PASS`; canonical workflow `31936724895` |
| `PE7-CWS-BENCHMARK-PROTOCOL-1` | `COMPLETE` | PR #520 exact head `fcf325f0bf4389e2c9ac177531758fc5917ba052`; merge `998a9124c1d2ea0d41e51137f36c466fe878021c`; exact-head `PASS`; canonical workflow `31936985369` |
| `PE7-CWS-BENCHMARK-PREFLIGHT-1` | `COMPLETE` | PR #521 exact head `8ceee8b411e7757cacea14c34f3a0792d60c1bb2`; merge `189ec4b4d7d82f254b05213aa500e00541edd9fd`; exact-head `PASS`; canonical workflow `31942335704` |
## Accepted Product and Control-Plane State

Accepted `main` contains:

- the Rust-owned workflow runtime, scheduler, ProductTask, and `LocalProductStore` authority boundaries;
- SQLite default storage with PostgreSQL parity and restart/recovery evidence;
- managed-coding runtime profiles and provider-free DeepSeek protocol/runtime wiring;
- delegated Golden Path authority with separate risk, spend, attempt, artifact approval, output confirmation, and terminal-evidence owners;
- exact-head CI, bounded review convergence, context-capsule transport, and the outbound local engineering loop;
- the repository-maintenance route contract with one existing queue/lease/controller boundary;
- an activated Plan lane behind real terminal-owner readiness checks, consuming the accepted bounded autonomous worker dispatch capsule and the provisioned Plan Execution Ledger Issue #383;
- controller-owned plan-packet lifecycle transitions (CI/review/merge/closeout receipts) recorded on the Plan Execution Ledger with idempotent readback and recovery;
- controller-owned exactly-one successor-promotion and bounded pause-escalation receipts, with no successor execution, EFFECT, or T3 authority;
- controller-derived full-SHA live PR review binding, append-only authenticated retrospective correction/readback, and deterministic plan-artifact allowed-path enforcement before any worktree mutation or GitHub write;
- a 16-input accepted controller dispatch surface, strict compact route-receipt payload validation, focused-check candidate mutation detection, and lifecycle consumer binding across packet, attempt, ledger, PR, head, CI, review, merge, and canonical closeout evidence;
- provider-free RWE/VDE contracts, production RWE v2 issue/admit/one-use spend, the first-live-baseline composition seam, store cell fence, and artifact validation;
- a transport whose authorized finite request timeout is no longer silently capped by the former 20-second body-read ceiling;
- an operator-gated, maximum-two-request compatibility calibration that requires parseable implementation content and is skipped by CI;
- Harness Evolution Level-1 as a default-off one-generation fixture laboratory with immutable active-Harness identity and no production-adoption authority.

No runtime path may merge, release, deploy, write target default branches, or adopt a candidate as the production Harness.

## Minimum First RWE Accepted State

The following prerequisites are accepted:

- Board A frozen v1 corpus/protocol/schedule, accepted distinct v2 refreeze, and strict production authorization contract;
- Board B store-owned production issue/admit/one-use spend wiring;
- PR #363 composition seam, role-separated delegated lifecycle, replay-safe store cell fence, unbypassable Provider provenance, allowed-path containment, restart/cleanup behavior, and honest fixture semantics;
- PR #368 timeout ownership and failure classification repair;
- PR #369 bounded compatibility-calibration mechanism.
- PR #370 v1-byte-preserving v2 corpus/protocol/schedule refreeze, semantic whitelist, freeze/hash locks, and bounded HTTP-test lock repair.

These capabilities do not authorize a live run by themselves.

## First Live v1 Attempts — Valid Failure Evidence, Not an Accepted Baseline

Two separately authorized one-use runs on 2026-08-07 (`auth-live-003`/`auth-live-004`, `run-live-20260807-c`/`-d`) executed all four v1 cells through the genuine delegated lifecycle:

- 8/8 planning nodes made real `deepseek-v4-pro` requests successfully;
- 8/8 implementation nodes failed after about 20.2 seconds because the then-current transport body-read timeout was shorter than `deepseek-v4-flash` reasoning generation;
- a direct probe also showed all 4,000 output tokens consumed by `reasoning_content`, leaving empty implementation content;
- no seal, Draft PR, target write, budget breach, outcome-unknown retry, or default-branch effect occurred;
- consumed cost and controlled failures remain valid evidence and must not be deleted or rewritten after v2.

These runs established the root cause and fail-closed behavior. They did not establish a viable baseline or architecture/economic improvement.

## Accepted Readiness Boundary

Accepted `main` contains the provider-free compatibility-calibration mechanism, the distinct versioned v2 refreeze, the repository-maintenance route, the accepted control-binding and workspace-preparation repairs, the bounded autonomous worker transport for OpenCode, the route-autopilot closeout, and the provider-free v2 and DB preflight receipts. A separately authorized DB RUN exists as controlled-failure evidence, but its route T3/owner-outcome closeout is not accepted; it is not a viable baseline. Neither the route controller nor a compiled successor may issue or consume external-effect authority, write a target, run an external schedule, or authorize downstream measurement/Architecture Convergence work outside each independently accepted packet.

Candidate evidence remains non-authoritative until it is bound to one exact PR head, passes the repository review protocol and canonical CI, and is merged. Do not record candidate branches, PR numbers, CI runs, or review claims here; the capsule observes them at handoff time and fails closed when unavailable or conflicting.

## Capability Status

| Capability | State | Entry or exit condition |
|---|---|---|
| RWE Board A freeze, Board B authority, live composition seam | `COMPLETE` | Accepted on main |
| Timeout ownership repair | `COMPLETE` | PR #368 accepted |
| Compatibility calibration mechanism | `COMPLETE` | PR #369 accepted; mechanism only |
| V2 refreeze + bounded test-race repair | `COMPLETE` | PR #370 accepted; exact v2 freeze and canonical lock repair are on main |
| Repository-maintenance route contract | `COMPLETE` | PR #380 accepted; its queue/lease/controller boundary is consumed by the accepted Plan lane |
| Plan-lane activation | `COMPLETE` | PR #382 accepted; Plan lane active behind terminal-owner readiness; ledger Issue #383 provisioned |
| Plan-packet lifecycle controller | `COMPLETE` | PR #385 accepted; CI/review/merge/closeout receipts controller-owned on the ledger with idempotent readback |
| Plan-lane successor promotion and escalation | `COMPLETE` | PR #387 accepted; exactly-one successor-promotion receipts and bounded pause escalation are controller-owned on the existing ledger |
| Route automation | `COMPLETE` | PR #416/#418/#422 accepted the control-plane path; PR #426 completed one OpenCode-backed code-and-document packet; PR #429 persisted trusted CI/review/merge/closeout on ledger #383 and closed the soak |
| `PE7-ROUTE-AUTONOMY-STABILIZATION-1` | `COMPLETE` | PR #416 accepted the stabilization implementation and PR #418 completed its one-time bootstrap reconciliation; overall Route automation remains incomplete until the soak, real packet lifecycle, and successor advancement are proved |
| Control binding integrity | `COMPLETE` | PR #408 accepted strict live exact-head review binding, authenticated append-only #405 correction/readback, and deterministic pre-effect plan-artifact scope enforcement |
| `PE7-WORKSPACE-PREP-RECEIPT-RACE-REPAIR-1` | `COMPLETE` | PR #413 base `59cec5745ddd7f89ce8c099a5de2c7e3c3ec3a1e`, exact head `fc8c005981d2fa12f0f494a131b839d65a46a8ba`, exact-head review receipt comment `5268787985`, canonical workflow `31611860646`, and squash merge `9cc118fa72d9d13a24cdf968cc5fc20dbe80b28f` accepted transactionally consistent SQLite receipt/status observation with deterministic concurrent-winner reuse and genuine missing-receipt rejection |
| Route-autopilot adversarial soak | `COMPLETE` | PR #426 worker plus PR #429 closeout: one real OpenCode-backed packet through existing PR/CI/review/merge/closeout owners, with trusted ledger receipts after GitHub verify |
| V2 provider-free viability preflight | `COMPLETE` | PR #437: operator_preflight ready=true without issue/admit/spend; unissued request sha256 `015c94e9d65a902f3aba5eae4f3da6cba6d534cc3c57af3a6faf89125663469a` |
| V2 four-cell run | `COMPLETE` | PR #441: 4/4 `controlled_failure`; no seal; no target-default-branch write |
| V2 four-cell closeout | `COMPLETE` | Disposition `CONTROLLED_FAILURE`; restricted-bundle sha256 `9b345faf744c14d67157856a512b39d90c6e03ff1081783c793b987d6f93bf82`; redacted-bundle sha256 `e2eafa226700061cb000b35dec776ef0b49417aa5faece0b065923b49ee83d3f` |
| Measurement-readiness estimands | `COMPLETE` | PR #444 accepted the source-bound task-level value, gate, margin, repetition, and paired-bootstrap contract |
| `PE7-RWE-MR-OPERATIONS-EVIDENCE-1` | `COMPLETE` | PR #446 exact head `34c68d94c1737769c60fb7ea1722b464a5d764aa`; exact-head review receipt comment `5289427966`; canonical workflow `31769511015`; exact-head check `31769511065`; squash merge `e34d1ae3c3ecf5e6c919c71a3d26d6690a66444`; provider-free field-owner and explicit-unavailable manifest accepted |
| `PE7-RWE-MR-PROTOCOL-FREEZE-1` | `COMPLETE` | PR #447 exact head `00c8592676c5f73447f94b3abc1361087b371196`; merge `f575b10a6de617bf3dab5611900bf0a48727c0c6`; manifest sha256 `b5e37c7c2419a3acb42a8f21dbf2ba56aa8ddabb995b84b644f1b116a3321c12` |
| `PE7-RWE-DB-SNAPSHOT-CORPUS-1` | `COMPLETE` | PR #448 recorded the original unavailable snapshot boundary; PR #451 accepted the reconstructable replacement without Provider or target effects |
| `PE7-RWE-DB-SNAPSHOT-RECONSTRUCT-1` | `COMPLETE` | PR #451 accepted a hash-bound provider-free reconstruction manifest; preflight remains provider-free and unissued |
| Provider-free RWE DB preflight | `COMPLETE` | Real same-tenant Store-owned Golden Path prerequisite completed; `rwe-live-baseline preflight` returned `rwe_operator_preflight.v1` `ready=true` with zero blockers, no authority consumption, Provider call, or target write; projection sha256 `b8c35d4060d98598ce3e3bc3977a84d125b1df09ff66b2b9f6d9aa4303c03954` |
| Measurement readiness | `BLOCKED_PREREQUISITE` | Estimands, finite corpus/sampling, operations/evidence, protocol freeze, and reconstructable snapshot are accepted; the observed DB RUN is retained as a non-baseline controlled failure |
| Decision-grade pre-AC baseline | `DEFERRED` | The failed DB RUN and its analysis are parked and are not an AC prerequisite; no decision-grade baseline is claimed |
| AC0 runtime inventory and AC1 ProcessSupervisor | `DEFERRED` | Optional hardening; existing runtime-specific owners retain timeout, cancellation, kill/reap, failure, and outcome-unknown boundaries |
| AC0 data/trace freeze | `COMPLETE` | The bounded provider-free owner/caller/transaction/projection/legacy inventory and trace/order closeout are recorded below; no ownership move was made |
| AC2 typed execution contract | `COMPLETE` | Provider-free typed state/outcome/usage contract, executor mappings, and the accepted fail-closed boundary repair are recorded below; PRs #469, #472, and #475 are merge-backed. The contract authorizes no wire/schema change, authority move, or shared `ProcessSupervisor` |
| AC2 typed boundary and caller migration | `COMPLETE` | Boundary repair and the enumerated ProductTask verification/managed-review caller migration are accepted by PRs #475 and #478; callers use the canonical typed mapping, unknown evidence remains fail-closed, and AC1 shared supervision remains deferred optional hardening |
| AC3 Golden Path responsibility contract | `IN_PROGRESS` | Provider-free Golden Path responsibility matrix, state transitions, audit identities, pure inputs/outputs, effect ports, store commands, and migration sequence are frozen below; no runtime, wire, or schema change is authorized |
| AC4–AC5 | `BLOCKED_PREREQUISITE` | Each stage remains behind its immediately preceding accepted AC contract and closeout |
| AC6 schema convergence | `BLOCKED_PREREQUISITE` | Contract, Rust/codegen, SDK, Dashboard data migration, compatibility closeout |
| AC7 cleanup | `BLOCKED_PREREQUISITE` | Removal manifest, deletion-only implementation, independent closeout |
| Contemporary old/new replay | `BLOCKED_PREREQUISITE` | Reconstruction, protocol/preflight, authorized run, analysis |
| EC1–EC5 experiment control | `BLOCKED_PREREQUISITE` | 17 packets; causal mutation evidence and each control family freeze before implementation |
| Level-1 core without memory/skill | `BLOCKED_PREREQUISITE` | Preflight, one authorized generation, independent closeout |
| Level-1 transfer pilot | `BLOCKED_PREREQUISITE` | Sealed protocol, authorized run, analysis |
| Optional memory/skill factor experiment | `BLOCKED_PREREQUISITE` | 5-packet side branch; not a Level-2 prerequisite |
| Level-2 GO/NO-GO | `BLOCKED_PREREQUISITE` | Rule audit, independent evidence analysis, explicit human receipt |
| Bounded Level-2 controller | `BLOCKED_PREREQUISITE` | 8 packets from frozen contract through simulation, one pilot, and closeout |
| Final sealed transfer | `BLOCKED_PREREQUISITE` | Protocol, authorized run, analysis |
| Human adoption branch | `BLOCKED_PREREQUISITE` | Readiness dossier then separate human decision |
| Meta Improver branch | `BLOCKED_PREREQUISITE` | 11 packets from claim GO/NO-GO through O0/O1 comparison, replication, and claim decision |
| Optional advanced R4–R6 research | `BLOCKED_PREREQUISITE` | Supported Meta result plus separate human GO; bounded metacognitive, weight-adapter, then one outer-policy family |
| Dashboard #225 / successor | `DEFERRED` | Disposition, presentation refresh, closeout; always last |

## AC0 bounded data and trace inventory

This is the provider-free inventory for the current AC0 packet. It records existing owners and representative call paths; it does not create a runtime, scheduler, store, evaluator, projection, credential, or rollback owner.

| Slice | Existing owner and representative path | Data/trace boundary | Compatibility and rollback obligation |
|---|---|---|---|
| Intake and Golden Path | `engine/src/product_golden_path.rs` (`validate_intake`, `ValidatedProductTaskIntake`, `ProductTaskStatus`); HTTP `api_create_product_task`; Dashboard `MissionControl.submitProductGoldenPath` → `dashboard/src/lib/api-client.ts:createProductTask` | Intake contract/hash, task version, run/node identity, and redacted public/task projections remain bound to the existing ProductTask | Reject before admission on invalid gates; retain the existing task version and never replay an unproved effect |
| Runtime orchestration | Existing `engine/src/scheduler.rs`, `engine/src/workflow/`, `engine/src/executor_pool.rs`, and `engine/src/node_executor.rs`; HTTP callers `api_compile_and_schedule_product_task`, `api_finalize_product_task`, `api_recover_product_task_workspace` | Scheduler leases, node attempts, executor result, verification, and terminal classification stay under the existing runtime/store path | Pause, cancellation, cleanup, failure, and outcome-unknown behavior remains executor/store-owned; no shared ProcessSupervisor is introduced |
| Persistence and transactions | `LocalProductStore` in `engine/src/storage/local_product_store/`: `admit_product_task` → `reserve_product_task`, `transition_product_task`, `compile_and_schedule_product_task`, `finalize_product_task_after_execution`, `approve_product_task_for_tenant`/`approve_product_task`, `output_product_task_for_tenant`/`output_product_task`, `approve_and_output_product_task_for_tenant`, `recover_product_task_workspace_for_tenant`, and `fail_product_task_and_compensate` | ProductTask/run/node/attempt, delegation, spend, output, audit, terminal, and recovery rows are committed through the existing transaction boundary | Preserve CAS/idempotency/restart semantics; the same owner has SQLite/PostgreSQL branches, but only the backend anchors listed below are claimed as parity evidence; no direct SQL mutation or second store |
| Provider and credential boundary | Existing managed-provider adapters under `engine/src/provider/` and ProductTask managed-acceptance/store owners; repository maintenance uses the existing OpenCode controller seam | Store-owned request journal claim/reservation occurs before send; usage reconciliation and redacted evidence follow the send; credentials remain parent-held and never enter persistence or evidence | This packet makes no Provider call and consumes no authority; a crash-left send/outcome-unknown record remains non-retryable under existing owners |
| Wire, codegen, SDK, Dashboard | Dispatch wire contracts `wire_contract/v1/` → `codegen/generate_wire_types.py` → generated Rust/Python/TypeScript outputs; ProductTask API handlers use Rust `public_product_task_projection`/`public_product_task_result_projection`; TypeScript `ProductTaskIntakeRequest`/`ProductTaskResponse`, SDK `createProductTask`/`compileAndScheduleProductTask`/`finalizeProductTask`/`approveProductTask`/`outputProductTask`, Python `sdk/python/src/agent_control_plane_sdk/client.py`, and Dashboard mirror these client calls | Generated types and ProductTask API/SDK/Dashboard data are projections of Rust/API contracts, not authorities | No wire/schema migration in AC0; `scripts/check_wire_codegen_drift.sh` remains the dispatch drift gate; ProductTask projection compatibility is a separate API/consumer boundary, and AC6 owns any later schema migration |
| Configuration and composition | `engine/src/main.rs`, `engine/src/http_server/state.rs` (`AxumApiState`), and existing CLI/config owners | Composition-time gates and symbolic provider/config identity are derived by existing roots; no config is copied into worker evidence | Missing or conflicting gates fail closed; no new composition root or config owner |
| Legacy/advisory callers | Existing dispatch/planning and local-runner paths (`engine/src/dispatch_engine.rs`, `engine/src/local_runner_provider.rs`, and the Dashboard plan path) remain under their current owners | Legacy dispatch records/traces are not rebound to ProductTask data and are not treated as managed acceptance | Caller migration and deletion are deferred to their accepted AC contract; no deletion or semantic rebind in AC0 |

### Provider-free golden traces and parity anchors

The minimum trace set is already exercised by existing tests and is the accepted read-only evidence for the AC0 trace/order freeze and next AC2 contract:

| Trace anchor | Ordered provider-free path and terminal outcomes | Exact evidence and backend boundary |
|---|---|---|
| Intake/admission/status | `api_create_product_task` → `validate_intake` → `admit_product_task`/`reserve_product_task`; `admitted` → `workspace_preparing` → `workspace_bound`, or `failed`/`killed`/`blocked` | `engine/tests/test_product_golden_path_g1.rs`, `test_product_golden_path_authority.rs`; primarily SQLite, with no full cross-backend claim |
| Compile/execute/verify | `api_compile_and_schedule_product_task` → `compile_and_schedule_product_task` → `graph_ready` → `running` → `verifying`; the successful path then advances `awaiting_approval` → `output_pending` → `completed`; each execution stage may instead end `failed`/`killed`/`paused`/`budget_exhausted`/`blocked`, while `outcome_unknown` enters reconciliation | `engine/tests/test_product_golden_path_g2.rs` (`compile_creates_executable_run_bound_to_task`, `scheduler_tick_executes_command_in_bound_worktree`, `finalize_does_not_execute_nodes_before_scheduler`), `test_product_golden_path_g3.rs` (`end_to_end_artifact_only_path_with_real_verification`, `verification_failure_blocks_capture_approval_and_output`); PostgreSQL anchor `postgres_managed_executor_receives_exact_long_objective_without_public_persistence` is explicit and provider-free, while remaining steps are not claimed as per-backend equivalent here |
| Recovery and compensation | `recover_product_task_workspace_for_tenant` and `fail_product_task_and_compensate` re-read the existing receipt/version and retain failure or reconciliation state; `outcome_unknown` never becomes a speculative retry | `engine/tests/test_product_golden_path_recovery.rs`; SQLite anchors include admission/artifact audit rollback and restart usage; PostgreSQL anchors include `postgres_product_admission_audit_failure_rolls_back_private_objective`, `postgres_workspace_transition_audit_failure_keeps_task_admitted`, and `postgres_restart_accumulates_failed_managed_attempt_usage` under `ACP_TEST_DATABASE_URL` |
| Delegation, approval, output, projections | `prepare_delegated_managed_product_task` → activation/terminal evidence → `api_approve_product_task`/`api_output_product_task` or `api_approve_and_output_product_task`; public projection remains redacted | `engine/tests/test_product_golden_path_evidence.rs` (`drive_to_awaiting_approval`, `approval_and_output_are_separate_and_missing_confirmation_has_zero_side_effects`, `terminal_evidence_links_task_owners_without_fabricated_cost`, `duplicate_concurrent_output_calls_reuse_one_canonical_terminal_evidence`), `test_managed_acceptance_delegation.rs` (`bootstrap_only_delegates_minimal_managed_identities_and_reissues_after_restart`), and `test_product_golden_path_evidence.rs`/`test_local_product_store.rs` projection tests; backend-specific projection parity remains explicitly unproved and is not an AC0 or AC2 acceptance claim |

The inventory is intentionally not a claim of a successful live DeepSeek run or a decision-grade RWE baseline. SQLite is the default provider-free evidence backend; PostgreSQL evidence is limited to the named `ACP_TEST_DATABASE_URL` anchors, and API/SDK/Dashboard projection parity across both backends remains explicitly unmapped. That gap is retained as a fail-closed boundary: any later packet that requires cross-backend projection parity must stop and add evidence before changing the contract. Reverting this document-only inventory and the packet-status note is the rollback; existing failure evidence is retained and no parked effect is replayed.

## AC0 provider-free trace/order closeout

The accepted inventory stabilizes the minimum route without adding a runtime owner:

1. Intake/admission/status remains `api_create_product_task` → `validate_intake` → `LocalProductStore` admission/reservation, with invalid gates rejected before admission.
2. Compile/execute/verify remains `api_compile_and_schedule_product_task` → existing scheduler/executor/store owners → verification and terminal classification; `outcome_unknown` enters reconciliation and is never a speculative retry.
3. Recovery/compensation remains `recover_product_task_workspace_for_tenant` and `fail_product_task_and_compensate`, with existing receipt/version, CAS, idempotency, audit, restart, and rollback owners.
4. Delegation/approval/output remains store-owned activation and terminal evidence followed by separate approval/output confirmation; public projections remain redacted projections, not authorities.

The dependency order is frozen as `AC0 inventory → AC0 trace/order closeout → AC2 contract → AC2 typed boundary → AC2 caller migration → AC3 → AC4 → AC5 → AC6 → AC7`. AC1 shared `ProcessSupervisor` remains deferred optional hardening and is not a prerequisite. The accepted AC2 contract starts from `engine/src/node_executor.rs::ProcessOutcome`, `engine/src/executor_adapter.rs::ExecutionResult`, `engine/src/provider/executor.rs`, `engine/src/provider/managed_deepseek_executor.rs`, `engine/src/execution_usage/reconcile.rs`, and the existing persistence owners `engine/src/storage/local_product_store/product_tasks.rs` and `engine/src/storage/local_product_store/workflow_runs.rs`; the boundary-core packet may add only typed mappings and focused tests. It must not move admission, lease, spend, verification, approval, output, audit, recovery, or target authority. Within the enumerated Golden Path and legacy/advisory caller closure, no unknown production caller was found; any caller outside that closure or any cross-backend parity requirement stops the next packet rather than widening scope.

## AC2 typed execution contract (provider-free contract freeze)

This is the closed contract for the next additive AC2 boundary work. It names the existing owners and their exact variants; it does not add a Rust type, wire field, schema migration, public projection, scheduler, store, or second authority. The ProductTask lifecycle remains authoritative in `LocalProductStore`; executor and usage records are evidence inputs to that owner, not competing state machines.

### Closed state, outcome, and evidence vocabulary

| Contract axis | Closed variants and existing representation | Authority and fail-closed rule |
|---|---|---|
| Admission | `rejected_before_admission` (intake validation fails before `ProductTaskStatus::Admitted`); `admitted` (`Admitted` with the existing reservation) | `validate_intake` and `admit_product_task`/`reserve_product_task` remain the only admission path. Rejection has no execution effect and is not a retryable execution outcome. |
| Preparation | `preparing` (`WorkspacePreparing`); `prepared` (`WorkspaceBound`) | `LocalProductStore` owns the receipt, target/workspace binding, CAS, and restart-safe compensation. A missing or mismatched receipt blocks advancement. |
| Effect boundary | `effect_not_started` (`GraphReady`, whose run exists but has no leased node); `effect_started` (`Running`, `Verifying`, `RepairPending`, `AwaitingApproval`, or `OutputPending`) | Scheduler leases, node attempts, verification, approval, and output remain under their current owners. A pre-send refusal is not evidence that an effect started; a request that may have been sent cannot be replayed speculatively. |
| Pause and cancellation | `paused` (`Paused`, resumable under the existing transition table); `cancelled` (`Killed`, terminal) | Cancellation/kill stays executor, scheduler, and store-owned. No caller may turn a pause or kill into success or an automatic retry. |
| Known outcome | `known_success` is `ProcessOutcome::exited(0)` plus the existing verification/terminal owner for OS-process executors, or an explicit completed provider/managed response with accepted executor evidence; `known_failure` is a definitive `exited(non_zero)`, `signaled`, `output_read_failed`, `spawn_failed`, `timed_out`, `timeout`, `wait_failed`, `stdout_reader_failed`, `stderr_reader_failed`, `combined_reader_failed`, `process_tree_cleanup_failed`, `output_limit_exceeded`, `process_tree_containment_unavailable`, `process_tree_containment_unsupported`, `invalid_output_limits`, or pre-send/definitive adapter refusal | `ProcessOutcome` describes the executor/process observation and is optional for provider/managed responses. `spawn_failed`, `process_tree_containment_unavailable`, `process_tree_containment_unsupported`, and `invalid_output_limits` are effect-not-started; the other listed process failures are effect-started unless the executor's pre-child evidence proves otherwise. A provider response is not forced through an OS-process variant. `signaled` with an unavailable signal remains a known termination with incomplete detail; `unavailable` is incomplete process evidence, never success. `ProductTaskStatus::Completed` is the only canonical product success. |
| Terminal failure | `terminal_failure` is the store-owned `ProductTaskStatus::{Failed,BudgetExhausted,Blocked}` set; `Killed` is represented only by the separate `cancelled` variant | Terminal failure requires the existing run/node evidence, CAS, audit, and compensation path. It is non-success and never an automatic retry authorization. |
| Unknown outcome | `unknown` is `ProductTaskStatus::OutcomeUnknown` or an executor/provider result explicitly classified as outcome-unknown after a possible send | Unknown is not success, not complete evidence, and not retry authorization. Existing reconciliation/compensation must determine the next terminal or output state; automatic replay is forbidden. |
| Usage completeness | `complete`, `partial`, `ambiguous`, `conflicting` are exactly `EventCompleteness::{Complete,Partial,Ambiguous,Conflicting}` | `Complete` plus conflict-free reconciliation may support `known_success` when the execution and verification owners also prove success. `Partial` or `Ambiguous` is incomplete evidence: a definitive execution failure remains `known_failure`, but an otherwise-successful or effect-possible execution remains `unknown` until reconciliation proves the missing usage/cost fields; neither authorizes retry. `ReconcileResult.conflicts` and `EventCompleteness::Conflicting` make `admission_evidence_ok` fail closed. Missing cost is represented by `CostSource::Unavailable`, not fabricated zero cost. |

`ProductTaskStatus` is the canonical lifecycle vocabulary: `Admitted`, `WorkspacePreparing`, `WorkspaceBound`, `GraphReady`, `Running`, `Verifying`, `RepairPending`, `AwaitingApproval`, `OutputPending`, `Completed`, `Failed`, `Killed`, `Paused`, `BudgetExhausted`, `Blocked`, and `OutcomeUnknown`. `Completed`, `Failed`, `Killed`, `BudgetExhausted`, `Blocked`, and `OutcomeUnknown` are terminal; only `GraphReady`, `Running`, `Verifying`, and `OutputPending` admit execution. The existing transition table remains the sole legal transition relation.

### Executor-specific mapping

| Existing executor/evidence owner | Mapping into the closed contract | Boundary that must remain unchanged |
|---|---|---|
| `CommandNodeExecutor` and other local process executors | `ExitStatus` becomes `ProcessOutcome::exited` or `signaled`; the existing failure states are `output_read_failed`, `spawn_failed`, `timed_out`, `timeout`, `wait_failed`, `stdout_reader_failed`, `stderr_reader_failed`, `combined_reader_failed`, `process_tree_cleanup_failed`, `output_limit_exceeded`, `process_tree_containment_unavailable`, `process_tree_containment_unsupported`, and `invalid_output_limits`. `ProcessOutcome::successful_exit()` is the narrow process-success predicate. | `spawn_failed`, `process_tree_containment_unavailable`, `process_tree_containment_unsupported`, and `invalid_output_limits` are effect-not-started known failures. The other listed process failures are effect-started known failures unless trusted pre-child evidence proves otherwise. `ProcessOutcome::unavailable` means known pre-spawn failure only when the owner records that boundary; when it only means that the executor has no OS-process owner or the evidence is absent, it is incomplete evidence and any possible effect remains `unknown`. Process cleanup, timeout, kill/reap, and output redaction remain executor-owned. A process outcome is not by itself ProductTask terminalization. |
| `codex_cli` and `claude_code_cli` node paths | `NodeExecutionOutput` carries `status`, bounded/redacted output, optional usage, and `process_outcome`; absent process ownership is represented by `ProcessOutcome::unavailable(...)` | `NodeExecutionOutput::to_value` is a projection/evidence adapter. It cannot mint approval, output, spend, or target authority. |
| Provider and managed-provider paths, including `provider/executor.rs` and `provider/managed_deepseek_executor.rs` | Preserve the existing pre-send/definitive/unknown classification. A definitive pre-send refusal maps to known failure with effect-not-started; a possible-send unknown maps to `OutcomeUnknown`; a verified response maps to known executor evidence and then existing verification/store terminalization | Credentials remain parent-held. Provider request journaling, usage reconciliation, redaction, compensation, and no-retry-after-unknown stay with the existing provider/store owners. This packet makes no Provider call. |
| Adaptive provider adapter | Endpoint/config construction errors (`sensitive_pattern_detected`, `invalid_endpoint_config_json`, `endpoint_config_empty`, `endpoint_limit_exceeded`, `duplicate_endpoint_id`, `invalid_endpoint_identity`, `invalid_timeout_ms`, `invalid_pricing`, `invalid_provider_type`, `invalid_credential_env`, `invalid_base_url`, `invalid_credential_backend`, `credential_env_unavailable`, and `adaptive_registry_invalid`) and pre-call plan/limit/gate errors (`adaptive_global_cost_gate_blocked`, `adaptive_plan_missing`, `adaptive_plan_invalid`, `adaptive_policy_plan_invalid`, `adaptive_policy_not_promoted`, `adaptive_policy_plan_missing`, `adaptive_policy_decision_invalid`, `adaptive_request_invalid`, `adaptive_call_limit_invalid`, `adaptive_cost_limit_invalid`, `adaptive_timeout_limit_invalid`, `adaptive_token_limit_invalid`, `adaptive_concurrency_limit_invalid`, `adaptive_panel_quorum_invalid`, `adaptive_concurrency_not_supported`, `adaptive_endpoint_invalid`, `adaptive_endpoint_not_found`, `adaptive_endpoint_model_mismatch`, `adaptive_endpoint_model_unbound`, and `adaptive_execution_disabled`) are `effect_not_started` + `known_failure`. `adaptive_provider_disabled` is the same classification because it records a disabled call admission without sending a provider request. `adaptive_execution_killed` is `cancelled`, with effect-started determined by call evidence. `adaptive_provider_token_invalid`, `adaptive_provider_identity_mismatch`, `adaptive_provider_token_over_reservation`, and `adaptive_provider_cost_over_reservation` are effect-started `known_failure` because `complete_success` received a response and rejected its usage/identity/cost. `adaptive_execution_timeout` and `adaptive_provider_error` are effect-started `unknown` when a call was admitted but no definitive response/refusal exists; they are known failure only when an explicit pre-send refusal is recorded. `adaptive_runtime_failure` is always `unknown` unless a trusted explicit pre-send refusal is recorded; missing or empty call evidence never proves that no request was sent. `adaptive_call_limit_exceeded`, `adaptive_cost_limit_exceeded`, `adaptive_token_limit_exceeded`, `adaptive_panel_quorum_not_met`, and `adaptive_fallback_exhausted` are `known_failure` only when every attempted call is definitive; otherwise they remain `unknown`. `AdaptiveExecutionResult` is `known_success` only after its explicit result and existing verification owner accept it. | Adaptive execution does not become a second scheduler, budget ledger, usage owner, or ProductTask authority. These mappings are classification only; unknown calls never become retry authorization. |
| Legacy dispatch adapter | `executor_adapter.rs::ExecutionResult` keeps its existing `execution_result.v1` fields and legacy statuses (`not_executed`, `preview_generated`, `mock_completed`, `manual_pending`, `manual_completed`, `failed`, `cli_completed`, `provider_completed`) | These are legacy/advisory dispatch projections. They are not rebound to ProductTask lifecycle state and cannot authorize managed acceptance, approval, output, retry, or target effects. |

### Usage and recovery binding

`ExecutionUsageEventV1` is the normalized usage evidence shape. Its executor/source identity, disjoint token buckets, `CostSource::{Unavailable,Estimated,ProviderOrExecutorReported}`, stable dedupe identity, and `EventCompleteness` are preserved. `ReconcileResult` returns canonical events, suppressed duplicates, and explicit conflicts; only conflict-free evidence may pass `admission_evidence_ok`. Usage uncertainty never becomes a zero-cost success or a retry permission.

When an effect may have started, `ProductTaskStatus::OutcomeUnknown` is retained until the existing reconciliation owner proves a safe disposition. `fail_product_task_and_compensate` revalidates the workspace receipt and target boundary, performs the existing cleanup/reconciliation, and transitions through the store's CAS/audit path. No typed boundary may bypass that sequence, delete unknown evidence, or create an alternate recovery owner. Public API, SDK, Dashboard, wire/codegen, and schema compatibility remain unchanged until the later accepted AC2 caller/schema packets.

The focused current evidence is `engine/tests/test_product_golden_path_g1.rs`, `test_product_golden_path_g2.rs`, `test_product_golden_path_g3.rs`, `test_product_golden_path_recovery.rs`, `test_product_golden_path_evidence.rs`, the provider failure/reconciliation tests, and the existing `execution_usage` unit tests. PR #472 added the internal `ProcessBoundaryMapping` adapter; PR #475 closed its contradictory-success and pre-spawn classification gaps with exhaustive G2 coverage and unchanged `process_outcome.v1` serialization. PR #478 consumed this mapping in the enumerated verification/managed-review callers only; the accepted migration authorizes no new effect, retry, terminal, or authority decision.

## AC2 caller migration closeout

`PE7-AC2-CALLER-MIGRATION-1` migrated the two accepted callers—`engine/src/storage/local_product_store/product_tasks.rs` and `engine/src/storage/local_product_store/managed_acceptance.rs`—to `ProcessBoundaryMapping::is_known_success()`. Raw serialized `process_outcome.v1` evidence remains preserved; malformed, unknown, ambiguous, and not-started outcomes remain non-success, and the existing pre-spawn refusal classification remains a verification failure. Focused evidence passed with `cargo test -p engine product_verification_failure_tests` and `cargo test -p engine --test test_product_golden_path_g2`; the full `cargo test -p engine`, clippy, wire-drift, security-baseline, handoff, formatting, and diff checks also passed locally. The exact stable-head review receipt is comment `5302349955` for head `4c748ce5f7988da9f61dd1e4650351b5d6c8bf72`; canonical workflow `31886172712` completed all required jobs; PR #478 was squash-merged as `36d7b33a5483cff63715b7981794aff1de614ae2`, and refreshed `main` is that merge with a clean checkout. No wire/schema, provider, target, runtime, store, or authority owner changed.

## AC3 Golden Path responsibility contract (provider-free contract freeze)

This is the closed file-level extraction contract for the next additive AC3 boundary work. It freezes the Golden Path responsibility matrix, state transitions, audit identities, pure inputs/outputs, effect ports, store commands, and migration sequence; it does not add a runtime, scheduler, store, budget, approval, output, audit, rollback, or authority owner, and it makes no wire/schema or ProductTask runtime change.

### Responsibility matrix

| Contract axis | Frozen binding |
|---|---|
| Orchestration | `engine/src/product_golden_path.rs` (`validate_intake`, `compile_product_executable_graph`, `intake_contract_sha256`, `redacted_intake_json`) is the pure orchestration/projection side; it remains distinct from the sole `LocalProductStore` mutation authority. |
| Store mutation | `engine/src/storage/local_product_store/product_tasks.rs` remains the sole ProductTask mutation owner for the commands below. |
| External effects | Scheduler leases, node attempts, executor/provider calls, verification, artifact, approval, output, and target-output owners remain separate effect owners; they must not mutate ProductTask lifecycle or mint authority. |

### State transitions and golden traces

The `Provider-free golden traces and parity anchors` table above is the accepted trace set: intake/admission/status, compile/execute/verify, recovery/compensation, and delegation/approval/output. `ProductTaskStatus` remains the canonical lifecycle vocabulary and the existing transition table remains the sole legal transition relation. No new state or transition is introduced; later AC3 code packets must prove golden-trace equivalence against these anchors before changing code.

### Audit identities

Every stage binds to the exact current identities named in `docs/ARCHITECTURE_BOOK.md`: tenant/workspace, ProductTask version, plan/run/node attempt, lease/owner token, executable/provider/model, budget, source/tree, artifact, approval, output receipt, and audit. A ProductTask status/version transition and its transition-audit record commit atomically in each supported storage backend.

### Pure inputs and outputs

Existing `ProductTaskIntakeRequest`/`ValidatedProductTaskIntake`, the executable-graph projection, the verification result, and redacted task projections are the pure inputs/outputs. No new durable field or public projection is added by this contract.

### Effect ports and store commands

The existing scheduler/executor/provider/output ports remain effect ports. The store commands `admit_product_task`, `reserve_product_task`, `transition_product_task`, `compile_and_schedule_product_task`, `finalize_product_task_after_execution`, `approve_product_task_for_tenant`, `approve_product_task`, `output_product_task_for_tenant`, `output_product_task`, `approve_and_output_product_task_for_tenant`, `recover_product_task_workspace_for_tenant`, and `fail_product_task_and_compensate` remain the sole store mutation surface. This contract invokes none and adds no mutation path.

### Migration sequence

`PE7-AC3-CONTRACT-1` → `PE7-AC3-ORCHESTRATOR-CORE-1` → `PE7-AC3-PORT-MIGRATION-1`. Later packets must preserve the bindings above and prove golden-trace equivalence before changing code.

### Forbidden ownership imports

`engine/src/product_golden_path.rs` must not import or call the store mutation commands above, provider adapters under `engine/src/provider/`, HTTP approval/output handlers, or target-output owners; handlers and callers remain adapters, and `engine/src/storage/local_product_store/product_tasks.rs` remains the sole ProductTask mutation owner. Effect owners must not mutate ProductTask lifecycle or mint authority. Any required ownership move is `DECISION_REQUIRED`.

The read-only evidence bindings for this packet are `engine/src/product_golden_path.rs`, `engine/src/http_server/handlers/product_tasks.rs`, `engine/src/storage/local_product_store/managed_acceptance.rs`, `engine/src/storage/local_product_store/product_tasks.rs`, `engine/tests/test_product_golden_path_g3.rs`, and `engine/tests/test_product_golden_path_recovery.rs`; they were re-proved as owner, caller, and recovery anchors without edits. The six route-control/test paths and five canonical documents are the only allowed edits; `allowed_paths` is the closed edit scope and `read_paths` is its safe superset.

## Primary-route scope decision

The repository's primary product is the exact-head, fail-closed coding-agent control plane: Rust-owned runtime/scheduler/ProductTask execution, LocalProductStore authority, bounded provider/CLI adapters, auditable patch/PR output, and truthful recovery evidence. The repository-wide subprocess inventory and a new shared `ProcessSupervisor` are not required to run that product and are parked as optional hardening. The smaller AC0 data/trace freeze remains because it protects existing owner and recovery boundaries without changing production behavior. Existing OpenCode, LangGraph, managed CLI, command, and target-Git owners remain authoritative; this decision does not weaken their existing timeout, cancellation, cleanup, failure, or outcome-unknown behavior.

## Recursive-Improvement Classification

The repository currently demonstrates neither:

- automatic multi-generation Harness evolution;
- improvement of the improvement operator itself;
- self-referential metacognitive-operator improvement;
- Harness and model-weight/adapter co-evolution;
- outer parent/lever/curriculum-policy evolution;
- stable cross-task or cross-model transfer;
- expanding problem-space exploration;
- continuous learning;
- production self-update.

```text
Harness improvement
!= improvement-operator improvement
!= production adoption
!= general recursive self-improvement
```

A NO-GO, saturation result, diversity collapse, transfer failure, or inability to beat the frozen baseline under equal total lifecycle budget is valid completion and must be preserved.

## Confirmed Integration Gaps

1. No accepted four-cell v2 RWE baseline exists. A provider-free preflight ran ready=true with zero blockers (unissued request package digest `015c94e9d65a902f3aba5eae4f3da6cba6d534cc3c57af3a6faf89125663469a`). The later operator EFFECT `run-live-20260813-v2c` executed the four frozen v2 cells through the genuine delegated lifecycle and terminated `controlled_failure`: `cell-rwe-minimum-t1-fix_flow_linkage-r1-bp-standard-s2026080601`, `cell-rwe-minimum-t1-fix_flow_linkage-r2-bp-standard-s2026080602`, `cell-rwe-minimum-t2-draft_contract_tests-r1-bp-standard-s2026080601`, `cell-rwe-minimum-t2-draft_contract_tests-r2-bp-standard-s2026080602`. Planning completed on `deepseek-v4-pro`; implementation failed closed after 36–39s with `managed workspace action rejected: implementer output must be one JSON workspace action`; review/verify blocked; no seal; no target-default-branch write. Restricted-bundle sha256 `9b345faf744c14d67157856a512b39d90c6e03ff1081783c793b987d6f93bf82`; redacted-bundle sha256 `e2eafa226700061cb000b35dec776ef0b49417aa5faece0b065923b49ee83d3f`. Coordinator aggregate still reports zero provider requests; truthful planning usage is in store-owned workflow events (planning cost about USD 0.00135). Two earlier same-day one-use attempts (`auth-live-v2-001`/`-002`) failed before Provider POST (Golden Path intake off; detached HEAD). The durable B2 rule remains caller-supplied finite `expires_at`.
2. The current two-task/four-cell design is lifecycle viability evidence, not a statistically decision-grade architecture baseline.
3. No accepted operations/evidence manifest, larger decision baseline, or executed contemporary old/new comparison exists; the reconstructable snapshot verifier, real provider-free CLI preflight, and one separately authorized DB RUN now exist. That effect is uniquely bound to packet `PE7-RWE-DB-RUN-1`, run `run-goal-db-baseline-20260814-2340`, authorization `auth-goal-db-run-20260814-2340`, and run-evidence sha256 `a841e6d092d2946de2ee96bef03409ab8c276111c3ace53aef827bd0c00c277e`. ProductTasks `ptask-20260814154014-18cbb634bffe42c2`, `ptask-20260814154055-18cbb63e1d04419d`, `ptask-20260814154136-18cbb647bf8562c7`, and `ptask-20260814154219-18cbb651c35d869f` all ended `failed/execution_failed`; their exact attempt evidence retains workspace IDs `rwe-ws:run-goal-db-baseline-20260814-2340:cell-rwe-minimum-t1-fix_flow_linkage-r1-bp-standard-s2026080601`, `rwe-ws:run-goal-db-baseline-20260814-2340:cell-rwe-minimum-t1-fix_flow_linkage-r2-bp-standard-s2026080602`, `rwe-ws:run-goal-db-baseline-20260814-2340:cell-rwe-minimum-t2-draft_contract_tests-r1-bp-standard-s2026080601`, and `rwe-ws:run-goal-db-baseline-20260814-2340:cell-rwe-minimum-t2-draft_contract_tests-r2-bp-standard-s2026080602`, with `cleanup_status=not_required` for each. No route T3/owner-outcome receipt, analysis receipt, or decision-grade baseline is claimed. `PE7-RWE-DB-ANALYSIS-1` is parked with the run so provider-free AC0 can proceed; do not replay the effect or infer acceptance from its evidence.
4. The AC0 data/trace freeze is complete as a provider-free contract and closeout; AC1 shared ProcessSupervisor hardening remains deferred and is not an active implementation frontier, while AC2–AC7 remain gated by their immediately preceding accepted current-main contract packet.
5. No accepted causal-mutation, lineage/mutation, evaluator/holdout, lifecycle-budget, diversity/exploration, or Pareto/stop/recovery contract or implementation packet exists.
6. No Level-2 rule audit, controller contract, provider-free conformance, live pilot, final transfer, adoption decision, or fixed Meta operator-comparison result exists.
7. No accepted metacognitive-operator, parameter-efficient training adapter, weight/harness factorial, co-evolution, or outer-policy research contract exists; full-weight and model-architecture evolution remain unrouted.
8. The failed bootstrap from accepted main `aa83ac1f5eada74199e0ce28ecb91d37a48769d6` remains valid non-authorizing evidence: it stopped with `route_controller_unavailable_timeout` after GitHub rejected 28 workflow inputs with HTTP 422, before any workflow run, PR, claim, Provider call, target write, or external effect. PR #416 and accepted-main smoke `31631388199` removed that exact dispatch blocker. The route remains stopped until the one-time merge-backed bootstrap starts from current main; route10 remains non-resumable obsolete-main evidence.

## Maintenance Boundary

After each accepted merge, update this file only when an accepted capability or confirmed gap changed. Update `docs/NEXT_DECISION.md` only when the current executable window changed, and update `docs/FUTURE_ROUTE.md` only when long-horizon order or a routing-only sketch changed. Never copy live PR, CI, or review state into any of those documents.

## Safety Boundary

Default-off execution; no provider call in CI; no target-default-branch write; no auto-merge; no release or deployment authority; no reusable credential in a child; no secret, raw prompt, raw output, transcript, private path, fixture-only result, forecast value, memory projection, novelty score, or scalar VDE index may become production-adoption authority.
