"""
Exponential backoff implementation.

Implements ORC-05: Retry with exponential backoff
"""

import asyncio
import random
from typing import Callable, Optional, TypeVar, List
from dataclasses import dataclass
from datetime import datetime
import logging

logger = logging.getLogger(__name__)
T = TypeVar('T')


@dataclass
class BackoffConfig:
    """Configuration for backoff behavior."""
    initial_delay: float = 1.0
    max_delay: float = 60.0
    multiplier: float = 2.0
    jitter: bool = True
    max_retries: int = 5


@dataclass
class RetryAttempt:
    """Record of a retry attempt."""
    attempt: int
    delay: float
    error: Optional[Exception] = None
    timestamp: str = ""

    def __post_init__(self):
        if not self.timestamp:
            self.timestamp = datetime.utcnow().isoformat()


class ExponentialBackoff:
    """Exponential backoff with jitter."""

    def __init__(
        self,
        initial_delay: float = 1.0,
        max_delay: float = 60.0,
        multiplier: float = 2.0,
        jitter: bool = True,
        max_retries: int = 5
    ):
        self.initial_delay = initial_delay
        self.max_delay = max_delay
        self.multiplier = multiplier
        self.jitter = jitter
        self.max_retries = max_retries
        self._attempts: List[RetryAttempt] = []

    def get_delay(self, attempt: int) -> float:
        """Calculate delay for given attempt number."""
        delay = self.initial_delay * (self.multiplier ** attempt)
        delay = min(delay, self.max_delay)

        if self.jitter:
            jitter_amount = delay * random.random() * 0.25
            delay += jitter_amount

        return delay

    def delays(self):
        """Generator yielding delays for each retry."""
        for attempt in range(self.max_retries):
            yield self.get_delay(attempt)

    async def retry(
        self,
        operation: Callable[[], T],
        exceptions: tuple = (Exception,),
        on_retry: Optional[Callable[[int, Exception], None]] = None
    ) -> T:
        """Execute operation with automatic retry on failure."""
        last_error = None

        for attempt in range(self.max_retries + 1):
            try:
                if asyncio.iscoroutinefunction(operation):
                    return await operation()
                else:
                    return operation()
            except exceptions as e:
                last_error = e

                if attempt < self.max_retries:
                    delay = self.get_delay(attempt)
                    self._attempts.append(RetryAttempt(attempt=attempt + 1, delay=delay, error=e))

                    if on_retry:
                        on_retry(attempt + 1, e)

                    await asyncio.sleep(delay)
                else:
                    raise

        raise last_error

    @property
    def attempt_count(self) -> int:
        return len(self._attempts)

    @property
    def total_delay(self) -> float:
        return sum(a.delay for a in self._attempts)

    def reset(self) -> None:
        self._attempts = []


def with_backoff(
    max_retries: int = 5,
    initial_delay: float = 1.0,
    max_delay: float = 60.0,
    exceptions: tuple = (Exception,)
):
    """Decorator for automatic retry with backoff."""
    def decorator(func: Callable) -> Callable:
        backoff = ExponentialBackoff(
            max_retries=max_retries, initial_delay=initial_delay, max_delay=max_delay
        )

        async def wrapper(*args, **kwargs):
            async def operation():
                return await func(*args, **kwargs)
            return await backoff.retry(operation, exceptions)

        return wrapper
    return decorator
