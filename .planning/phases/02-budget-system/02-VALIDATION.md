---
phase: 2
slug: budget-system
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-04-04
---

# Phase 2 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | pytest (existing from Phase 1) |
| **Config file** | tests/conftest.py (existing) |
| **Quick run command** | `pytest tests/budget/ -x -v` |
| **Full suite command** | `pytest tests/ --cov=src/budget --cov-report=term-missing` |
| **Estimated runtime** | ~5 seconds |

---

## Sampling Rate

- **After every task commit:** Run `pytest tests/budget/ -x -v`
- **After every plan wave:** Run `pytest tests/ --cov=src/budget`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 10 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 02-01-01 | 01 | 1 | BUD-01 | unit | `pytest tests/budget/test_token_tracking.py -x` | ❌ W0 | ⬜ pending |
| 02-01-02 | 01 | 1 | BUD-01 | unit | `pytest tests/budget/test_token_tracking.py -x` | ❌ W0 | ⬜ pending |
| 02-02-01 | 02 | 1 | BUD-02 | unit | `pytest tests/budget/test_time_budget.py -x` | ❌ W0 | ⬜ pending |
| 02-03-01 | 03 | 2 | BUD-03 | unit | `pytest tests/budget/test_cost_tracking.py -x` | ❌ W0 | ⬜ pending |
| 02-04-01 | 04 | 2 | BUD-04 | integration | `pytest tests/budget/test_circuit_breaker.py -x` | ❌ W0 | ⬜ pending |
| 02-05-01 | 05 | 3 | BUD-05 | integration | `pytest tests/budget/test_concurrent_isolation.py -x` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `tests/budget/__init__.py` — test package initialization
- [ ] `tests/budget/conftest.py` — shared fixtures (BudgetManager, mock LLM responses)
- [ ] `tests/budget/test_token_tracking.py` — covers BUD-01
- [ ] `tests/budget/test_time_budget.py` — covers BUD-02
- [ ] `tests/budget/test_cost_tracking.py` — covers BUD-03
- [ ] `tests/budget/test_circuit_breaker.py` — covers BUD-04
- [ ] `tests/budget/test_concurrent_isolation.py` — covers BUD-05

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| None | - | - | All phase behaviors have automated verification |

*All phase behaviors have automated verification.*

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 10s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
