"""
Type annotation utilities.

Implements QTY-02: Type annotations and validation
"""

from typing import (
    TypeVar, Generic, Optional, List, Dict, Any,
    Callable, Union, Type, get_type_hints, get_origin, get_args
)
from dataclasses import dataclass
from functools import wraps
import inspect


T = TypeVar('T')
K = TypeVar('K')
V = TypeVar('V')


@dataclass
class TypedResult(Generic[T]):
    """Typed result wrapper for operations."""
    success: bool
    value: Optional[T] = None
    error: Optional[str] = None

    @classmethod
    def ok(cls, value: T) -> 'TypedResult[T]':
        return cls(success=True, value=value)

    @classmethod
    def fail(cls, error: str) -> 'TypedResult[T]':
        return cls(success=False, error=error)


def validate_type(value: Any, expected_type: Type) -> bool:
    """Validate that a value matches expected type."""
    if expected_type is Any:
        return True

    origin = get_origin(expected_type)
    if origin is None:
        return isinstance(value, expected_type)

    if origin is Union:
        args = get_args(expected_type)
        return any(validate_type(value, arg) for arg in args)

    if origin is list:
        if not isinstance(value, list):
            return False
        args = get_args(expected_type)
        if args:
            return all(validate_type(item, args[0]) for item in value)
        return True

    if origin is dict:
        if not isinstance(value, dict):
            return False
        args = get_args(expected_type)
        if len(args) == 2:
            key_type, value_type = args
            return all(
                validate_type(k, key_type) and validate_type(v, value_type)
                for k, v in value.items()
            )
        return True

    if origin is Optional:
        if value is None:
            return True
        args = get_args(expected_type)
        return validate_type(value, args[0]) if args else True

    return isinstance(value, origin)


def enforce_types(func: Callable) -> Callable:
    """Decorator to enforce type annotations at runtime."""
    hints = get_type_hints(func)
    sig = inspect.signature(func)

    @wraps(func)
    def wrapper(*args, **kwargs):
        bound = sig.bind(*args, **kwargs)
        bound.apply_defaults()

        for name, value in bound.arguments.items():
            if name in hints and name != 'return':
                if not validate_type(value, hints[name]):
                    raise TypeError(
                        f"Argument '{name}' expected {hints[name]}, got {type(value)}"
                    )

        result = func(*args, **kwargs)

        if 'return' in hints:
            if not validate_type(result, hints['return']):
                raise TypeError(
                    f"Return value expected {hints['return']}, got {type(result)}"
                )

        return result

    return wrapper


def ensure_type(value: Any, expected_type: Type[T], message: str = "") -> T:
    """Ensure value is of expected type, raise if not."""
    if not validate_type(value, expected_type):
        raise TypeError(message or f"Expected {expected_type}, got {type(value)}")
    return value


def safe_cast(value: Any, expected_type: Type[T], default: T = None) -> T:
    """Safely cast value to type, return default if invalid."""
    try:
        if validate_type(value, expected_type):
            return value
        return default
    except Exception:
        return default


class TypeValidator:
    """Validator for complex type structures."""

    def __init__(self, schema: Dict[str, Type]):
        self.schema = schema

    def validate(self, data: dict) -> tuple:
        """Validate data against schema. Returns (valid, errors)."""
        errors = []

        for key, expected_type in self.schema.items():
            if key not in data:
                errors.append(f"Missing key: {key}")
            elif not validate_type(data[key], expected_type):
                errors.append(f"Invalid type for '{key}': expected {expected_type}")

        return len(errors) == 0, errors

    def __call__(self, data: dict) -> bool:
        valid, _ = self.validate(data)
        return valid
