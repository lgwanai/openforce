"""
Standardized error handling with logging.

Implements QTY-01: Proper exception handling and logging
"""

import logging
import traceback
import asyncio
from typing import Optional, Callable, Any, TypeVar
from functools import wraps
from dataclasses import dataclass, field
from datetime import datetime
from enum import Enum

logger = logging.getLogger(__name__)
T = TypeVar('T')


class ErrorSeverity(Enum):
    """Error severity levels."""
    LOW = "low"
    MEDIUM = "medium"
    HIGH = "high"
    CRITICAL = "critical"


@dataclass
class ErrorContext:
    """Context for error tracking."""
    error_type: str
    message: str
    severity: ErrorSeverity
    timestamp: str = field(default_factory=lambda: datetime.utcnow().isoformat())
    traceback: Optional[str] = None
    context: dict = field(default_factory=dict)


class AppError(Exception):
    """Base application error."""
    def __init__(self, message: str, severity: ErrorSeverity = ErrorSeverity.MEDIUM, **context):
        super().__init__(message)
        self.severity = severity
        self.context = context
        self.timestamp = datetime.utcnow().isoformat()


class ToolExecutionError(AppError):
    """Error during tool execution."""
    pass


class AgentError(AppError):
    """Error in agent execution."""
    pass


class BudgetExceededError(AppError):
    """Budget limit exceeded."""
    def __init__(self, message: str, budget_type: str, current: float, limit: float):
        super().__init__(message, severity=ErrorSeverity.HIGH,
                       budget_type=budget_type, current=current, limit=limit)


class ApprovalRequiredError(AppError):
    """Operation requires approval."""
    def __init__(self, message: str, approval_id: str):
        super().__init__(message, severity=ErrorSeverity.MEDIUM, approval_id=approval_id)


def log_error(error: Exception, context: dict = None, severity: ErrorSeverity = None) -> ErrorContext:
    """Log an error with context."""
    if isinstance(error, AppError):
        severity = severity or error.severity
        context = {**(context or {}), **error.context}
    else:
        severity = severity or ErrorSeverity.MEDIUM

    error_ctx = ErrorContext(
        error_type=type(error).__name__,
        message=str(error),
        severity=severity,
        traceback=traceback.format_exc() if severity in [ErrorSeverity.HIGH, ErrorSeverity.CRITICAL] else None,
        context=context or {}
    )

    log_msg = f"[{error_ctx.error_type}] {error_ctx.message}"

    if severity == ErrorSeverity.CRITICAL:
        logger.critical(log_msg, extra={"context": error_ctx.context})
    elif severity == ErrorSeverity.HIGH:
        logger.error(log_msg, extra={"context": error_ctx.context})
    elif severity == ErrorSeverity.MEDIUM:
        logger.warning(log_msg, extra={"context": error_ctx.context})
    else:
        logger.info(log_msg, extra={"context": error_ctx.context})

    return error_ctx


def safe_execute(
    func: Callable[..., T],
    default: T = None,
    exceptions: tuple = (Exception,),
    log_context: dict = None
) -> tuple:
    """Safely execute a function, catching exceptions."""
    try:
        return func(), None
    except exceptions as e:
        log_error(e, context=log_context)
        return default, e


async def safe_execute_async(
    func: Callable[..., T],
    default: T = None,
    exceptions: tuple = (Exception,),
    log_context: dict = None
) -> tuple:
    """Async version of safe_execute."""
    try:
        return await func(), None
    except exceptions as e:
        log_error(e, context=log_context)
        return default, e


def with_error_handling(default=None, exceptions: tuple = (Exception,), severity: ErrorSeverity = ErrorSeverity.MEDIUM):
    """Decorator for automatic error handling."""
    def decorator(func: Callable) -> Callable:
        @wraps(func)
        def wrapper(*args, **kwargs):
            try:
                return func(*args, **kwargs)
            except exceptions as e:
                log_error(e, severity=severity)
                return default

        @wraps(func)
        async def async_wrapper(*args, **kwargs):
            try:
                return await func(*args, **kwargs)
            except exceptions as e:
                log_error(e, severity=severity)
                return default

        if asyncio.iscoroutinefunction(func):
            return async_wrapper
        return wrapper
    return decorator
