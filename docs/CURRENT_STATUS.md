# Current Status

Last updated: 2026-08-25.

This document owns accepted repository truth and confirmed capability gaps only. It separates two states that must not be conflated:

1. **Merged and accepted truth** — code and governing documents on remote `main` that passed their required checks.
2. **Confirmed gaps** — capabilities or evidence not yet accepted on `main`.

Open PR heads, Draft/Ready state, CI, reviews, mergeability, and the next permitted action are live observations and must come from a fresh context capsule. Current execution routing belongs in `docs/NEXT_DECISION.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`. Historical packet detail remains in Git history and merged PRs; do not append stale chronology here.

## Verified Repository State

- Repository: `Igzela/token-efficient-agent-harness-lab`.
- Latest accepted baseline: PR #617 exact head `23675dbff59d030eeae2e7e6cbfebca81f50e66b`, squash-merged as `7caed005a9914e8669a64f6174eab286e160e6d7`; it freezes the provider-free C1 three-axis experiment contract on top of the accepted C0 closeout and v38 lifecycle-cost enforcement baseline. No Provider call, credential-value read, target write, authority consumption, enforcement escape, or live effect was authorized by this contract freeze.
- A new `main`, PR head, CI result, review receipt, or canonical-document change invalidates older context capsules and branch-local status prose.

## Accepted Packet Receipts

This table is the durable cross-document prerequisite index. A packet may appear here only after merge, exact-head review, canonical CI, and canonical-document synchronization have all been established; live PR state still comes from a fresh capsule. Evidence cells bind the minimal verifiable identity (PR, exact head, merge, canonical workflow); chronology and full receipts remain owned by Git history and GitHub.

| Packet | State | Accepted evidence |
|---|---|---|
| `PE7-AC0-DATA-CONTRACT-INVENTORY-1` | `COMPLETE` | PR #466 exact head `1662bde29a53d942a28a9982cc5e9a999ff44c12`; merge `e17767e6ebe1a0d6c6031dec61349deeb3ef9585`; exact-head `PASS`; canonical workflow `31868206197` |
| `PE7-AC0-TRACE-ORDER-FREEZE-1` | `COMPLETE` | PR #467 exact head `19cc238fec27236873262c12998eabe2eda26ac4`; merge `a4879fc60f1c080579df7ba942793a4c94367ff5`; exact-head `PASS`; canonical workflow `31869014363` |
| `PE7-AC2-CONTRACT-1` | `COMPLETE` | PR #469 exact head `142fad048f1d9e8dfb40aa61145108a2fe48f871`; merge `591f8c607804813fe0b809f92f494cb6bcee7820`; exact-head `PASS`; canonical workflow `31871125792` |
| `PE7-AC2-BOUNDARY-REPAIR-1` | `COMPLETE` | PR #475 exact head `e89a24bd6776282bbd52ee72cc5be8ecc66acbc2`; merge `b75cd81620ed51aefce5d245855cf00f1bb6385b`; exact-head `PASS`; canonical workflow `31882791484` |
| `PE7-AC2-CALLER-MIGRATION-1` | `COMPLETE` | PR #478 exact head `4c748ce5f7988da9f61dd1e4650351b5d6c8bf72`; merge `36d7b33a5483cff63715b7981794aff1de614ae2`; exact-head `PASS`; canonical workflow `31886172712` |
| `PE7-RWE-V2-REFREEZE-1` | `COMPLETE` | PR #370 exact head `36c92b93975366c3f85471f247a3afb128e5351c`; merge `3b4afb3e5ab4254904aa5a63473ab6ae0eac1e82`; exact-head `PASS`; canonical workflow `31312135471`; redacted calibration and restricted-bundle digests bound in the PR evidence |
| `PE7-CTRL-ROUTE-CONTRACT-1` | `COMPLETE` | PR #380 exact head `e905cf6ec7a989b54e60f913657ca306f33ebf49`; merge `546cabc1ceb98b49b543d0bd90a62fc228e67338`; exact-head `PASS`; canonical workflow `31386777810`; route-contract receipt bound to the accepted main merge |
| `PE7-PLAN-LANE-ACTIVATION-1` | `COMPLETE` | PR #382 exact head `dde26f884ce8a85b776b5933c84c4e6cfd73cb19`; merge `e55e19f1b7c353b4baa2b40ee7b5b16af8918a6c`; exact-head `PASS`; canonical workflow `31395404498` |
| `PE7-LIFECYCLE-CONTROLLER-1` | `COMPLETE` | PR #385 exact head `5867eb9e35151c8252cda26bb6a956dfe80252b0`; merge `ca7e4585c594a5c9820c8d1267858780c28503ac`; exact-head `PASS`; canonical workflow `31401184171`; plan-packet CI/review/merge/closeout receipts recorded on the ledger as controller-owned transitions with idempotent readback |
| `PE7-CONTROL-BINDING-INTEGRITY-REPAIR-1` | `COMPLETE` | Authority PRs #406/#407/#409; implementation PR #408 exact head `4a2dcf42728ae53f7daaec73e15310e8b0d67b59`; merge `57a86c78c3f9611ce48c5bce249721af23db5532`; exact-head `PASS` on both review axes; canonical workflow `31593460813`; #405 retrospective correction workflow `31594277043` and production readback bind actual head `e68ec0b3a7b78d3ca241922bf3995c2f3ba4ecfa` while retaining `historical_merge_compliant=false` |
| `PE7-WORKSPACE-PREP-RECEIPT-RACE-REPAIR-1` | `COMPLETE` | PR #413 base `59cec5745ddd7f89ce8c099a5de2c7e3c3ec3a1e`; exact head `fc8c005981d2fa12f0f494a131b839d65a46a8ba`; exact-head `PASS` receipt comment `5268787985`; canonical workflow `31611860646`; merge `9cc118fa72d9d13a24cdf968cc5fc20dbe80b28f`; deterministic production-path concurrent-winner receipt reuse and genuine missing-receipt rejection |
| `PE7-ROUTE-AUTONOMY-STABILIZATION-1` | `COMPLETE` | PR #416 exact head `9ce548f620314303b37753a18539c17b5daa6698`; merge `306b500c43270ca83d7cb9defd365140b525187c`; exact-head `PASS`; canonical workflow `31630036965` |
| `PE7-ROUTE-AUTOPILOT-SOAK-1` | `COMPLETE` | PR #426 exact head `c54860674fbf5045239469c2a842ec88002bb3df`; merge `f02d58b5d1fb8d74dd1c68349e4075eb7641879e`; exact-head `PASS`; canonical workflow `31664342318`; companion PR #429 merge `d40c8ce82101922e7270f30bd6da592d72354ffe` |
| `PE7-RWE-V2-PREFLIGHT-GATE-CONTRACT-1` | `COMPLETE` | PR #432 exact head `f31ba002720424deb003728eec52aa9ceae35e33`; merge `710ce06fee68fb75889aa5fa3b9e031b4fdc3a50`; exact-head `PASS`; canonical workflow `31686429471`; contract digest `c8ea4c802e2554b1fa5d0b2f247879ba758d67e4d5df23ed43f1eddadf8aef74` |
| `PE7-RWE-V2-PREFLIGHT-GATE-REPAIR-1` | `COMPLETE` | PR #434 exact head `9fdd1045928f862a5b1c1017bc0e9d73e5d50966`; merge `e311db76bf4d2a3a407213b8129a600bc447fd56`; exact-head `PASS`; canonical workflow `31690000442`; durable B2 rule caller-supplied finite expires_at |
| `PE7-RWE-V2-VIABILITY-PREFLIGHT-1` | `COMPLETE` | PR #437 exact head `4bf6f33c9318369c99a0920eac2048527bea2e83`; merge `97ca257345460e1939662b8ffaf602c0a668028a`; exact-head `PASS`; canonical workflow `31698417170`; unissued request sha256 `015c94e9d65a902f3aba5eae4f3da6cba6d534cc3c57af3a6faf89125663469a` |
| `PE7-RWE-V2-VIABILITY-RUN-1` | `COMPLETE` | PR #441 exact head `ba47462d6cd200d28cb55b1f547924b52afa0584`; merge `2933ba1353f1cda3fc82209b6025094afb79b29e`; exact-head `PASS`; canonical workflow `31704360890`; 4/4 controlled_failure run-live-20260813-v2c auth-live-v2-003 |
| `PE7-RWE-V2-VIABILITY-CLOSEOUT-1` | `COMPLETE` | PR #442 exact head `50e18540f40a8d47c384f2cac74683618f93c273`; merge `8c5c2f85bc5d66c08d730b7d0c69d914af19540c`; exact-head `PASS`; canonical workflow `31710478692` |
| `PE7-RWE-MR-ESTIMANDS-1` | `COMPLETE` | PR #444 exact head `c3b61d1ecd898abfab910f0c2f5c33fa6692acef`; merge `4a0048fcb6785adfb3614769298519c95a01de2f`; exact-head `PASS`; canonical workflow `31765676789`; exact-head check `31765676776` |
| `PE7-RWE-MR-CORPUS-SAMPLING-1` | `COMPLETE` | PR #445 exact head `2ddec8d5e2afce104ee718d64eb517219ecdf888`; merge `3f88d985af3f7701ab9f3c382becb84f73364c9b`; exact-head `PASS`; canonical workflow `31766911605`; exact-head check `31766911606` |
| `PE7-RWE-MR-OPERATIONS-EVIDENCE-1` | `COMPLETE` | PR #446 exact head `34c68d94c1737769c60fb7ea1722b464a5d764aa`; merge `e34d1ae3c3ecf5e6c919c71a3d26d6690a66444`; exact-head review receipt comment `5289427966`; canonical workflow `31769511015`; exact-head check `31769511065` |
| `PE7-RWE-MR-PROTOCOL-FREEZE-1` | `COMPLETE` | PR #447 exact head `00c8592676c5f73447f94b3abc1361087b371196`; merge `f575b10a6de617bf3dab5611900bf0a48727c0c6`; exact-head `PASS`; canonical workflow `31770551762` |
| `PE7-RWE-DB-SNAPSHOT-CORPUS-1` | `COMPLETE` | PR #448 exact head `923d9f750c652a268b3d7944be35f34c2a2f9fac`; merge `a4472b9a0aa9c78d1616e9d22c88c2f6a6405cb8`; exact-head `PASS`; canonical workflow `31773697000` |
| `PE7-RWE-DB-SNAPSHOT-RECONSTRUCT-1` | `COMPLETE` | PR #451 exact head `d48e9853856714a964709956651fc0ac0961315c`; merge `e1ff80b7599d8aec8d64909f937f79c948010392`; exact-head `PASS`; canonical workflow `31790256137` |
| `PE7-RWE-DB-PREFLIGHT-1` | `COMPLETE` | Store-owned Golden Path prerequisite via ProductTask `ptask-20260814135947-18cbb0b9731e62bf`, terminal evidence `product-terminal-ptask-20260814135947-18cbb0b9731e62bf-9-44d49301c781`; provider-free preflight projection sha256 `b8c35d4060d98598ce3e3bc3977a84d125b1df09ff66b2b9f6d9aa4303c03954` |
| `PE7-AC3-CONTRACT-1` | `COMPLETE` | PR #486 exact head `9487a73ab9e00018103193d18c848e375b215a1b`; merge `6b2a6c46d30089800394ee82edd21075a2ef0d86`; exact-head `PASS`; canonical workflow `31922547776` |
| `PE7-AC3-ORCHESTRATOR-CORE-1` | `COMPLETE` | PR #533 exact head `7c3f3de61b2ecc29fd3512e693948d52d511f4a3`; merge `77dac008a60cb569d3b9a8eb1a3e013e47743387`; exact-head `PASS`; canonical workflow `31984430115` |
| `PE7-AC3-PORT-MIGRATION-1` | `COMPLETE` | PR #535 exact head `8c52fc1201025844dcbeb72dc31cc1217acd8f9e`; merge `3a58cf57abd0a09ea63bfcacad17c815af272de8`; exact-head `PASS`; canonical workflow `31985387700` |
| `PE7-AC4-CONTRACT-1` | `COMPLETE` | PR #538 exact head `2be275e714bc753eb492f2545d6174a72d7f87e6`; merge `437838289e48907767321363c394cf095b9f41dd`; exact-head `PASS`; canonical workflow `31986463688` |
| `PE7-AC4-VIEWS-CORE-1` | `COMPLETE` | PR #540 exact head `541df294dafdc2c9a3dcea3c07ae3945af7e0d46`; merge `3f88b37e070c572df3acb86d598c885e80193652`; exact-head `PASS`; canonical workflow `31988264703` |
| `PE7-AC4-CALLER-MIGRATION-1` | `COMPLETE` | PR #542 exact head `b0fb353ac4334ff906b503f1c710c9b8be71d9bb`; merge `658e8ce4619447c64712d2965ba3109d18ee5c7f`; exact-head `PASS`; canonical workflow `31999114312` |
| `PE7-AC5-CONTRACT-1` | `COMPLETE` | PR #544 exact head `fb4c412adafe69c6e3b1432ecc005addc9d70b3e`; merge `a69e7f18281a87eea932fd9b80c80ebe1d214191`; exact-head `PASS`; canonical workflow `32000036819` |
| `PE7-AC5-ROOT-CORE-1` | `COMPLETE` | PR #546 exact head `e4c6b6ca601a5aab715f953f3d3229e342a54c50`; merge `a3a515e9d43a4ffa3d0c180bcd137d5034cf33ae`; exact-head `PASS`; canonical workflow `32000687882` |
| `PE7-AC5-MODULE-MIGRATION-1` | `COMPLETE` | PR #548 exact head `a10175092de7ba7cc06d41a9a60d9d333dbb3bb2`; merge `4455832eabdb46fd17d6a14ec3ee849bfda04868`; exact-head `PASS`; canonical workflow `32002409448` |
| `PE7-AC6-CONTRACT-1` | `COMPLETE` | PR #550 exact head `5e7f0130b8ca518e5d30d198f7209332e77c99bb`; merge `baa21e0495154e6ba00d4f06452679b9a6722e0b`; exact-head `PASS`; canonical workflow `32003759191` |
| `PE7-AC6-RUST-CODEGEN-1` | `COMPLETE` | PR #552 exact head `aae64b55af549d9acf3aec9f204383f3341d6c2e`; merge `11f00f99c46a864f31033fb43aaabafeaaf142c8`; exact-head `PASS`; canonical workflow `32004338480` |
| `PE7-AC6-SDK-MIGRATION-1` | `COMPLETE` | PR #554 exact head `f7f18e1b22c6d16f325bdcb153726d611f9b5761`; merge `77f41084ebd5076e01b1a73fbea821dbc44a98d5`; exact-head `PASS`; canonical workflow `32005606558` |
| `PE7-AC6-DASHBOARD-MIGRATION-1` | `COMPLETE` | PR #556 exact head `25feca06d5b7af32983479eb8a1f53e1f1da2e5f`; merge `9189bb83fd742b7bf489fca1124a4563bbd5ee22`; exact-head `PASS`; canonical workflow `32006356641` |
| `PE7-AC6-COMPATIBILITY-CLOSEOUT-1` | `COMPLETE` | PR #558 exact head `646e076cba8b349d45880dd84d4520109a11db69`; merge `4ea5f7707fa8c1f370cb8a8323c0b017bfcb3443`; exact-head `PASS`; canonical workflow `32006997709` |
| `PE7-AC7-REMOVAL-MANIFEST-1` | `COMPLETE` | PR #560 exact head `5567c670cb0338bf3bf089db95757714365829ec`; merge `eb692703ab3b3d030478b539fff4496014e45c7a`; exact-head `PASS`; canonical workflow `32015963930` |
| `PE7-AC7-CLEANUP-1` | `COMPLETE` | PR #562 exact head `84735a064466b81a5bf521cf20b1a924c80408e6`; merge `8142a447c1b9ca861978bd3392da5ccea4263924`; exact-head `PASS`; canonical workflow `32026577558` |
| `PE7-AC7-CLOSEOUT-1` | `COMPLETE` | PR #563 exact head `80e68a108eb1d752f6632944300786fe9ea6511d`; merge `42fcfa5ad7e349d27d3caa815163340f9c0d5c0b`; exact-head `PASS`; canonical workflow `32030794178` |
| `PE7-RWE-CR-RECONSTRUCTION-1` | `COMPLETE` | PR #566 exact head `57f4a5ee3a9be48a6ebdc20eddbd5df978c4440f`; merge `7cfa817a82ea3a638bd3e50af5266ee54eefe0c0`; exact-head `PASS`; canonical workflow `32103730088` |
| `PE7-RWE-CR-PROTOCOL-PREFLIGHT-REPAIR-1` | `COMPLETE` | PR #572 exact head `0f63ad49c2b5ba87bf5e661bcbae9fd5fab9a9a8`; merge `262b67b675c36859c3dee6e1556fa0090654b75c`; exact-head `PASS`; canonical workflow `32137758400` |
| `PE7-RWE-CR-PROTOCOL-PREFLIGHT-1` | `COMPLETE` | PR #577 exact head `1bfffe1c620cff79caf37bd566f9ee80073d252e`; merge `9c25d193d3b85ad9e7cc66af21a0c78ba0171d7a`; exact-head `PASS`; canonical workflow `32276756829`; companion PR #576 merge `837ae2aadc0470713121361d5c529d6936e8926f` |
| `PE7-CWS-INGRESS-INVENTORY-1` | `COMPLETE` | PR #579 exact head `b91f207eba8d5910dd97c626c458be0e369c577e`; merge `76d21ea2fd4d8a691bc83c28d680e5affff77ba2`; exact-head `PASS`; canonical workflow `32279656821` |
| `PE7-CWS-PROJECTION-CONTRACT-1` | `COMPLETE` | PR #580 exact head `0a750a3a5cda92b419efbfb35f89f5cfee0fe429`; merge `4129ca5d08cd7a2e89ad2485864ba28900ecc645`; exact-head `PASS`; canonical workflow `32280864211` |
| `PE7-CWS-REHYDRATION-CONTRACT-1` | `COMPLETE` | PR #581 exact head `b7b4037bd31731e1ba0f16904006d38bf4c78b82`; merge `1b6d73fce72cb195578ae5af784203f7de274e9f`; exact-head `PASS`; canonical workflow `32281612446` |
| `PE7-CWS-PROJECTOR-CORE-1` | `COMPLETE` | PR #582 exact head `cdcd41655aa098b46cdf7d2ee12031d1860e71c2`; merge `07446ffe1cb31e49ace25e36deb6233433a3814e`; exact-head `PASS`; canonical workflow `32284433657` |
| `PE7-CWS-TOOL-RESULT-REDUCTION-1` | `COMPLETE` | PR #583 exact head `bd793a7ea449e96df9576876bc38003d6f295be1`; merge `2af00e19463a10a58c44a52587ceb78114b23538`; exact-head `PASS`; canonical workflow `32286825170` |
| `PE7-CWS-REPOSITORY-INTEGRATION-1` | `COMPLETE` | PR #584 exact head `323d479d73f26f280cf28502e3c609d4baf78298`; merge `d33d7d04709575d1f6fb9fdbe94169175a261108`; exact-head `PASS`; canonical workflow `32290928328` |
| `PE7-CWS-RUNTIME-INTEGRATION-1` | `COMPLETE` | PR #585 exact head `7cbe7a0f3660468862302075f024b627a26a0a2e`; merge `1dffbc4271a68aebce93a540e7a5793eacefa546`; exact-head `PASS`; canonical workflow `32292746487` |
| `PE7-CWS-CACHE-PARTITION-1` | `COMPLETE` | PR #586 exact head `ecb1367a26d56a633902f0685b3d13d02efff9b4`; merge `5a3929dc97b0a94bcec0a95b6e77450238d437da`; exact-head `PASS`; canonical workflow `32294752392` |
| `PE7-CWS-BENCHMARK-PROTOCOL-1` | `COMPLETE` | PR #587 exact head `fe9372732559ffab61b7e98fb81c578cd61bd3fc`; merge `f561089103a4a6e51b47f38d6640054ec8a660d0`; exact-head `PASS`; canonical workflow `32296178643` |
| `PE7-CWS-BENCHMARK-PREFLIGHT-1` | `COMPLETE` | PR #588 exact head `c806f75c5910b117c3cf7e44ad1c6a6503e48ddd`; merge `1569c70e9f2034bb4f7bc5ccbc24d889b66645ab`; exact-head `PASS`; canonical workflow `32297108984` |
| `PE7-CWS-BENCHMARK-RUN-1` | `COMPLETE` | PR #589 exact head `0f9cad12a850a7ed2ffcc823ebd2da29318c5ae6`; merge `84b1933bc3d9e657acae94d9e5f14810c0651917`; exact-head `PASS`; canonical workflow `32298813456` |
| `PE7-CWS-ANALYSIS-1` | `COMPLETE` | PR #590 exact head `da09ea576154e55e532d2de5477972f2c5c516d5`; merge `1544c8d0a3f1b196fdb4b560759609662cd5f432`; exact-head `PASS`; canonical workflow `32301497907` |
| `PE7-HE-EC1-CONTRACT-1` | `COMPLETE` | PR #591 exact head `50661a622c19e1f6da1f934a43bcbbaa4b52a003`; merge `e116e212ed043d773e215f2ba029e5b2f1763e4d`; exact-head `PASS`; canonical workflow `32306087501` |
| `PE7-HE-EC1-IDENTITY-LINEAGE-1` | `COMPLETE` | PR #592 exact head `155fa749effdcd790fb954eefcf64d12790d21b6`; merge `3dc2d3b12fbb95ec2b26220681cba5ad7547c6d2`; exact-head `PASS`; canonical workflow `32309602816` |
| `PE7-HE-EC1-CAUSAL-MANIFEST-1` | `COMPLETE` | PR #593 exact head `c00f24dac433d9b3fc23f5b0df746c89442097dd`; merge `b2fa400395a0502bf52ea5fd9468af5830766422`; exact-head `PASS`; canonical workflow `32311374839` |
| `PE7-HE-EC1-MUTATION-REGISTRY-1` | `COMPLETE` | PR #594 exact head `b3199736d85312083c45a3522211ae086f5fe756`; merge `b970226181957de98859f26f03db3bf101b1f8a0`; exact-head `PASS`; canonical workflow `32313718374` |
| `PE7-HE-EC2-CONTRACT-1` | `COMPLETE` | PR #595 exact head `e0585701dec206fca5645299d65cbb3341257008`; merge `f996ded631f12f74f42528c70e76ccf0f040bdfd`; exact-head `PASS`; canonical workflow `32317253205` |
| `PE7-HE-EC2-HOLDOUT-SEAL-1` | `COMPLETE` | PR #596 exact head `cffd49edfc36fe602cc311f025367cadb15a425a`; merge `5c367b85d79f680b5f76b7aa4f2f1656c0a460ae`; exact-head `PASS`; canonical workflow `32320235684` |
| `PE7-HE-EC2-SENTINEL-CONFORMANCE-1` | `COMPLETE` | PR #597 exact head `4e39a52a265d4a9e3a6902c68da142b424b15c36`; merge `dbe20eccb4980e595958d615cf937ba34cfdaed2`; exact-head `PASS`; canonical workflow `32321977265` |
| `PE7-HE-EC2-PREDICTION-OUTCOME-1` | `COMPLETE` | PR #600 exact head `0ccdbefa59e18b241cba7cb6f26f3d267608a9a9`; merge `ac2b2f640406ca766b0cd567c2782e426d8dad2b`; exact-head `PASS`; canonical workflow `32633510108` |
| `PE7-HE-EC3-CONTRACT-1` | `COMPLETE` | PR #603 exact head `c1c1c23eb68d11f38fd85623f412dd13b5c867e1`; merge `d1b939865e5dcf3b11093e1e6932078e55068054`; exact-head `PASS`; canonical workflow `32646001459` |
| `PE7-HE-EC3-INSTRUMENTATION-1` | `COMPLETE` | PR #608 exact head `36474545563bd1b91015d4e3f2005f12dd43bde9`; merge `789b7dba9afdd5e1e6e41d191ebcbbfa933b2c12`; exact-head `PASS`; canonical workflow `32687392603` |
| `PE7-HE-EC3-ENFORCEMENT-1` | `COMPLETE` | PR #610 exact head `076885e88cc7d2dfe1f7a64da1ee5c88b8d97c3b`; merge `720c9c90ce95c5831693916bd4feea81af513f4c`; exact-head `PASS`; canonical workflow `32710034017` |
| `PE7-HE-CL0-PILOT-1` | `COMPLETE` | Authorized finite artifact-only effect 2026-08-24: task `ptask-20260824115348-18cebba70936f600`, delegated terminal receipt `0b85bac66a61a8565bd7be238471c0551cd607f3c801dd1c785af2c232e51f25`; realized cost 0.0023360868 USD, workspace cleaned |
| `PE7-HE-CL0-CLOSEOUT-1` | `COMPLETE` | PR #614 exact head `c464235313255b224c481214be16f7a24831e379`; merge `075f995b574fb8a28f08986291751152bf158dd5`; exact-head `PASS`; canonical workflow `32812721310` |
| `PE7-HE-MX1-CONTRACT-1` | `COMPLETE` | PR #617 exact head `23675dbff59d030eeae2e7e6cbfebca81f50e66b`; merge `7caed005a9914e8669a64f6174eab286e160e6d7`; exact-head `PASS`; canonical workflow `32828369869` |

Durable closeout consequences (receipts above own identity): AC7 cleanup/closeout proved a merged-tree fixed-string zero-match inventory with separate approve/output authority paths intact; its rollback anchor is revert-PR #562 to pre-cleanup tree `eb692703ab3b3d030478b539fff4496014e45c7a`. Lifecycle-cost receipt fields remain defined by the Architecture Book evidence contract; normalized CI-compute aggregates were never instrumented and stay explicitly unavailable rather than estimated.

## Context working-set ingress inventory (`PE7-CWS-INGRESS-INVENTORY-1`)

Provider-free read-only inventory. Conversation text is not durable truth. No prompt/runtime/store change. Harvest `candidate_status` is not an implementation disposition.

### Ingress matrix

| Path | Owner | Authority class | Size/repetition | Sensitivity | Current reduction | Exact recovery |
|---|---|---|---|---|---|---|
| Coding session entry JSON | `scripts/session_context.py` | packet/dispatch capsule; not product truth | bounded JSON; one entry per session | no secrets; paths allowlisted | digest-bound projection | yes, regenerate from accepted main |
| Frontier evidence capsule | `scripts/project_context.py` | live GitHub observation; not accepted truth | truncated live fields | no raw transcripts | short-lived artifact | yes, regenerate |
| Plan/implement/review prompts | `scripts/agent-control/prompt_builder.py` | repository-maintenance only | template + exact-head diff bound | no credential values | max review-diff chars | yes, from PR/issue + template |
| Local claim-bound plan run | `scripts/agent-control/local_run_once.py` | existing plan-lane owners | one claim per run | no reusable child credential | existing prompt builders | yes, from claim receipts |
| HTTP product/task handlers | `engine/src/http_server/` | LocalProductStore-backed | request-scoped | auth-scoped; no secret echo | existing handlers | store-owned records |
| Provider transport | `engine/src/provider/` | existing provider owner; default-off in CI | request-scoped | credentials stay in env, not prompts | existing redaction | no: raw provider payloads are not retained as authority |
| Agent scratchpad/notes | `engine/src/node_executor.rs` | agent-state summary only | bounded summary | summaries, not raw bodies, in audit | `update_scratchpad_summary` | store agent_state; raw bodies excluded |
| Durable memory versions | `engine/src/storage/local_product_store/` `durable_memory_versions` | experimental/store-owned; not CWS authority | versioned rows | store-scoped | existing retrieval events | yes, store versions |
| Tool results / artifacts | existing artifact/evidence owners | evidence, not model truth | large logs possible | may contain diagnostics | CWS reducer `REIMPLEMENT` in `reduce_tool_result`; raw stays with artifact owner | rehydrate from `ARTIFACT_REF` handle |
| Canonical docs / Git | `docs/*`, Git objects | accepted-main truth | full files | public prose | role-targeted reads | yes, exact Git identity |
| Shared Sol investigation | `scripts/ask_sol.py` | read-only consultation; not product authority | bounded goal/hypothesis | no worktree mutation; no secrets in receipts | existing script redaction | yes, from consultation receipts |
| Managed CLI / Codex child context | `engine/src/cli/` | existing CLI mediation; not a second provider owner | child-process prompt assembly | credentials stay out of child env | existing mediation | no: child raw prompts are not authority |

Unknown ingress: none identified beyond the rows above at this checkout. A later packet must not treat chat history as a source row.

### Harvest matrix (non-authoritative; `REFRESH_AT_PROMOTION`)

| Candidate | Public source | License/NOTICE | `candidate_status` | Notes |
|---|---|---|---|---|
| Gemini CLI | github.com/google-gemini/gemini-cli | re-verify at promotion | `UNKNOWN` | harvest identity not frozen |
| OpenAI Codex CLI | github.com/openai/codex | re-verify at promotion | `UNKNOWN` | harvest identity not frozen |
| Aider | github.com/Aider-AI/aider | re-verify at promotion | `UNKNOWN` | harvest identity not frozen |
| Command Code | unpublished harness source | n/a | `INELIGIBLE_SOURCE` | architecture/behavior reference only; not a transplant candidate |

No harvest-candidate `TRANSPLANT` / `ADAPT` / `REJECT` is recorded. Projector and reducer implementation-selection dispositions are each `REIMPLEMENT` (see below); an INGRESS `candidate_status` is not that decision.

### Working-set residency policy (`PE7-CWS-PROJECTION-CONTRACT-1`)

Derived, deletable, rebuildable projection. Not a second store or evaluator.

| Class | Holds | Eviction |
|---|---|---|
| `PINNED` | authority, unresolved blockers, outcome-unknown evidence, exact packet/task bindings, allowed/forbidden scope, verification contracts | never by relevance score |
| `HOT` | active-path working items with exact source identity | lexicographic demotion to WARM only after PINNED capacity is satisfied |
| `WARM` | recently used source-bound items | lexicographic demotion to COLD; never if it would drop PINNED |
| `COLD` | reduced handles requiring rehydration | drop only when a source-bound rehydration recipe remains |

Promotion/demotion is deterministic and independent of embedding/model scores for all safety-relevant decisions.

### Rehydration policy (`PE7-CWS-REHYDRATION-CONTRACT-1`)

A reduced or `COLD` item is a handle, not a second copy of truth. Rehydration reconstructs model-visible bytes from an existing owner. It never creates a hidden transcript store, never expands permissions, and never authorizes repeating an EFFECT.

Handle fields (all required unless noted): `source_owner`, `source_identity` (Git object SHA + path, or existing artifact id), `content_sha256`, optional `byte_range`, `recipe_kind`, `redaction_class`.

| `recipe_kind` | Retrieval | May repeat EFFECT? |
|---|---|---|
| `GIT_BLOB` | exact Git object + path at the bound SHA | no |
| `ARTIFACT_REF` | existing artifact/evidence owner by id + hash | no |
| `DETERMINISTIC_RERUN` | named provider-free local recipe with identical inputs | no; stop if the recipe would call a Provider, write a target, or consume authority |

Integrity: retrieved bytes must match `content_sha256`; mismatch is `UNAVAILABLE`, not a repaired substitute. Freshness is identity-bound: “latest file” is not a recipe. Redaction at rehydration must be at least as strict as admission; secrets and private paths stay out. Missing source, missing hash, or failed owner lookup is `UNAVAILABLE`. Outcome-unknown evidence rehydrates only as the original unknown receipt; it does not become success.

Named fail-closed vectors for later IMPLEMENT packets:

1. PINNED authority handle → same digest as the bound Git/doc identity.
2. COLD tool-result handle → `ARTIFACT_REF` only; raw log is not copied into a new store.
3. Missing or mismatched `content_sha256` → `UNAVAILABLE`, never empty success.
4. `DETERMINISTIC_RERUN` that would POST a Provider or write a target → rejected; not an implicit RUN-1.
5. Summary-only handle with no source identity → not rehydratable; packet stop.

### Projector-core disposition (`PE7-CWS-PROJECTOR-CORE-1`)

Implementation-selection disposition is `REIMPLEMENT`. Ingress `candidate_status` values remain `UNKNOWN` or `INELIGIBLE_SOURCE` and are not a TRANSPLANT decision. The pure projector lives in `engine/src/context_working_set.rs` and is consumed by the existing `context_pack` owner. It does not persist, call a Provider, or become a second memory/store/evaluator.

### Tool-result reducer disposition (`PE7-CWS-TOOL-RESULT-REDUCTION-1`)

Reducer disposition is `REIMPLEMENT`. Large tool results stay with existing artifact owners. The model-visible slice is bounded, redacted through the existing provider redaction owner, and bound to a raw `ARTIFACT_REF` handle. Failure and unknown outcomes cannot become success; required failure diagnostics cannot be dropped by truncation.

### Repository session projection (`PE7-CWS-REPOSITORY-INTEGRATION-1`)

`project_repository_session` binds accepted main, head, packet, and mode as PINNED items consumed by the existing `context_pack` owner. Claim-bound prompts attach identity/hash handles via `cws_session_projection_block` instead of a second full canonical-document copy. Fresh sessions fail closed on a changed head. Capsules remain non-authoritative.

### Runtime prompt composition (`PE7-CWS-RUNTIME-INTEGRATION-1`)

`compose_runtime_prompt` turns an already-projected working set into a provider-free prompt: PINNED prefix, dynamic items, then cold rehydration handles. The stub provider is used only as a deterministic hash oracle; it does not own context. Outcome-unknown and cancellation text are preserved.

### Cache partition (`PE7-CWS-CACHE-PARTITION-1`)

Disposition is `REIMPLEMENT`. `partition_working_set` hashes the PINNED prefix separately from dynamic items and cold handles. Optional `CacheTelemetryObservation` is attached as observation only: missing cached-token or cache-write fields stay `None` and never become zero. Changing telemetry cannot change digests or authorize work.

### CWS benchmark protocol (`PE7-CWS-BENCHMARK-PROTOCOL-1`)

Hard-gate-first comparison. Treatment may differ from the post-AC baseline only in accepted CWS projection (`compose_runtime_prompt` + `partition_working_set`). This freeze does not authorize a Provider request.

| Field | Frozen value |
|---|---|
| Arms | `baseline` = post-AC runtime without CWS compose/partition; `treatment` = same runtime with CWS compose/partition |
| Tasks | Reconstructable post-AC scorecard/fixture task identities only; no selective replacement after freeze |
| Seeds | Packet-bound, recorded before observation |
| Provider/model/tool | Then-current accepted identities at preflight; must match both arms |
| Toggle | Explicit `cws_projection=off\|on`; no other treatment delta |
| Quality / non-inferiority | Existing scorecard/evaluator owners; gates frozen before outcomes |
| Context metrics | Repeated and total input tokens/bytes from existing usage owners |
| Cache telemetry | Observe `cached_input_tokens` / `cache_write_tokens` when present; missing stays missing, never zero |
| Rehydration / tool / retry / latency / cost | Existing artifact, tool, retry, and cost owners |
| Missingness | Unknown stays unknown; incomplete arm is not success |
| Analysis | Hard gates first; then maintenance-burden evidence only (upstream-derived LOC, adapter LOC, new LOC, retained tests, dependency delta, source identities, patch/upgrade burden) |
| Stop | Authority expiry, unknown EFFECT, incomparable arms, post-hoc threshold, burden used as evaluator |

Maintenance-burden metrics are not evaluator or acceptance authority.

### CWS benchmark preflight (`PE7-CWS-BENCHMARK-PREFLIGHT-1`)

`cws_benchmark_preflight` binds protocol main `f5610891`, baseline `cws_projection=off`, treatment `cws_projection=on`, cache telemetry not required, and exactly one unissued T3 authorization package with `authorizations_issued=false`. `ready=false` when provider capability or evidence paths are unverified. This packet does not POST a Provider or inspect comparison results.

### CWS benchmark run (`PE7-CWS-BENCHMARK-RUN-1`)

`cws_benchmark_run` fail-closes when preflight is not ready or the T3 package is unissued. Observed this environment: no Provider credential in process env; preflight `authorizations_issued=false`. The runner records `executed=false` and `provider_posts=0`. It does not invent arm terminals or POST a Provider.

### CWS analysis (`PE7-CWS-ANALYSIS-1`)

Disposition is `INSUFFICIENT_DEFAULT_OFF`. Live arms were not observed; hard gates did not pass. The active Harness identity for later HE packets is accepted main `84b1933bc3d9e657acae94d9e5f14810c0651917` with CWS default-off. ENABLE is not recorded. Implementation-maintenance burden (REIMPLEMENT projector/reducer, no upstream transplant LOC) is decision evidence only and is not evaluator authority.

### HE EC1 contract (`PE7-HE-EC1-CONTRACT-1`)

`engine/src/harness_evolution.rs` freezes `FailurePatternEvidenceV1`, `MutationHypothesisManifestV1`, `PredictionOutcomeV1`, and a pre-registered mutation-family registry under the existing HE owner. Active Harness identity is CWS default-off SHA `84b1933b`. Causal status may be `unknown` or `disputed`. Prediction outcomes are not evaluator, admission, or adoption authority. No candidate generation, evaluation, or persistence change.

### HE EC1 identity lineage (`PE7-HE-EC1-IDENTITY-LINEAGE-1`)

Immutable `Ec1IdentityLineageRecord` rows persist under `LocalProductStore` table `harness_evolution_ec1_identity_lineage`. Lineage IDs are derived; parent rows must exist; causal sources cannot orphan; active Harness is CWS default-off `84b1933bc3d9e657acae94d9e5f14810c0651917`. No selection, adoption, or second store.

### HE EC1 causal manifest (`PE7-HE-EC1-CAUSAL-MANIFEST-1`)

Source-bound `FailurePatternEvidenceV1` and pre-execution `MutationHypothesisManifestV1` persist under existing `LocalProductStore` HE tables. Observation vs inference is explicit; causal status is not proof. Hypotheses are insert-only and must bind a proposal digest. Sensitive fields remain refused. No candidate execution or evaluator result.

### HE EC1 mutation registry (`PE7-HE-EC1-MUTATION-REGISTRY-1`)

Bounded generator adapters bind each candidate to the accepted mutation-family registry and an addressable causal hypothesis. Unknown families are rejected. The adapter cannot edit the registry or evaluator. No ENABLE or Level-1.

### HE EC2 contract (`PE7-HE-EC2-CONTRACT-1`)

`engine/src/harness_evolution_eval.rs` freezes the evaluator/holdout/access/sentinel/outcome/review constellation. Candidates cannot observe plaintext labels. Prediction accuracy is not selection authority. Validate recomputes component and manifest digests. No holdout access implementation and no Level-1.

### HE EC2 holdout seal (`PE7-HE-EC2-HOLDOUT-SEAL-1`)

Sealed holdout membership is hash-only under `LocalProductStore` table `harness_evolution_ec2_holdout_seals`. Candidate and operator reads fail closed. Evaluator/reviewer see hashes. Rotation inserts a later epoch; prior vaults are derived `INVALIDATED`. No candidate run.

### HE EC2 sentinel conformance (`PE7-HE-EC2-SENTINEL-CONFORMANCE-1`)

Contamination, evaluator-gaming, and safety sentinel observations now fail closed before Pareto selection and retain rejected-candidate evidence. No ENABLE, Level-1, or second evaluator.

### HE EC2 prediction outcome (`PE7-HE-EC2-PREDICTION-OUTCOME-1`)

The existing evaluator path now derives immutable `PredictionOutcomeV1` records from frozen EC1 hypotheses and actual evaluation evidence, then persists them atomically with the evaluation bundle and receipt through `LocalProductStore` table `harness_evolution_ec2_prediction_outcomes` (additive migration v37 with SQLite/PostgreSQL parity). Candidate and operator reads fail closed, sentinel-rejected evidence records an unavailable outcome when a bound hypothesis exists, and prediction accuracy cannot gate Pareto selection or candidate status.

### HE EC3 lifecycle-budget contract (`PE7-HE-EC3-CONTRACT-1`)

`Ec3LifecycleBudgetContractV1` now freezes 11 lifecycle phases, six canonical resource dimensions, measured-direct/deterministic-derived/explicit-unavailable source semantics, fail-closed missingness, explicit-zero proof, per-candidate/global envelopes, reservation-before-execution vocabulary, exact-once terminal reconciliation vocabulary, and charge-all-attempts failure accounting. It is an accounting contract only: it performs no persistence, reservation, spend, admission, candidate execution, or external effect and creates no second authority owner.

### HE EC3 lifecycle-cost instrumentation (`PE7-HE-EC3-INSTRUMENTATION-1`)

The accepted owner path now captures, normalizes, and immutably persists source-bound lifecycle-cost observations in additive schema v38 through `LocalProductStore`, with SQLite/PostgreSQL replay, conflict, integrity, rollback, and parity coverage. The read-only operator projection exposes only redacted metadata and explicit missingness; no reservation, enforcement, Provider call, target write, or live effect is accepted.

## Invalidated Historical Receipts (Repair Required)

The following invalidated/superseded receipts were merged on historical PRs without complete production implementations, valid independent reviews, or required gate enforcement; they are excluded from the accepted prerequisite index until genuine re-execution and re-proof. Git history owns their merges:

| Packet | Historical PR / Merge | Audit Disposition |
|---|---|---|
| `PE7-SUCCESSOR-PROMOTION-ESCALATION-1` | PR #387 | `SUPERSEDED`: Replaced by PR #532 route promotion and gate integrity repair |
| `PE7-ROUTE-AUTOMATION-1` | PR #390 | `SUPERSEDED`: Replaced by PR #532 route promotion and gate integrity repair |
| `PE7-AC4-CONTRACT-1` | PR #490 | `REPAIR_REQUIRED`: Blocked behind AC3 chain |
| `PE7-AC4-VIEWS-CORE-1` | PR #491 | `REPAIR_REQUIRED`: Blocked behind `PE7-AC4-CONTRACT-1` |
| `PE7-AC4-CALLER-MIGRATION-1` | PR #493 | `REPAIR_REQUIRED`: Blocked behind `PE7-AC4-VIEWS-CORE-1` |
| `PE7-AC5-CONTRACT-1` | PR #494 | `REPAIR_REQUIRED`: Blocked behind AC4 chain |
| `PE7-AC5-ROOT-CORE-1` | PR #495 | `REPAIR_REQUIRED`: Blocked behind `PE7-AC5-CONTRACT-1` |
| `PE7-AC5-MODULE-MIGRATION-1` | PR #497 | `REPAIR_REQUIRED`: Blocked behind `PE7-AC5-ROOT-CORE-1` |
| `PE7-AC6-CONTRACT-1` | PR #499 | `REPAIR_REQUIRED`: Blocked behind AC5 chain |
| `PE7-AC6-RUST-CODEGEN-1` | PR #500 | `REPAIR_REQUIRED`: Blocked behind `PE7-AC6-CONTRACT-1` |
| `PE7-AC6-SDK-MIGRATION-1` | PR #501 | `REPAIR_REQUIRED`: Blocked behind `PE7-AC6-RUST-CODEGEN-1` |
| `PE7-AC6-DASHBOARD-MIGRATION-1` | PR #503 | `REPAIR_REQUIRED`: Blocked behind `PE7-AC6-SDK-MIGRATION-1` |
| `PE7-AC6-COMPATIBILITY-CLOSEOUT-1` | PR #504 | `REPAIR_REQUIRED`: Blocked behind `PE7-AC6-DASHBOARD-MIGRATION-1` |
| `PE7-AC7-REMOVAL-MANIFEST-1` | PR #505 | `REPAIR_REQUIRED`: Blocked behind AC6 chain |
| `PE7-AC7-CLEANUP-1` | PR #506 | `REPAIR_REQUIRED`: Blocked behind `PE7-AC7-REMOVAL-MANIFEST-1` |
| `PE7-AC7-CLOSEOUT-1` | PR #507 | `REPAIR_REQUIRED`: Blocked behind `PE7-AC7-CLEANUP-1` |
| `PE7-RWE-CR-RECONSTRUCTION-1` | PR #508 | `REPAIR_REQUIRED`: Blocked behind AC7 chain |
| `PE7-RWE-CR-PROTOCOL-PREFLIGHT-1` | PR #509 | `REPAIR_REQUIRED`: Blocked behind `PE7-RWE-CR-RECONSTRUCTION-1` |
| `PE7-RWE-CR-RUN-1` | PR #510 | `REPAIR_REQUIRED`: Blocked behind `PE7-RWE-CR-PROTOCOL-PREFLIGHT-1` |
| `PE7-RWE-CR-ANALYSIS-1` | PR #511 | `REPAIR_REQUIRED`: Blocked behind `PE7-RWE-CR-RUN-1` |
| `PE7-CWS-INGRESS-INVENTORY-1` | PR #512 | `REPAIR_REQUIRED`: Blocked behind RWE Contemporary Reconstruction |
| `PE7-CWS-PROJECTION-CONTRACT-1` | PR #513 | `REPAIR_REQUIRED`: Blocked behind `PE7-CWS-INGRESS-INVENTORY-1` |
| `PE7-CWS-REHYDRATION-CONTRACT-1` | PR #514 | `REPAIR_REQUIRED`: Blocked behind `PE7-CWS-PROJECTION-CONTRACT-1` |
| `PE7-CWS-PROJECTOR-CORE-1` | PR #515 | `REPAIR_REQUIRED`: Blocked behind `PE7-CWS-REHYDRATION-CONTRACT-1` |
| `PE7-CWS-TOOL-RESULT-REDUCTION-1` | PR #516 | `REPAIR_REQUIRED`: Blocked behind `PE7-CWS-PROJECTOR-CORE-1` |
| `PE7-CWS-REPOSITORY-INTEGRATION-1` | PR #517 | `REPAIR_REQUIRED`: Blocked behind `PE7-CWS-TOOL-RESULT-REDUCTION-1` |
| `PE7-CWS-RUNTIME-INTEGRATION-1` | PR #518 | `REPAIR_REQUIRED`: Blocked behind `PE7-CWS-REPOSITORY-INTEGRATION-1` |
| `PE7-CWS-CACHE-PARTITION-1` | PR #519 | `REPAIR_REQUIRED`: Blocked behind `PE7-CWS-RUNTIME-INTEGRATION-1` |
| `PE7-CWS-BENCHMARK-PROTOCOL-1` | PR #520 | `REPAIR_REQUIRED`: Blocked behind `PE7-CWS-CACHE-PARTITION-1` |
| `PE7-CWS-BENCHMARK-PREFLIGHT-1` | PR #521 | `REPAIR_REQUIRED`: Blocked behind `PE7-CWS-BENCHMARK-PROTOCOL-1` |
| `PE7-CWS-BENCHMARK-RUN-1` | PR #522 | `REPAIR_REQUIRED`: Blocked behind `PE7-CWS-BENCHMARK-PREFLIGHT-1` |
| `PE7-CWS-ANALYSIS-1` | PR #523 | `REPAIR_REQUIRED`: Blocked behind `PE7-CWS-BENCHMARK-RUN-1` |
| `PE7-HE-EC1-CONTRACT-1` | PR #524 | `REPAIR_REQUIRED`: Blocked behind CWS chain |
| `PE7-HE-EC1-IDENTITY-LINEAGE-1` | PR #525 | `REPAIR_REQUIRED`: Blocked behind `PE7-HE-EC1-CONTRACT-1` |
| `PE7-HE-EC1-CAUSAL-MANIFEST-1` | PR #526 | `REPAIR_REQUIRED`: Blocked behind `PE7-HE-EC1-IDENTITY-LINEAGE-1` |
| `PE7-HE-EC1-MUTATION-REGISTRY-1` | PR #527 | `REPAIR_REQUIRED`: Blocked behind `PE7-HE-EC1-CAUSAL-MANIFEST-1` |
| `PE7-HE-EC2-CONTRACT-1` | PR #529 | `REPAIR_REQUIRED`: Blocked behind EC1 chain |
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
| `PE7-WORKSPACE-PREP-RECEIPT-RACE-REPAIR-1` | `COMPLETE` | PR #413 accepted transactionally consistent SQLite receipt/status observation with deterministic concurrent-winner reuse and genuine missing-receipt rejection (receipts table owns identity) |
| Route-autopilot adversarial soak | `COMPLETE` | PR #426 worker plus PR #429 closeout: one real OpenCode-backed packet through existing PR/CI/review/merge/closeout owners, with trusted ledger receipts after GitHub verify |
| V2 provider-free viability preflight | `COMPLETE` | PR #437: operator_preflight ready=true without issue/admit/spend; unissued request sha256 `015c94e9d65a902f3aba5eae4f3da6cba6d534cc3c57af3a6faf89125663469a` |
| V2 four-cell run | `COMPLETE` | PR #441: 4/4 `controlled_failure`; no seal; no target-default-branch write |
| V2 four-cell closeout | `COMPLETE` | Disposition `CONTROLLED_FAILURE`; restricted-bundle sha256 `9b345faf744c14d67157856a512b39d90c6e03ff1081783c793b987d6f93bf82`; redacted-bundle sha256 `e2eafa226700061cb000b35dec776ef0b49417aa5faece0b065923b49ee83d3f` |
| Measurement-readiness estimands | `COMPLETE` | PR #444 accepted the source-bound task-level value, gate, margin, repetition, and paired-bootstrap contract |
| `PE7-RWE-MR-OPERATIONS-EVIDENCE-1` | `COMPLETE` | PR #446 accepted a provider-free field-owner and explicit-unavailable manifest (receipts table owns identity) |
| `PE7-RWE-MR-PROTOCOL-FREEZE-1` | `COMPLETE` | PR #447 accepted the frozen measurement protocol; manifest digest binds via its receipts-table identity and frozen contract owner |
| `PE7-RWE-DB-SNAPSHOT-CORPUS-1` | `COMPLETE` | PR #448 recorded the original unavailable snapshot boundary; PR #451 accepted the reconstructable replacement without Provider or target effects |
| `PE7-RWE-DB-SNAPSHOT-RECONSTRUCT-1` | `COMPLETE` | PR #451 accepted a hash-bound provider-free reconstruction manifest; preflight remains provider-free and unissued |
| Provider-free RWE DB preflight | `COMPLETE` | Real same-tenant Store-owned Golden Path prerequisite completed; `rwe-live-baseline preflight` returned `rwe_operator_preflight.v1` `ready=true` with zero blockers, no authority consumption, Provider call, or target write; projection sha256 `b8c35d4060d98598ce3e3bc3977a84d125b1df09ff66b2b9f6d9aa4303c03954` |
| Measurement readiness | `BLOCKED_PREREQUISITE` | Estimands, finite corpus/sampling, operations/evidence, protocol freeze, and reconstructable snapshot are accepted; the observed DB RUN is retained as a non-baseline controlled failure |
| Decision-grade pre-AC baseline | `DEFERRED` | The failed DB RUN and its analysis are parked and are not an AC prerequisite; no decision-grade baseline is claimed |
| AC0 runtime inventory and AC1 ProcessSupervisor | `DEFERRED` | Optional hardening; existing runtime-specific owners retain timeout, cancellation, kill/reap, failure, and outcome-unknown boundaries |
| AC0 data/trace freeze | `COMPLETE` | The bounded provider-free owner/caller/transaction/projection/legacy inventory and trace/order closeout are recorded below; no ownership move was made |
| AC2 typed execution contract | `COMPLETE` | Provider-free typed state/outcome/usage contract, executor mappings, and the accepted fail-closed boundary repair are recorded below; PRs #469, #472, and #475 are merge-backed. The contract authorizes no wire/schema change, authority move, or shared `ProcessSupervisor` |
| AC2 typed boundary and caller migration | `COMPLETE` | Boundary repair and the enumerated ProductTask verification/managed-review caller migration are accepted by PRs #475 and #478; callers use the canonical typed mapping, unknown evidence remains fail-closed, and AC1 shared supervision remains deferred optional hardening |
| AC3 Golden Path responsibility contract | `COMPLETE` | PR #486 accepted; contract frozen |
| AC3 Golden Path orchestrator core | `COMPLETE` | PR #533 accepted; pure orchestration and golden-trace proof are merge-backed |
| AC3 port migration | `COMPLETE` | PR #535 accepted; canonical store/effect-port callers are merge-backed |
| AC4–AC5 | `COMPLETE` | AC4 contract/views/caller migration and AC5 contract/root-core/module migration are accepted by PRs #538, #540, #542, #544, #546, and #548 |
| AC6 schema convergence | `COMPLETE` | PRs #550, #552, #554, #556, and #558 accepted; zero drift is verified across the Rust producer, SDKs, and Dashboard, and the AC7 removal manifest is accepted for deletion-only cleanup |
| AC7 removal manifest | `COMPLETE` | PR #560 accepted; exact deprecated route, handler, LocalProductStore compatibility methods, consumer wrappers, test callers, rollback groups, and negative-search gates are frozen |
| AC7 cleanup | `COMPLETE` | PR #562 accepted; fixed-string inventory is zero across tracked source, tests, SDK, Dashboard, scripts, tools, and replay/fixture paths; separate approve/output authority and recovery semantics remain; rollback is the pre-cleanup tree `eb692703…` |
| AC7 closeout | `COMPLETE` | PR #563 accepted; exact cleanup convergence, implementation-cost aggregation, rollback index, and contemporary old/new replay inputs are bound |
| Contemporary old/new replay reconstruction | `COMPLETE` | PR #566 accepted; reconstruction verifier, frozen bindings, provider-free traces, and exact old/new identities are accepted |
| Contemporary old/new replay | `DECISION_REQUIRED` | Protocol/preflight freeze is complete; captured CLI is not `ready=true`; `PE7-RWE-CR-RUN-1` is parked on a pre-tenant empty Store |
| Contemporary old/new replay analysis | `BLOCKED_PREREQUISITE` | One independent RWE closeout remains behind the parked run; it neither blocks nor substitutes for the Harness-Evolution spiral |
| Harness-Evolution C0–C4 capability spiral | `READY_FOR_EXECUTION` (C1 CORE window) | EC3, the executed-and-closed C0 pilot/closeout, and the C1 three-axis contract are accepted on `main`; the current provider-free window `PE7-HE-MX1-CORE-1` implements the shared run seam, exact arm manifest, Strategy adapters, deterministic matrix planning, and `INCOMPARABLE` projections before any C1 variant effect; the 28 formal C0–C4 packets span accepted evidence and routing-only successors through one generation, bounded recursion, final sealed transfer, readiness, and human adoption decision |
| Experimental C5–C8 research spiral | `BLOCKED_PREREQUISITE` | 28 formal default-off packets retain fixed-Meta, advanced gate/R4, Harness-plus-weight R5, and bounded outer-policy R6 contracts, effects, independent closeouts, and replications; negative or insufficient dispositions remain valid terminals |
| Dashboard #225 / successor | `DEFERRED` | 3 formal packets: disposition, presentation-only refresh, exact-head closeout; it projects accepted core and optional-research dispositions without owning backend behavior |

## C1 three-axis experiment contract (provider-free contract freeze)

Frozen by `PE7-HE-MX1-CONTRACT-1`. This contract defines the identity,
admission, comparability, allocation, and analysis boundary for the Harness x
Model x Strategy experiment matrix. It creates no runtime, scheduler, Store,
evaluator, budget, admission, audit, rollback, or adoption owner; performs no
candidate generation, Provider call, holdout access, target write, or live
effect; and does not start `PE7-HE-MX1-CORE-1`.

### Arm-zero identity and provenance

The C0 closeout disposition `READY_FOR_BOUNDED_LOCAL_USE` freezes the semantic
arm-zero family as the engine-managed Harness x resolved model
`deepseek-v4-pro` x single-pass plan/implement/review Strategy. The observed C0
chain is version-composite: the finite effect authority binds accepted main
`9325d5e996d82a157c36c9220bec28c0c0bad5a6`, the terminal-owner repair is merge
`ed691c6d7666b01da1b020190233457ac320491b`, and the independent closeout is
merge `075f995b574fb8a28f08986291751152bf158dd5`. No claim is made that
`run-0002` executed the later `075f995b...` tree. Observed provenance is task
`ptask-20260824115348-18cebba70936f600`, run `run-0002`, delegation
`cl0-delegation-overlay-20260824`, patch artifact `patch-artifact-0001` against
source `6240768506320a324d68787b9eaa86971c8c930c`, and terminal receipt
`0b85bac66a61a8565bd7be238471c0551cd607f3c801dd1c785af2c232e51f25`.
That single task is usability evidence, not an estimated treatment effect.

| Axis | Arm-zero descriptor | Frozen meaning |
|---|---|---|
| `HarnessImplementation` | `engine-managed@075f995b574fb8a28f08986291751152bf158dd5` | Exact C1 comparison implementation chosen by this contract: this repository's Rust engine and existing Product Golden Path/managed-acceptance owners at the accepted closeout tree, with CWS default-off. This is a preregistered implementation identity, not a claim that C0 ran that tree; CORE must prove the shared interface provider-free and PILOT preflight must bind it before any effect. |
| `ModelPlan` | `deepseek-v4-pro:single-model-three-role:v1` | Exact requested/resolved id `deepseek-v4-pro` for ordered planning, implementation, and review through OpenAI-compatible Chat Completions at `https://api.deepseek.com/chat/completions`; symbolic parent-held credential reference `DEEPSEEK_API_KEY`; no credential value enters the descriptor. No alias fallback, endpoint/protocol drift, panel, fusion, or role-specific substitution. |
| `StrategyPlan` | `single-pass-plan-implement-review:no-projection:v1` | One ordered plan/implement/review pass with no memory or skill projection, no hidden rescue, and no post-observation prompt or retry-policy change. Its implementation source is bound to the Harness commit above. |

The accepted closeout owns the semantic family; this contract owns the exact C1
comparison descriptor. A later accepted security or correctness repair does not
silently mutate arm zero: the preflight must bind the exact frozen implementation
in confinement or register the repaired implementation as a distinct descriptor
and mark cross-version contrasts `INCOMPARABLE` unless the compatibility
contract proves equivalence. Failure of the `075f995b...` implementation to pass
the provider-free shared-interface suite stops C1; C0 evidence is not used to
waive that gate.

### Descriptor schema

Every scheduled, rejected, skipped, failed, or incomparable cell binds exactly
one descriptor from each axis. A CLI display name is transport metadata, never
Harness identity.

| Descriptor | Required fields before admission |
|---|---|
| `HarnessImplementation` | Stable descriptor id and schema version; repository/source owner; exact 40-hex commit or immutable package digest and version; build/executable identity and probe; shared run-seam version; supported task/tool/workspace capabilities; process and workspace confinement; terminal-outcome and verified-deliverable mapping; usage/cost and missingness mapping; cancellation, cleanup, restart, retry, failure, and outcome-unknown behavior; license identity; SBOM digest; provenance/audit digest; supported ModelPlan/StrategyPlan cells; default-off and rollback binding. |
| `ModelPlan` | Stable descriptor id and schema version; exact requested and resolved model ids/version; provider, protocol, exact endpoint and endpoint-allowlist identity; immutable admitted-profile digest; parent-held credential class and exact symbolic credential reference (name only, never the secret value); ordered role assignment and routing topology; generation/retry parameters; context/output limits; tokenizer and usage fields; pricing currency, unit basis, source, and effective date; lifecycle-cost mapping; supported Harness/Strategy cells; missing-identity and missing-usage disposition. |
| `StrategyPlan` | Stable descriptor id and schema version; strategy type and composition order; exact source identity; for every projection, `GIT_BLOB` or `ARTIFACT_REF` owner handle plus `content_sha256`; admitted input classes and redaction; expiry; deterministic deletion/rebuild recipe; cross-task and cross-arm isolation; leakage scan; prompt/tool/retry/compression policy; no-authority declaration; supported Harness/Model cells. Raw prompts, outputs, transcripts, credentials, and private paths are not descriptor fields. |

Before admission, a candidate Harness receives exactly one recorded disposition:
`EMBED_BEHIND_SEAM`, `CONFINED_SUBPROCESS`, `POLICY_BLOCKED`,
`CAPABILITY_BLOCKED`, `INCOMPARABLE`, or `REJECT`. Only the first two can admit
a cell, and only after exact commit/version, license, SBOM, provenance, binary,
confinement, cleanup, and shared-interface evidence pass. The disposition does
not transfer Rust/Store/evaluator/budget/output authority to the candidate.

### Admission and common comparison basis

- A cell is admitted only when all three descriptors are complete, immutable,
  mutually supported, default-off, and joined to the exact task, corpus,
  evaluator, verification, total-lifecycle budget, value basis, environment,
  seed, source, capsule, and head identities.
- Arms may differ only in the registered factor for the contrast. Provider,
  evaluator, task bytes, verification, budget, value weights, environment,
  rescue policy, and evidence completeness stay common. Any other difference is
  contamination and stops the affected rung.
- Existing binary, credential, workspace/process confinement, ProductTask,
  spend, verification, approval/output, audit, recovery, redaction, and
  exact-once terminal guards remain mandatory. Admission cannot waive them or
  create a second owner.
- The value basis is verified delivered work first: authority/safety,
  trustworthy verification, terminal completeness, and the frozen
  non-inferiority gate below precede the Pareto vector of lifecycle tokens, money,
  elapsed time, review/repair/CI/recovery/human burden, retries, and unavailable
  values. No scalar efficiency score can override a failed hard gate.
- Candidate generation, task selection, evaluator thresholds, budget limits,
  and allocation are frozen before any outcome in the corresponding rung is
  observed. Labels and sealed evaluator inputs remain inaccessible to arms.

The common C1 task/evaluator/budget/value basis reuses the accepted
`measurement_estimands.v1` lineage with two distinct immutable source
identities: pre-AC Harness artifact-freeze SHA
`ee43eac853644266614da09de764a3bf19f2d281`, and PR #370 merge
`3b4afb3e5ab4254904aa5a63473ab6ae0eac1e82`, the first accepted repository tree
containing the v2 protocol, schedule, and tasks. The latter is a carrier/source
receipt and does not replace the former artifact identity. The bound corpus hash is
`044fcd7bf4c35c6a4798f60b5b87d79d8549b45351f4e350b397a63a0fe2ce20`,
and source tree `137e912f416a3a8d5be307e91bb2580154fc8fc34c6de52c2441ef3e3f93a064`:

| Basis field | Frozen C1 value |
|---|---|
| Tasks | `rwe-minimum-t1-fix_flow_linkage` (`task_definition_sha256=fcd13b6f7a970c048fd09e1f723a315b8e03d221cad1555bf694ca95115438f8`) and `rwe-minimum-t2-draft_contract_tests` (`task_definition_sha256=f49e374d8b818d9e2cf4566d6fb3323c472a3dd449ebc71413eab96891124e7d`), both at source commit `6240768506320a324d68787b9eaa86971c8c930c`; no selective replacement. |
| Evaluator/verification | Existing task acceptance rubrics and machine commands from that corpus; reviewer rubric sha256 `0e3c4275aacae5ae1eec563ea348135fa05b6719391c526490a7503b497c4e7b`; disagreement is recorded and fails closed. |
| Budget | C1 `bp-standard`: per cell `max_cost_usd=0.2` (operator ceiling, not provider quote), `max_provider_requests=3`, `max_input_tokens=12000`, `max_output_tokens=8192`, `max_total_tokens=20192`, `max_retries=0`, and `max_wall_time_ms=900000`, plus the accepted complete lifecycle-cost envelope; unavailable cost stays unavailable. |
| Value | `verified_delivery_points` on the two frozen task value profiles; no new conversion, normalization, or scalar aggregation. |
| Repetitions | Exactly two per task/cell. The complete derived C1 schedule digest is committed before the first outcome and cannot change afterward. |

The source v2 schedule hash
`6a729f1213384d2306091ce5f258c9ddd08fe569374167c04e7f10c930cb1b38`
is provenance only and is not reused because its cells freeze a different Model
layout. C1 instead freezes the cell set and deterministic schedule/seed
derivation in this contract. With two tasks and two repetitions, the per-rung
global ceilings are: `1x2x1` = 8 cells, USD 1.6, 24 Provider requests, 161,536
tokens, and 7,200,000 ms sequential cell time; `1x2x3` = 24 cells, USD 4.8, 72
requests, 484,608 tokens, and 21,600,000 ms; `2x2x3` = 48 cells, USD 9.6, 144
requests, 969,216 tokens, and 43,200,000 ms. These are finite ceilings, not spend
authority; every effect still requires its separate exact T3 request and one-use
Store-owned budget. CORE/PILOT preflight must publish the resulting exact
descriptor and schedule digests before execution. A missing or mismatched digest
is `INCOMPARABLE`, not authority to infer a schedule or spend.

Nonzero labels `H1`, `M1`, `SM`, and `SK` below are registered estimand slots,
not admitted arm identities. `PE7-HE-MX1-CORE-1` must, as a provider-free exit
artifact, bind each slot to one complete exact descriptor (including H1
commit/version/license/SBOM/provenance and every Model/Strategy identity field),
pass the common interface/isolation tests, and freeze a descriptor-manifest
digest. Only a later PILOT preflight may combine that accepted manifest with the
schedule digest and request finite T3 authority. No Provider outcome, pilot
authorization, allocation, or cell execution may exist while any slot is
unresolved, unsupported, changed, or merely a CLI/display name.

### Comparability and `INCOMPARABLE`

`INCOMPARABLE` is a first-class terminal disposition, not missing success and
not a reason to coerce a rank. It is emitted for unsupported cross-product
cells; absent, stale, ambiguous, or drifting identity; unregistered-factor
variance; unequal task/evaluator/verification/budget/value basis; hidden rescue;
unresolvable contamination; outcome unknown; or evidence missingness that
prevents the registered contrast. Rejected and skipped cells remain in the cell
ledger with their exact reason.

No imputation turns `INCOMPARABLE`, unavailable usage/cost, a failed hard gate,
or an unknown effect into a numeric outcome. A contrast is reported only over
the explicitly supported common cells. Marginal and interaction estimates must
name that support; they never extrapolate to unsupported Harness, Model,
Strategy, task, or environment regions. If one cell needed by a registered
contrast is incomparable, that contrast is `INCOMPARABLE`; other independently
supported contrasts may still be reported without advancing the rung.

Engine-owned `OutcomeUnknown` remains the durable effect/lifecycle state until
its existing reconciliation owner resolves it. The analysis ledger may project
the affected cell/contrast as `INCOMPARABLE`, but never replaces, clears,
retries, or downgrades the underlying `OutcomeUnknown` record.

### Staged ladder, allocation, and stop rules

The minimum matrix advances in this fixed order:

1. `1x2x1`: arm-zero Harness, arm-zero Strategy, and exactly two registered
   ModelPlans. This tests Model comparability first.
2. `1x2x3`: the same Harness and ModelPlans, with baseline/no-projection,
   memory-only, and skill-only StrategyPlans. This tests projection identity,
   deletion/rebuild, leakage, and isolation before another Harness is added.
3. `2x2x3`: the same two ModelPlans and three StrategyPlans, with arm zero and
   exactly one admitted second Harness. The second Harness is introduced last.

Within each task-and-repetition block, all admitted cells for the rung are
scheduled as a complete block; this is fixed complete-block allocation, not
random allocation. Execution order inside that block is the ascending SHA-256
hex key over the five fields `PE7-HE-MX1-CONTRACT-1/randomization:v1`, rung,
task id, base-10 repetition index, and cell-descriptor digest. Each field is
UTF-8 encoded and prefixed, in that order, by its decimal byte length and `:`;
there is no separator or trailing byte. Ties break by the full cell identity.
The task list and repetition count are frozen by the later preflight before the
first outcome, and no outcome-adaptive allocation, cell replacement, selective
rerun, or seed substitution is permitted. This deterministic serialization and
hash are the seed contract; preflight records every derived key and complete
ordered block before execution.

Hard gates are analyzed before any efficiency value. A rung stops and later
rungs remain unauthorized on authority/identity drift, budget exhaustion or
reservation mismatch, hidden or unequal rescue, target/output boundary breach,
unknown effect, cleanup/recovery uncertainty, evaluator/verification drift,
cross-arm leakage, incomplete complete-block evidence, any required cell or
registered contrast becoming `INCOMPARABLE`, or a delivery/safety/non-inferiority
gate failure. Early stopping is for safety, authority, feasibility, and frozen
hard gates only; it never uses a favorable interim efficiency estimate. All
observed and missing terminals remain recorded through existing evidence owners.
If a stop truncates any frozen task/cell/repetition block, completed observations
may be shown only as `POINT_ESTIMATE_ONLY` diagnostics. They cannot produce a
paired-bootstrap interval, non-inferiority PASS, rung advancement, Pareto
selection, or inferential main/interaction result.

### Frozen estimands and analysis

- The task x repetition block is the execution-order and pairing unit, not an
  independent inferential observation. For each cell, all frozen repetitions
  within one task are first aggregated by arithmetic mean into one task-level
  value. Fewer than the frozen repetition count makes that task value
  unavailable. Main and interaction contrasts pair these task-level values and
  marginalize them with equal task weight. Bootstrap resamples tasks only while
  retaining every nested repetition and every paired cell for the selected
  task; it never resamples task x repetition rows. Cell outcomes are joined only
  by their exact three-axis and common-basis identities.
- Let `H0`/`H1` be arm-zero/second Harness, `M0`/`M1` be arm-zero/second
  ModelPlan, and `S0`/`SM`/`SK` be no-projection/memory-only/skill-only.
  Harness main effect is `H1-H0`; Model main effect is `M1-M0`; Strategy has
  exactly two primary contrasts, `SM-S0` and `SK-S0`, plus secondary descriptive
  `SK-SM`. Each is a paired task-level contrast marginalized with equal weight
  only across the balanced, mutually supported cells of the other axes in that
  rung. No observed-frequency or cost-weighted marginalization is allowed.
- `Harness x Model` is the paired difference-in-differences
  `(H1-H0 at M1) - (H1-H0 at M0)`. `Harness x Strategy` and `Model x Strategy`
  each have two registered interactions, one substituting `SM-S0` and one
  substituting `SK-S0` as the Strategy contrast. The three-axis interaction has
  those same two forms: the difference in the `Harness x Model` interaction at
  `SM` versus `S0`, and at `SK` versus `S0`. They receive intervals but are not
  hard-gate or adoption estimands. `SK-SM` interactions are descriptive only.
  No unsupported interaction is estimated.
- Cell viability reuses accepted `measurement_estimands.v1`: protocol
  `rwe-minimum-first-protocol-v2`, protocol hash
  `bc68bfb320f891ee5490019385c17d71ee7bfc725bb43cd0c006d33c5d5d35db`,
  and paired-bootstrap method `paired-bootstrap-95` with method hash
  `0942b62fb4b864332bef8fa95d149cc59718d13428a120f3559672f8f00b6c63`.
  Against the matched arm-zero task/repetition baseline, the lower 95% paired
  bound for verified delivery, machine-verification acceptance, and reviewer
  acceptance must each be at least `-0.10`; the upper 95% bound for recovery
  failure must be at most `+0.05`. Minimum registered repetitions per task is
  two. All four are an intersection-union gate: every gate passes at level
  `0.05`, so no multiplicity adjustment or favorable-gate substitution is
  permitted. Axis/interaction intervals are evidence only and make no separate
  significance or global-superiority claim.
- A partial cell block, fewer than the frozen repetitions, or unavailable
  required evidence makes the affected registered task block unavailable; it
  is not removed from the denominator or replaced. Every estimand requiring
  that block becomes `INCOMPARABLE`, the rung cannot advance, and independently
  supported contrasts may be shown only as explicitly incomplete diagnostics.
- Conditional on every hard gate passing, the registered lifecycle-cost vector
  and hard-gate-first Pareto frontier are reported with ties, dominance,
  uncertainty, and explicit unavailable dimensions. There is no scalar winner
  or global-best claim.
- Uncertainty uses the accepted paired task-level bootstrap owner with frozen
  task blocks as resampling units. Its seed key hashes the length-prefixed UTF-8
  fields `PE7-HE-MX1-CONTRACT-1/bootstrap:v1`, rung, contrast id, and base-10
  replicate index using the same canonical encoding defined for randomization.
  Intervals are not a substitute for hard gates; no post-hoc threshold,
  multiplicity rule, support change, or estimand change is allowed.
- The closeout publishes from the same cell rows: the cell ledger, supported
  per-axis marginal contrasts, supported interactions, hard-gate-first Pareto
  frontier, and missing/`INCOMPARABLE` coverage map. Negative, insufficient,
  and incomparable results are valid completion evidence and may terminate or
  rewrite later routing; they are never rewritten as improvement.

Rollback is a revert of this documentation contract. It creates no runtime or
durable state, and no later packet may weaken it without a separately accepted
contract change.

## AC0 bounded data and trace inventory

Existing owners and representative paths for intake, runtime orchestration, persistence, provider/credential, wire/codegen/SDK/Dashboard, configuration, and legacy callers are owned by `docs/MODULE_MAP.md` and the Architecture Book Product Golden Path contract; this inventory creates no new owner. Compatibility obligations (reject-before-admission, CAS/idempotency/restart semantics, parent-held credentials, `scripts/check_wire_codegen_drift.sh` drift gate) are restatements of those owners' invariants.

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


The accepted route stabilizes the minimum path without a new runtime owner. The dependency order is frozen as `AC0 inventory → AC0 trace/order closeout → AC2 contract → AC2 typed boundary → AC2 caller migration → AC3 → AC4 → AC5 → AC6 → AC7`. AC1 shared `ProcessSupervisor` remains deferred optional hardening and is not a prerequisite. The accepted AC2 contract starts from `engine/src/node_executor.rs::ProcessOutcome`, `engine/src/executor_adapter.rs::ExecutionResult`, `engine/src/provider/executor.rs`, `engine/src/provider/managed_deepseek_executor.rs`, `engine/src/execution_usage/reconcile.rs`, and the existing persistence owners `engine/src/storage/local_product_store/product_tasks.rs` and `engine/src/storage/local_product_store/workflow_runs.rs`; the boundary-core packet may add only typed mappings and focused tests. It must not move admission, lease, spend, verification, approval, output, audit, recovery, or target authority. Within the enumerated Golden Path and legacy/advisory caller closure, no unknown production caller was found; any caller outside that closure or any cross-backend parity requirement stops the next packet rather than widening scope.

## AC2 typed execution contract (provider-free contract freeze)

This is the closed contract for the next additive AC2 boundary work. It names the existing owners and their exact variants; it does not add a Rust type, wire field, schema migration, public projection, scheduler, store, or second authority. The ProductTask lifecycle remains authoritative in `LocalProductStore`; executor and usage records are evidence inputs to that owner, not competing state machines.

### Closed state, outcome, and evidence vocabulary

| Contract axis | Closed variants and existing representation | Authority and fail-closed rule |
|---|---|---|
| Admission | `rejected_before_admission` (intake validation fails before `ProductTaskStatus::Admitted`); `admitted` (`Admitted` with the existing reservation) | `validate_intake` and `admit_product_task`/`reserve_product_task` remain the only admission path. Rejection has no execution effect and is not a retryable execution outcome. |
| Preparation | `preparing` (`WorkspacePreparing`); `prepared` (`WorkspaceBound`) | `LocalProductStore` owns the receipt, target/workspace binding, CAS, and restart-safe compensation. A missing or mismatched receipt blocks advancement. |
| Effect boundary | `effect_not_started` (`GraphReady`, whose run exists but has no leased node); `effect_started` (`Running`, `Verifying`, `RepairPending`, `AwaitingApproval`, or `OutputPending`) | Scheduler leases, node attempts, verification, approval, and output remain under their current owners. A pre-send refusal is not evidence that an effect started; a request that may have been sent cannot be replayed speculatively. |
| Pause and cancellation | `paused` (`Paused`, resumable under the existing transition table); `cancelled` (`Killed`, terminal) | Cancellation/kill stays executor, scheduler, and store-owned. No caller may turn a pause or kill into success or an automatic retry. |
| Known outcome | `known_success` is `ProcessOutcome::exited(0)` plus the existing verification/terminal owner for OS-process executors, or an explicit completed provider/managed response with accepted executor evidence; `known_failure` is any definitive failure classification carried by the existing `ProcessOutcome`/adapter evidence owners (`engine/src/node_executor.rs`): non-zero exit, signal, spawn/read/wait/timeout failures, output-limit or process-tree-containment failures, invalid limits, or a definitive pre-send adapter refusal | `ProcessOutcome` describes the executor/process observation and is optional for provider/managed responses. `spawn_failed`, `process_tree_containment_unavailable`, `process_tree_containment_unsupported`, and `invalid_output_limits` are effect-not-started; the other listed process failures are effect-started unless the executor's pre-child evidence proves otherwise. A provider response is not forced through an OS-process variant. `signaled` with an unavailable signal remains a known termination with incomplete detail; `unavailable` is incomplete process evidence, never success. `ProductTaskStatus::Completed` is the only canonical product success. |
| Terminal failure | `terminal_failure` is the store-owned `ProductTaskStatus::{Failed,BudgetExhausted,Blocked}` set; `Killed` is represented only by the separate `cancelled` variant | Terminal failure requires the existing run/node evidence, CAS, audit, and compensation path. It is non-success and never an automatic retry authorization. |
| Unknown outcome | `unknown` is `ProductTaskStatus::OutcomeUnknown` or an executor/provider result explicitly classified as outcome-unknown after a possible send | Unknown is not success, not complete evidence, and not retry authorization. Existing reconciliation/compensation must determine the next terminal or output state; automatic replay is forbidden. |
| Usage completeness | `complete`, `partial`, `ambiguous`, `conflicting` are exactly `EventCompleteness::{Complete,Partial,Ambiguous,Conflicting}` | `Complete` plus conflict-free reconciliation may support `known_success` when the execution and verification owners also prove success. `Partial` or `Ambiguous` is incomplete evidence: a definitive execution failure remains `known_failure`, but an otherwise-successful or effect-possible execution remains `unknown` until reconciliation proves the missing usage/cost fields; neither authorizes retry. `ReconcileResult.conflicts` and `EventCompleteness::Conflicting` make `admission_evidence_ok` fail closed. Missing cost is represented by `CostSource::Unavailable`, not fabricated zero cost. |

`ProductTaskStatus` is the canonical lifecycle vocabulary: `Admitted`, `WorkspacePreparing`, `WorkspaceBound`, `GraphReady`, `Running`, `Verifying`, `RepairPending`, `AwaitingApproval`, `OutputPending`, `Completed`, `Failed`, `Killed`, `Paused`, `BudgetExhausted`, `Blocked`, and `OutcomeUnknown`. `Completed`, `Failed`, `Killed`, `BudgetExhausted`, `Blocked`, and `OutcomeUnknown` are terminal; only `GraphReady`, `Running`, `Verifying`, and `OutputPending` admit execution. The existing transition table remains the sole legal transition relation.

### Executor-specific mapping

| Existing executor/evidence owner | Mapping into the closed contract | Boundary that must remain unchanged |
|---|---|---|
| `CommandNodeExecutor` and other local process executors | `ExitStatus` maps through `ProcessOutcome::exited`/`signaled`; the existing failure-state set stays enumerated in `engine/src/node_executor.rs`. `ProcessOutcome::successful_exit()` is the narrow process-success predicate. | Effect-not-started vs effect-started classification follows the owner's recorded pre-child evidence; `unavailable` is incomplete evidence unless the owner records a pre-spawn boundary. Cleanup, timeout, kill/reap, and redaction stay executor-owned; a process outcome never terminalizes a ProductTask by itself. |

| `codex_cli` and `claude_code_cli` node paths | `NodeExecutionOutput` carries `status`, bounded/redacted output, optional usage, and `process_outcome`; absent process ownership is represented by `ProcessOutcome::unavailable(...)` | `NodeExecutionOutput::to_value` is a projection/evidence adapter. It cannot mint approval, output, spend, or target authority. |
| Provider and managed-provider paths, including `provider/executor.rs` and `provider/managed_deepseek_executor.rs` | Preserve the existing pre-send/definitive/unknown classification. A definitive pre-send refusal maps to known failure with effect-not-started; a possible-send unknown maps to `OutcomeUnknown`; a verified response maps to known executor evidence and then existing verification/store terminalization | Credentials remain parent-held. Provider request journaling, usage reconciliation, redaction, compensation, and no-retry-after-unknown stay with the existing provider/store owners. This packet makes no Provider call. |
| Adaptive provider adapter | Endpoint/config/plan/gate refusals are effect-not-started known failures; post-response identity/usage/cost rejections are effect-started known failures; admitted-but-unanswered calls stay `unknown` unless trusted pre-send refusal evidence exists; limit/quota outcomes are known failures only when every attempted call is definitive. The enforced variant set lives in `engine/src/node_executor.rs` and its tests. | Adaptive execution remains classification-only: no second scheduler, budget ledger, usage owner, or ProductTask authority; unknown calls never become retry authorization. |

| Legacy dispatch adapter | `executor_adapter.rs::ExecutionResult` keeps its `execution_result.v1` fields and legacy statuses as enumerated by the code owner. | Legacy projections never rebind to ProductTask lifecycle state nor authorize managed acceptance, approval, output, retry, or target effects. |


### Usage and recovery binding

`ExecutionUsageEventV1` is the normalized usage evidence shape. Its executor/source identity, disjoint token buckets, `CostSource::{Unavailable,Estimated,ProviderOrExecutorReported}`, stable dedupe identity, and `EventCompleteness` are preserved. `ReconcileResult` returns canonical events, suppressed duplicates, and explicit conflicts; only conflict-free evidence may pass `admission_evidence_ok`. Usage uncertainty never becomes a zero-cost success or a retry permission.

When an effect may have started, `ProductTaskStatus::OutcomeUnknown` is retained until the existing reconciliation owner proves a safe disposition. `fail_product_task_and_compensate` revalidates the workspace receipt and target boundary, performs the existing cleanup/reconciliation, and transitions through the store's CAS/audit path. No typed boundary may bypass that sequence, delete unknown evidence, or create an alternate recovery owner. Public API, SDK, Dashboard, wire/codegen, and schema compatibility remain unchanged until the later accepted AC2 caller/schema packets.

Focused evidence: the golden-path g1/g2/g3, recovery, and evidence integration tests plus execution-usage unit tests; boundary-mapping repair history is owned by merged PRs #472/#475/#478.

## AC2 caller migration closeout

`PE7-AC2-CALLER-MIGRATION-1` migrated the two accepted store callers to `ProcessBoundaryMapping::is_known_success()`; raw serialized `process_outcome.v1` evidence is preserved and malformed/unknown/not-started outcomes remain non-success (receipts table owns identity).

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

Executed in accepted order AC3-CONTRACT → ORCHESTRATOR-CORE → PORT-MIGRATION; later packets prove golden-trace equivalence before changing code.

### Forbidden ownership imports

`engine/src/product_golden_path.rs` must not import or call the store mutation commands above, provider adapters under `engine/src/provider/`, HTTP approval/output handlers, or target-output owners; handlers and callers remain adapters, and `engine/src/storage/local_product_store/product_tasks.rs` remains the sole ProductTask mutation owner. Effect owners must not mutate ProductTask lifecycle or mint authority. Any required ownership move is `DECISION_REQUIRED`.


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
4. The AC0 data/trace freeze is complete as a provider-free contract and closeout; AC1 shared ProcessSupervisor hardening remains deferred and is not an active implementation frontier. AC2–AC7 are accepted through the closeout merge `42fcfa5a`; `PE7-RWE-CR-RECONSTRUCTION-1` is accepted through `7cfa817a`; `PE7-RWE-CR-PROTOCOL-PREFLIGHT-REPAIR-1` is accepted through `262b67b6`; and `PE7-RWE-CR-PROTOCOL-PREFLIGHT-1` is accepted through `837ae2aa` / `9c25d193`. All replay effects remain gated. `PE7-RWE-CR-RUN-1` is `DECISION_REQUIRED`.
5. Accepted Harness-Evolution identity/lineage, causal-manifest, mutation-registry, evaluator/holdout, holdout-seal, sentinel-conformance, and evaluator-owned `PredictionOutcomeV1` boundaries are provider-free and non-authoritative. The accepted EC3 contract, instrumentation, and enforcement freeze lifecycle-cost ontology, trust, missingness, envelopes, accounting rules, reservation, and exact-once reconciliation; they remain provider-free/default-off and do not authorize a live effect. No accepted diversity/exploration or Pareto/stop/recovery implementation exists.
6. No Level-2 rule audit, controller contract, provider-free conformance, live pilot, final transfer, adoption decision, or fixed Meta operator-comparison result exists.
7. No accepted metacognitive-operator, parameter-efficient training adapter, weight/harness factorial, co-evolution, or outer-policy research contract exists; full-weight and model-architecture evolution remain unrouted.
8. The failed bootstrap from accepted main `aa83ac1f5eada74199e0ce28ecb91d37a48769d6` remains valid non-authorizing evidence: it stopped with `route_controller_unavailable_timeout` after GitHub rejected 28 workflow inputs with HTTP 422, before any workflow run, PR, claim, Provider call, target write, or external effect. PR #416 and accepted-main smoke `31631388199` removed that exact dispatch blocker. The route remains stopped until the one-time merge-backed bootstrap starts from current main; route10 remains non-resumable obsolete-main evidence.
9. The invalidated historical packets from `PE7-AC3-ORCHESTRATOR-CORE-1` through `PE7-HE-EC2-CONTRACT-1` (35 materially unfulfilled packets plus one separately invalid `PE7-HE-EC2-CONTRACT-1` receipt), along with earlier superseded route-automation receipts (`PE7-SUCCESSOR-PROMOTION-ESCALATION-1` and `PE7-ROUTE-AUTOMATION-1`), remain non-authorizing historical evidence. Accepted AC3–AC7, Contemporary RWE reconstruction, read-only preflight repair, and protocol/preflight freeze receipts are listed above. `PE7-RWE-CR-RUN-1` is parked `DECISION_REQUIRED`. Every downstream packet must be executed sequentially with genuine evidence, exact-head reviews, and canonical CI before promotion.
10. `PE7-RWE-CR-PROTOCOL-PREFLIGHT-1` is complete on `837ae2aa` / `9c25d193`. A real `rwe-live-baseline preflight` against existing `.agent-control-plane/local-team.db` opened read-only then failed closed at principal auth (`no such column: tenant_id`; zero keys/tasks). `ready=true` is not claimed. `PE7-RWE-CR-RUN-1` is `DECISION_REQUIRED`. No Store was created, migrated, or seeded.

## Maintenance Boundary

After each accepted merge, update this file only when an accepted capability or confirmed gap changed. Update `docs/NEXT_DECISION.md` only when the current executable window changed, and update `docs/FUTURE_ROUTE.md` only when long-horizon order or a routing-only sketch changed. Never copy live PR, CI, or review state into any of those documents.

## Safety Boundary

Default-off execution; no provider call in CI; no target-default-branch write; no auto-merge; no release or deployment authority; no reusable credential in a child; no secret, raw prompt, raw output, transcript, private path, fixture-only result, forecast value, memory projection, novelty score, or scalar VDE index may become production-adoption authority.
