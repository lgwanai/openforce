"""Tests for QTY-02: Type utilities."""
import pytest
from typing import List, Dict, Optional, Union

from src.core.type_utils import (
    TypedResult, validate_type, enforce_types, ensure_type,
    safe_cast, TypeValidator
)


class TestTypedResult:
    """Tests for TypedResult."""

    def test_ok(self):
        """ok() should create success result."""
        result = TypedResult.ok(42)
        assert result.success
        assert result.value == 42
        assert result.error is None

    def test_fail(self):
        """fail() should create failure result."""
        result = TypedResult.fail("error message")
        assert not result.success
        assert result.value is None
        assert result.error == "error message"


class TestValidateType:
    """Tests for validate_type."""

    def test_simple_types(self):
        """Should validate simple types."""
        assert validate_type(42, int)
        assert validate_type("hello", str)
        assert validate_type([1, 2], list)
        assert not validate_type("hello", int)

    def test_list_type(self):
        """Should validate list with element type."""
        assert validate_type([1, 2, 3], List[int])
        assert not validate_type([1, "two"], List[int])

    def test_dict_type(self):
        """Should validate dict with key/value types."""
        assert validate_type({"a": 1}, Dict[str, int])
        assert not validate_type({"a": "b"}, Dict[str, int])

    def test_optional_type(self):
        """Should validate Optional types."""
        assert validate_type(None, Optional[int])
        assert validate_type(42, Optional[int])
        assert not validate_type("hello", Optional[int])

    def test_union_type(self):
        """Should validate Union types."""
        assert validate_type(42, Union[int, str])
        assert validate_type("hello", Union[int, str])
        assert not validate_type([], Union[int, str])


class TestEnforceTypes:
    """Tests for enforce_types decorator."""

    def test_valid_types(self):
        """Should pass with valid types."""
        @enforce_types
        def add(a: int, b: int) -> int:
            return a + b
        assert add(1, 2) == 3

    def test_invalid_argument(self):
        """Should raise on invalid argument."""
        @enforce_types
        def add(a: int, b: int) -> int:
            return a + b
        with pytest.raises(TypeError):
            add("one", 2)

    def test_invalid_return(self):
        """Should raise on invalid return."""
        @enforce_types
        def bad() -> int:
            return "not int"
        with pytest.raises(TypeError):
            bad()


class TestEnsureType:
    """Tests for ensure_type."""

    def test_valid(self):
        """Should return value if valid."""
        result = ensure_type(42, int)
        assert result == 42

    def test_invalid(self):
        """Should raise on invalid."""
        with pytest.raises(TypeError):
            ensure_type("hello", int)


class TestSafeCast:
    """Tests for safe_cast."""

    def test_valid(self):
        """Should return value if valid."""
        result = safe_cast(42, int, default=0)
        assert result == 42

    def test_invalid(self):
        """Should return default if invalid."""
        result = safe_cast("hello", int, default=0)
        assert result == 0


class TestTypeValidator:
    """Tests for TypeValidator."""

    def test_valid_schema(self):
        """Should validate valid data."""
        validator = TypeValidator({"name": str, "age": int})
        valid, errors = validator.validate({"name": "Alice", "age": 30})
        assert valid
        assert len(errors) == 0

    def test_missing_key(self):
        """Should report missing keys."""
        validator = TypeValidator({"name": str, "age": int})
        valid, errors = validator.validate({"name": "Alice"})
        assert not valid
        assert "age" in errors[0]

    def test_invalid_type(self):
        """Should report type mismatches."""
        validator = TypeValidator({"age": int})
        valid, errors = validator.validate({"age": "thirty"})
        assert not valid
        assert "age" in errors[0]

    def test_callable(self):
        """Should be callable."""
        validator = TypeValidator({"name": str})
        assert validator({"name": "Alice"})
        assert not validator({})
