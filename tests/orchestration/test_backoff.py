"""Tests for ORC-05: Exponential backoff."""
import pytest
import asyncio

from src.core.backoff import ExponentialBackoff, BackoffConfig, with_backoff


class TestExponentialBackoff:
    """Tests for ExponentialBackoff."""

    def test_initial_delay(self):
        """First delay should be initial_delay."""
        backoff = ExponentialBackoff(initial_delay=2.0, jitter=False)
        assert backoff.get_delay(0) == 2.0

    def test_exponential_growth(self):
        """Delay should grow exponentially."""
        backoff = ExponentialBackoff(initial_delay=1.0, multiplier=2.0, jitter=False)

        assert backoff.get_delay(0) == 1.0
        assert backoff.get_delay(1) == 2.0
        assert backoff.get_delay(2) == 4.0
        assert backoff.get_delay(3) == 8.0

    def test_max_delay(self):
        """Delay should not exceed max_delay."""
        backoff = ExponentialBackoff(initial_delay=1.0, max_delay=10.0, multiplier=2.0, jitter=False)

        assert backoff.get_delay(10) == 10.0  # Would be 1024 without cap

    def test_jitter_adds_randomness(self):
        """Jitter should add randomness."""
        backoff = ExponentialBackoff(initial_delay=10.0, jitter=True)

        delays = [backoff.get_delay(0) for _ in range(10)]
        # All delays should be in range [10, 12.5] (10 + 25% jitter)
        assert all(10.0 <= d <= 12.5 for d in delays)
        # Not all should be identical (very unlikely with random jitter)
        assert len(set(delays)) > 1

    def test_delays_generator(self):
        """delays() should yield correct number of delays."""
        backoff = ExponentialBackoff(max_retries=3)
        delays = list(backoff.delays())
        assert len(delays) == 3

    @pytest.mark.asyncio
    async def test_retry_success(self):
        """Should return result on success."""
        backoff = ExponentialBackoff()

        async def operation():
            return "success"

        result = await backoff.retry(operation)
        assert result == "success"
        assert backoff.attempt_count == 0

    @pytest.mark.asyncio
    async def test_retry_on_failure(self):
        """Should retry on failure."""
        backoff = ExponentialBackoff(max_retries=2, initial_delay=0.01)
        call_count = 0

        async def flaky():
            nonlocal call_count
            call_count += 1
            if call_count < 3:
                raise ValueError("temporary error")
            return "success"

        result = await backoff.retry(flaky, exceptions=(ValueError,))
        assert result == "success"
        assert backoff.attempt_count == 2

    @pytest.mark.asyncio
    async def test_retry_exhausted(self):
        """Should raise after max retries."""
        backoff = ExponentialBackoff(max_retries=2, initial_delay=0.01)

        async def always_fails():
            raise ValueError("always fails")

        with pytest.raises(ValueError):
            await backoff.retry(always_fails, exceptions=(ValueError,))

        assert backoff.attempt_count == 2

    @pytest.mark.asyncio
    async def test_retry_specific_exceptions(self):
        """Should only catch specified exceptions."""
        backoff = ExponentialBackoff(max_retries=2, initial_delay=0.01)

        async def raises_key_error():
            raise KeyError("not caught")

        with pytest.raises(KeyError):
            await backoff.retry(raises_key_error, exceptions=(ValueError,))

    @pytest.mark.asyncio
    async def test_sync_function(self):
        """Should work with sync functions."""
        backoff = ExponentialBackoff(initial_delay=0.01)

        def sync_op():
            return "sync_result"

        result = await backoff.retry(sync_op)
        assert result == "sync_result"

    def test_reset(self):
        """Reset should clear attempts."""
        backoff = ExponentialBackoff()
        backoff._attempts = [1, 2, 3]

        backoff.reset()
        assert backoff.attempt_count == 0

    def test_total_delay(self):
        """total_delay should sum all delays."""
        backoff = ExponentialBackoff()
        backoff._attempts = [
            type('RetryAttempt', (), {'delay': 1.0})(),
            type('RetryAttempt', (), {'delay': 2.0})(),
        ]
        assert backoff.total_delay == 3.0


class TestWithBackoffDecorator:
    """Tests for with_backoff decorator."""

    @pytest.mark.asyncio
    async def test_decorator(self):
        """Decorator should add retry behavior."""
        call_count = 0

        @with_backoff(max_retries=2, initial_delay=0.01, exceptions=(ValueError,))
        async def decorated_func():
            nonlocal call_count
            call_count += 1
            if call_count < 2:
                raise ValueError("retry")
            return "done"

        result = await decorated_func()
        assert result == "done"
        assert call_count == 2
