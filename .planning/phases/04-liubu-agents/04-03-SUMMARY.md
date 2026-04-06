---
phase: 04-liubu-agents
plan: 03
subsystem: xingbu-agent
tags: [agent, code-review, testing, agt-04]

provides:
  - Xingbu Agent for code review and testing
  - code_review module for linting and testing
requirements-completed: [AGT-04]

duration: 10min
completed: 2026-04-05
---

# Phase 4 Plan 03: Xingbu Agent Summary

**Xingbu (刑部) Agent for code review and testing**

## Performance

- **Duration:** 10 min
- **Files created:** 2

## Accomplishments
- Created src/tools/code_review.py with review_code, run_tests, check_coverage, run_security_scan
- Created src/agents/xingbu.py following Bingbu pattern
- 16 tests pass

## Files Created
- src/tools/code_review.py
- src/agents/xingbu.py
- tests/agents/test_xingbu.py (updated)

## Tools
| Tool | Purpose |
|------|---------|
| tool_review_code | Review code with ruff/mypy |
| tool_run_tests | Run pytest tests |
| tool_check_coverage | Check test coverage |
| tool_security_scan | Run bandit security scan |

---
*Phase: 04-liubu-agents*
*Completed: 2026-04-05*
