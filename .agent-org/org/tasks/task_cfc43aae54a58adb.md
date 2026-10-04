# agent-org task

```yaml
schema_version: 2
task_id: task_cfc43aae54a58adb
employee_id: emp_project_owner
project_id: cc-switch-aggregate-evaluation
assignment: PROJECT/cc-switch-aggregate-evaluation
workflow: agent-org-workflow@1
stage: delivered
risk: high
fault_domain: local-only
allowed_models:
- gpt-6.1-sol
roles:
  implementer:
    kind: llm
    actor_id: cc-switch-route-compatibility-implementer
    executor: project-llm
    model: gpt-6.1-sol
    provider: codex-desktop
    effort: high
  reviewer_b:
    kind: llm
    actor_id: cc-switch-route-compatibility-reviewer
    executor: project-llm
    model: glm-4.7
    provider: hermes-cli
    effort: high
required_review: single
scope: aggregateRoutes utilities/types/form UI, Claude aggregate serialized retirement
  data and validation/resolution, related focused tests and reports; preserve first
  account fix and user work
forbidden: No real credential/config/DB reads or live model calls; no global/provider
  settings change or commit/merge/publish; do not modify frozen/accepted account fix
  or discard user changes
budget_hint: M
acceptance_criteria:
- Reorder/insert/edit preserves existing legal route IDs, upstream mapping, defaults
  and aliases; deleted IDs cannot be silently reused
- All existing same-provider/same-tier models remain visible and individually editable/removable
  without dropping others
- Retirement metadata persists across save/reload, legacy absent metadata loads, and
  requests for retired IDs fail explicitly before aliases/fallback
- Actual RED-to-GREEN and focused utility/component/Rust regressions, typecheck and
  real component visual verification; frozen independent review before delivery
assignment_snapshot:
  project_id: cc-switch-aggregate-evaluation
  effective_from: '2026-10-04'
  effective_to: null
  employee_id: emp_project_owner
  checked_at: '2026-10-04'
evidence:
  subject:
    kind: artifact
    project_id: cc-switch-aggregate-evaluation
    path: .agent-org/evidence/aggregate-route-compatibility-package-v2.zip
    sha256: 63e2bb93efd8a52f3c481cfef139976e43231e65c82551811d1076e3de59747e
  tested_revision: 63e2bb93efd8a52f3c481cfef139976e43231e65c82551811d1076e3de59747e
  test_report:
    project_id: cc-switch-aggregate-evaluation
    path: .agent-org/evidence/route-compatibility-test-report.json
    sha256: 81b705125ac276fb0096696c80edbe82022325c14126dcf2bc7b2290a85b09ed
  test_result: passed
  implementation_run: 01a09561-490b-7f22-8575-5ec743c800e9
  test_summary:
    total: 182
    passed: 182
    failed: 0
    skipped: 0
    commands:
    - pnpm exec vitest run seven affected files
    - cargo test --offline --lib aggregate -- --test-threads=1
    - pnpm exec tsc --noEmit
    - cargo check --offline --lib
    limitations:
    - Source/unit/component and isolated UI fixture acceptance only; no Tauri save
      or real upstream
    - Not full ~3000 backend or entire frontend suite
    - Some inherited base Japanese aggregate translations remain absent; new migration
      controls rendered in en/ja
    - Retirement observability and original tooltip remain nonblocking suggestions
    - V1 C0M0Minor7 and v2 C0M0Minor4 preserved; no billing/matched ROI proof
  reviewed_revision: 63e2bb93efd8a52f3c481cfef139976e43231e65c82551811d1076e3de59747e
  review_report:
    project_id: cc-switch-aggregate-evaluation
    path: .agent-org/evidence/route-compatibility-independent-review-final.md
    sha256: 0eead60be7f5356972cf81ef9ce6de89341c16d3fc46a703e2039cb67cedfd7b
  review_result: passed
  review_run: 20261004_202748_7004d2
execution_records:
  implementer:
    kind: llm
    executor: project-llm
    actor_id: cc-switch-route-compatibility-implementer
    model: gpt-6.1-sol
    provider: codex-desktop
    effort: high
    run_id: 01a09561-490b-7f22-8575-5ec743c800e9
    status: succeeded
    observation_source:
      kind: declared
      reference: Actual controller public turn_context and real source/RED/GREEN/browser/frozen
        evidence; no runtime identity attestation
  reviewer_b:
    kind: llm
    executor: project-llm
    actor_id: cc-switch-route-compatibility-reviewer
    model: glm-4.7
    provider: hermes-cli
    effort: high
    run_id: 20261004_202748_7004d2
    status: succeeded
    observation_source:
      kind: host-observed
      reference: Actual Hermes GLM-4.7 session20261004_202748_7004d2 initial review
        and v2 delta review CLIexit0; high requested; no tools in source-packet review;
        all raw outputs/v1 decisions retained; identity not attested
```
