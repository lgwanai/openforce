"""Budget test fixtures.

Provides reusable fixtures for budget system testing. All fixtures use
pytest.skip() when the src.budget module is not available, enabling
TDD workflow where tests are written before implementation.
"""

import pytest
from typing import Any, Dict, Optional


# ============================================================================
# Factory Fixtures
# ============================================================================

@pytest.fixture
def budget_limits():
    """Factory fixture that creates BudgetLimits instances.

    Note: This fixture requires src.budget.manager module which will be
    created in BUD-01. Tests using this fixture will be skipped if the
    module is not available.

    Usage:
        limits = budget_limits(max_tokens=1000, max_time_seconds=60)
    """
    try:
        from src.budget.manager import BudgetLimits
        def create_limits(
            max_tokens: Optional[int] = None,
            max_time_seconds: Optional[int] = None,
            max_cost_usd: Optional[float] = None
        ):
            return BudgetLimits(
                max_tokens=max_tokens,
                max_time_seconds=max_time_seconds,
                max_cost_usd=max_cost_usd
            )
        return create_limits
    except ImportError:
        pytest.skip("BudgetLimits not yet implemented (BUD-01)")


@pytest.fixture
def budget_manager():
    """Creates a BudgetManager with default limits for testing.

    Note: This fixture requires src.budget.manager module which will be
    created in BUD-01. Tests using this fixture will be skipped if the
    module is not available.
    """
    try:
        from src.budget.manager import BudgetManager, BudgetLimits
        limits = BudgetLimits(
            max_tokens=1000,
            max_time_seconds=60,
            max_cost_usd=1.0
        )
        return BudgetManager(limits=limits)
    except ImportError:
        pytest.skip("BudgetManager not yet implemented (BUD-01)")


# ============================================================================
# Mock LLM Response Fixtures
# ============================================================================

@pytest.fixture
def mock_llm_response():
    """Creates mock LLM result with token usage metadata.

    This simulates the response structure from OpenAI-compatible APIs.

    Usage:
        response = mock_llm_response(total_tokens=150)
    """
    def create_response(
        total_tokens: int = 100,
        prompt_tokens: int = 50,
        completion_tokens: int = 50,
        content: str = "Test response"
    ) -> Dict[str, Any]:
        return {
            "llm_output": {
                "token_usage": {
                    "total_tokens": total_tokens,
                    "prompt_tokens": prompt_tokens,
                    "completion_tokens": completion_tokens
                }
            },
            "generations": [[
                {
                    "text": content,
                    "generation_info": {
                        "usage": {
                            "total_tokens": total_tokens,
                            "prompt_tokens": prompt_tokens,
                            "completion_tokens": completion_tokens
                        }
                    }
                }
            ]]
        }
    return create_response


@pytest.fixture
def mock_token_usage():
    """Creates mock token usage dict for LLM responses.

    Usage:
        usage = mock_token_usage(total=200, prompt=100, completion=100)
    """
    def create_usage(
        total: int = 100,
        prompt: int = 50,
        completion: int = 50
    ) -> Dict[str, int]:
        return {
            "total_tokens": total,
            "prompt_tokens": prompt,
            "completion_tokens": completion
        }
    return create_usage


# ============================================================================
# Pricing Configuration Fixtures
# ============================================================================

@pytest.fixture
def pricing_table():
    """Provides pricing table for cost calculation tests.

    Cost per 1K tokens for various models/providers.
    """
    return {
        "gpt-4": {
            "prompt": 0.03,
            "completion": 0.06
        },
        "gpt-3.5-turbo": {
            "prompt": 0.0015,
            "completion": 0.002
        },
        "minimax": {
            "prompt": 0.001,
            "completion": 0.001
        },
        "glm-4": {
            "prompt": 0.001,
            "completion": 0.001
        },
        "kimi": {
            "prompt": 0.0012,
            "completion": 0.0012
        }
    }
