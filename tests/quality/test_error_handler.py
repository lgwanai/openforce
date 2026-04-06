"""Tests for QTY-01: Error handling."""
import pytest
import asyncio

from src.core.error_handler import (
    ErrorSeverity, ErrorContext, AppError, ToolExecutionError,
    AgentError, BudgetExceededError, ApprovalRequiredError,
    log_error, safe_execute, safe_execute_async, with_error_handling
)


class TestAppError:
    """Tests for AppError class."""

    def test_app_error_basic(self):
        """AppError should work as basic exception."""
        error = AppError("test error")
        assert str(error) == "test error"
        assert error.severity == ErrorSeverity.MEDIUM

    def test_app_error_with_context(self):
        """AppError should store context."""
        error = AppError("test", severity=ErrorSeverity.HIGH, key="value")
        assert error.severity == ErrorSeverity.HIGH
        assert error.context == {"key": "value"}


class TestBudgetExceededError:
    """Tests for BudgetExceededError."""

    def test_budget_exceeded(self):
        """BudgetExceededError should store budget info."""
        error = BudgetExceededError("Over limit", "token", 1500, 1000)
        assert error.severity == ErrorSeverity.HIGH
        assert error.context["budget_type"] == "token"
        assert error.context["current"] == 1500


class TestLogError:
    """Tests for log_error function."""

    def test_log_error_returns_context(self):
        """log_error should return ErrorContext."""
        error = ValueError("test")
        ctx = log_error(error)
        assert isinstance(ctx, ErrorContext)
        assert ctx.error_type == "ValueError"
        assert ctx.message == "test"

    def test_log_error_app_error(self):
        """log_error should extract context from AppError."""
        error = AppError("test", key="value")
        ctx = log_error(error)
        assert ctx.context == {"key": "value"}


class TestSafeExecute:
    """Tests for safe_execute function."""

    def test_safe_execute_success(self):
        """safe_execute should return result on success."""
        def success():
            return 42
        result, error = safe_execute(success)
        assert result == 42
        assert error is None

    def test_safe_execute_failure(self):
        """safe_execute should return default on failure."""
        def failure():
            raise ValueError("error")
        result, error = safe_execute(failure, default=0)
        assert result == 0
        assert isinstance(error, ValueError)

    def test_safe_execute_specific_exceptions(self):
        """safe_execute should only catch specified exceptions."""
        def raises_key():
            raise KeyError("key")
        with pytest.raises(KeyError):
            safe_execute(raises_key, exceptions=(ValueError,))


class TestSafeExecuteAsync:
    """Tests for safe_execute_async."""

    @pytest.mark.asyncio
    async def test_async_success(self):
        """safe_execute_async should work with async functions."""
        async def async_success():
            return "async"
        result, error = await safe_execute_async(async_success)
        assert result == "async"
        assert error is None

    @pytest.mark.asyncio
    async def test_async_failure(self):
        """safe_execute_async should handle async failures."""
        async def async_failure():
            raise ValueError("async error")
        result, error = await safe_execute_async(async_failure, default="default")
        assert result == "default"
        assert isinstance(error, ValueError)


class TestWithErrorHandling:
    """Tests for with_error_handling decorator."""

    def test_decorator_success(self):
        """Decorator should allow success."""
        @with_error_handling(default="fallback")
        def func():
            return "success"
        assert func() == "success"

    def test_decorator_failure(self):
        """Decorator should return default on failure."""
        @with_error_handling(default="fallback")
        def func():
            raise ValueError("error")
        assert func() == "fallback"

    @pytest.mark.asyncio
    async def test_decorator_async(self):
        """Decorator should work with async functions."""
        @with_error_handling(default="fallback")
        async def async_func():
            raise ValueError("error")
        result = await async_func()
        assert result == "fallback"
