---
phase: 3
slug: human-in-the-loop
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-04-04
---

# Phase 3 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | pytest (existing from Phase 1) |
| **Config file** | tests/conftest.py (existing) |
| **Quick run command** | `pytest tests/hil/ -x -v` |
| **Full suite command** | `pytest tests/ --cov=src --cov-report=term-missing` |
| **Estimated runtime** | ~5 seconds |

---

## Sampling Rate

- **After every task commit:** Run `pytest tests/hil/ -x -v`
- **After every plan wave:** Run `pytest tests/ --cov=src`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 10 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 03-01-01 | 01 | 1 | HIL-01 | integration | `pytest tests/hil/test_approval_integration.py -x` | ❌ W0 | ⬜ pending |
| 03-02-01 | 02 | 1 | HIL-02 | unit | `pytest tests/hil/test_snapshot.py -x` | ❌ W0 | ⬜ pending |
| 03-03-01 | 03 | 2 | HIL-03 | unit | `pytest tests/hil/test_token_consumption.py -x` | ❌ W0 | ⬜ pending |
| 03-04-01 | 04 | 2 | HIL-04 | integration | `pytest tests/hil/test_resumption.py -x` | ❌ W0 | ⬜ pending |
| 03-05-01 | 05 | 3 | AGT-01 | integration | `pytest tests/agents/test_bingbu.py -x` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `tests/hil/__init__.py` — test package initialization
- [ ] `tests/hil/conftest.py` — shared fixtures (ApprovalRequest, mock tokens)
- [ ] `tests/hil/test_approval_integration.py` — covers HIL-01
- [ ] `tests/hil/test_snapshot.py` — covers HIL-02
- [ ] `tests/hil/test_token_consumption.py` — covers HIL-03
- [ ] `tests/hil/test_resumption.py` — covers HIL-04
- [ ] `tests/agents/test_bingbu.py` — covers AGT-01

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Approval dialog displays correctly | HIL-01 | Requires interactive CLI | Run CLI, trigger high-risk tool, verify dialog |
| Approval resumption after restart | HIL-04 | Requires process restart | Start approval, kill process, restart, verify resume |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 10s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
