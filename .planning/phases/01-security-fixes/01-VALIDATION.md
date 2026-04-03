---
phase: 1
slug: security-fixes
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-04-03
---

# Phase 1 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | pytest |
| **Config file** | pytest.ini (Wave 0 creates) |
| **Quick run command** | `pytest tests/security/ -x -v` |
| **Full suite command** | `pytest tests/ --cov=src --cov-report=term-missing` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `pytest tests/security/ -x -v`
- **After every plan wave:** Run `pytest tests/ --cov=src --cov-report=term-missing`
- **Before `/gsd:verify-work`:** Full suite must be green + bandit scan clean
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 1-01-01 | 01 | 1 | SEC-01 | unit | `pytest tests/security/test_command_execution.py::test_shell_injection_blocked -x` | ❌ W0 | ⬜ pending |
| 1-01-02 | 01 | 1 | SEC-01 | unit | `pytest tests/security/test_command_execution.py::test_whitelist_enforced -x` | ❌ W0 | ⬜ pending |
| 1-02-01 | 02 | 1 | SEC-02 | unit | `pytest tests/security/test_approval_tokens.py::test_token_signature_valid -x` | ❌ W0 | ⬜ pending |
| 1-02-02 | 02 | 1 | SEC-02 | unit | `pytest tests/security/test_approval_tokens.py::test_expired_token_rejected -x` | ❌ W0 | ⬜ pending |
| 1-02-03 | 02 | 1 | SEC-02 | unit | `pytest tests/security/test_approval_tokens.py::test_timing_attack_resistance -x` | ❌ W0 | ⬜ pending |
| 1-03-01 | 03 | 2 | SEC-03 | unit | `pytest tests/config/test_settings.py::test_env_config_loaded -x` | ❌ W0 | ⬜ pending |
| 1-03-02 | 03 | 2 | SEC-03 | integration | `pytest tests/tools/test_base.py::test_no_hardcoded_paths -x` | ❌ W0 | ⬜ pending |
| 1-04-01 | 04 | 2 | SEC-04 | unit | `pytest tests/security/test_ssrf.py::test_private_ip_blocked -x` | ❌ W0 | ⬜ pending |
| 1-04-02 | 04 | 2 | SEC-04 | unit | `pytest tests/security/test_ssrf.py::test_localhost_blocked -x` | ❌ W0 | ⬜ pending |
| 1-04-03 | 04 | 2 | SEC-04 | unit | `pytest tests/security/test_ssrf.py::test_valid_url_allowed -x` | ❌ W0 | ⬜ pending |
| 1-05-01 | 05 | 3 | SEC-05 | unit | `pytest tests/security/test_taint_engine.py::test_taint_propagation -x` | ❌ W0 | ⬜ pending |
| 1-05-02 | 05 | 3 | SEC-05 | unit | `pytest tests/security/test_taint_engine.py::test_high_risk_blocked -x` | ❌ W0 | ⬜ pending |
| 1-05-03 | 05 | 3 | SEC-05 | unit | `pytest tests/security/test_taint_engine.py::test_sanitization_upgrades_trust -x` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `tests/conftest.py` — shared fixtures (mock settings, token manager)
- [ ] `tests/security/__init__.py` — package init
- [ ] `tests/security/test_command_execution.py` — SEC-01 tests
- [ ] `tests/security/test_approval_tokens.py` — SEC-02 tests
- [ ] `tests/config/__init__.py` — package init
- [ ] `tests/config/test_settings.py` — SEC-03 tests
- [ ] `tests/security/test_ssrf.py` — SEC-04 tests
- [ ] `tests/security/test_taint_engine.py` — SEC-05 tests
- [ ] `pytest.ini` — pytest configuration
- [ ] `pip install pytest pytest-cov bandit` — if not in requirements

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Bandit SAST scan clean | All | Tool output review | Run `bandit -r src/ -ll` and verify no high-severity findings |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
