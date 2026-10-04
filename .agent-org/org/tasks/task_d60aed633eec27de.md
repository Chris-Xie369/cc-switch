# agent-org task

```yaml
schema_version: 2
task_id: task_d60aed633eec27de
employee_id: emp_project_owner
project_id: cc-switch-aggregate-evaluation
assignment: PROJECT/cc-switch-aggregate-evaluation
workflow: agent-org-workflow@1
stage: delivered
risk: normal
fault_domain: local-only
allowed_models:
- gpt-6.1-sol
roles:
  implementer:
    kind: llm
    actor_id: cc-switch-aggregate-evaluator
    executor: project-llm
    model: gpt-6.1-sol
    provider: codex-desktop
    effort: high
  reviewer_b:
    kind: llm
    actor_id: cc-switch-aggregate-independent-reviewer
    executor: project-llm
    model: gpt-6.1-sol
    provider: codex-desktop
    effort: high
required_review: single
scope: Read current src/src-tauri/tests/designs and inspect isolated UI; write evaluation
  reports and governance/evidence only
forbidden: No real provider credentials/database inspection, changing live account/model
  settings, business source edits during assessment, merging/committing/publishing
  or overwriting pre-existing dirty work
budget_hint: M
acceptance_criteria:
- Current working source and uncommitted forwarder preserved and bound by hashes
- Architecture/usage/UI findings have concrete source or observed UI evidence and
  explicit uncertainty
- Actionable prioritized improvement plan with scope, validation and rollback
- Actual independent report review before delivery
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
    path: .agent-org/evidence/cc-switch-aggregate-evaluation-package.zip
    sha256: 33ede2ca15598f9cbd4170ce9a7de7e56fe9a279ee0afbefc3f4a504ae39ea4d
  tested_revision: 33ede2ca15598f9cbd4170ce9a7de7e56fe9a279ee0afbefc3f4a504ae39ea4d
  test_report:
    project_id: cc-switch-aggregate-evaluation
    path: .agent-org/evidence/evaluation-acceptance.json
  test_result: passed
  implementation_run: 01a09561-490b-7f22-8575-5ec743c800e9
  test_summary:
    total: 4
    passed: 4
    failed: 0
    skipped: 0
    commands:
    - 'Evaluation artifact acceptance: accept-evaluation.py (PowerShell; source and
      frozen evidence checks, no upstream request)'
    limitations:
    - Four checks are evaluation artifact acceptance; product has 107 passed and 3
      failed tests and six Major findings, not product acceptance
    - No native app/upstream/model invoice/performance acceptance
    - 46-test stdout not retained; controller observation is identified
    - Unisolated OAuth test incident preserved as redacted history; reproduction uses
      synthetic login only
  reviewed_revision: 33ede2ca15598f9cbd4170ce9a7de7e56fe9a279ee0afbefc3f4a504ae39ea4d
  review_report:
    project_id: cc-switch-aggregate-evaluation
    path: .agent-org/evidence/aggregate-independent-review.md
    sha256: 55afe4213b985147ce85f007c57cd7f016e433b58ac3cfef0a8ac7435a6cef3d
  review_result: passed
  review_run: /root/cc_switch_aggregate_review
execution_records:
  implementer:
    kind: llm
    executor: project-llm
    actor_id: cc-switch-aggregate-evaluator
    model: gpt-6.1-sol
    provider: codex-desktop
    effort: high
    run_id: 01a09561-490b-7f22-8575-5ec743c800e9
    status: succeeded
    observation_source:
      kind: declared
      reference: Actual Codex controller thread 01a09561-490b-7f22-8575-5ec743c800e9;
        model/effort from its public turn_context; provider label is host declaration,
        not provider authentication; actual tools and frozen reports recorded in v016
        cc-switch evidence
  reviewer_b:
    actor_id: cc-switch-aggregate-independent-reviewer
    executor: project-llm
    kind: llm
    run_id: /root/cc_switch_aggregate_review
    model: gpt-6.1-sol
    provider: codex-desktop
    effort: high
    status: succeeded
    observation_source:
      kind: declared
      reference: Actual independent Codex spawn invocation canonical task_name=/root/cc_switch_aggregate_review;
        controller reports spawn only returned this identifier, not a session UUID.
        Identity and model/provider/effort are registered controller host-dispatch
        declarations, not provider authentication. Actual checks are documented in
        aggregate-independent-review.md; tool chunks c9b89a, b687be, 2b51a5, 9cbb5d,
        ea31a6, 83864f, 238d0a, 7a5308 and image inspection in this independent task.
```
