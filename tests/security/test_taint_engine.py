import pytest

from src.security.taint_engine import (
    TaintEngine,
    TaintedValue,
    TrustLevel,
    TaintSource,
    taint_source,
)


class TestTaintedValue:
    """Tests for TaintedValue dataclass."""

    def test_tainted_value_tracks_sources(self):
        """Verify TaintedValue tracks sources correctly."""
        value = TaintedValue(value="test data", sources={TaintSource.WEB})
        assert TaintSource.WEB in value.sources
        assert value.value == "test data"

    def test_trust_level_from_sources(self):
        """Verify trust level derived from sources."""
        # WEB source should be UNTRUSTED
        web_value = TaintedValue(value="web data", sources={TaintSource.WEB})
        assert web_value.trust_level == TrustLevel.UNTRUSTED

        # UPLOAD source should be UNTRUSTED
        upload_value = TaintedValue(value="upload data", sources={TaintSource.UPLOAD})
        assert upload_value.trust_level == TrustLevel.UNTRUSTED

        # USER_FREE_TEXT should be DERIVED
        user_value = TaintedValue(value="user input", sources={TaintSource.USER_FREE_TEXT})
        assert user_value.trust_level == TrustLevel.DERIVED

        # SEARCH should be DERIVED
        search_value = TaintedValue(value="search result", sources={TaintSource.SEARCH})
        assert search_value.trust_level == TrustLevel.DERIVED

        # INTERNAL should be TRUSTED
        internal_value = TaintedValue(value="internal data", sources={TaintSource.INTERNAL})
        assert internal_value.trust_level == TrustLevel.TRUSTED

    def test_propagate_to_creates_new_tainted_value(self):
        """Verify propagate_to creates new tainted value with same taint."""
        original = TaintedValue(value="original", sources={TaintSource.WEB})
        propagated = original.propagate_to("new value")

        assert propagated.value == "new value"
        assert propagated.sources == original.sources
        assert propagated.trust_level == original.trust_level
        # Ensure it's a copy, not the same object
        assert propagated is not original

    def test_trusted_factory_method(self):
        """Verify trusted() factory method creates trusted values."""
        value = TaintedValue.trusted("safe data")
        assert value.value == "safe data"
        assert value.trust_level == TrustLevel.TRUSTED
        assert TaintSource.INTERNAL in value.sources

    def test_from_web_factory_method(self):
        """Verify from_web() factory method."""
        value = TaintedValue.from_web("web content")
        assert value.value == "web content"
        assert value.trust_level == TrustLevel.UNTRUSTED
        assert TaintSource.WEB in value.sources

    def test_from_user_factory_method(self):
        """Verify from_user() factory method."""
        value = TaintedValue.from_user("user input")
        assert value.value == "user input"
        assert value.trust_level == TrustLevel.DERIVED
        assert TaintSource.USER_FREE_TEXT in value.sources


class TestTaintEngine:
    """Tests for SEC-05: Taint tracking enforcement."""

    def test_high_risk_tools_blocked(self):
        """Verify high-risk tools are always blocked."""
        # execute_command should always return False (requires approval)
        assert TaintEngine.check_tool_call("execute_command", {}) is False
        assert TaintEngine.check_tool_call("delete_file", {}) is False
        assert TaintEngine.check_tool_call("write_api", {}) is False
        assert TaintEngine.check_tool_call("run_shell", {}) is False

    def test_medium_risk_tools_check_trust(self):
        """Verify medium-risk tools check trust level."""
        trusted_value = TaintedValue.trusted("safe content")
        untrusted_value = TaintedValue.from_web("web content")

        # Medium-risk tool with trusted data - allowed
        result = TaintEngine.check_tool_call(
            "write_file",
            {"content": trusted_value.value},
            {"content": trusted_value}
        )
        assert result is True

        # Medium-risk tool with untrusted data - blocked
        result = TaintEngine.check_tool_call(
            "write_file",
            {"content": untrusted_value.value},
            {"content": untrusted_value}
        )
        assert result is False

    def test_low_risk_tools_allowed(self):
        """Verify low-risk tools are allowed for all trust levels."""
        untrusted_value = TaintedValue.from_web("web content")

        # Low-risk tool with untrusted data - allowed
        result = TaintEngine.check_tool_call(
            "read_file",
            {"path": "/some/path"},
            {"path": untrusted_value}
        )
        assert result is True

    def test_sanitization_upgrades_trust(self):
        """Verify sanitization upgrades trust level."""
        untrusted_value = TaintedValue.from_web("dirty data")
        sanitized = TaintEngine.sanitize(untrusted_value, "html_sanitizer")

        assert sanitized.trust_level == TrustLevel.DERIVED
        assert TaintSource.INTERNAL in sanitized.sources
        # Original value preserved
        assert sanitized.value == untrusted_value.value

    def test_get_trust_level_from_sources(self):
        """Verify get_trust_level derives from sources."""
        assert TaintEngine.get_trust_level([TaintSource.WEB]) == TrustLevel.UNTRUSTED
        assert TaintEngine.get_trust_level([TaintSource.UPLOAD]) == TrustLevel.UNTRUSTED
        assert TaintEngine.get_trust_level([TaintSource.USER_FREE_TEXT]) == TrustLevel.DERIVED
        assert TaintEngine.get_trust_level([TaintSource.SEARCH]) == TrustLevel.DERIVED
        assert TaintEngine.get_trust_level([TaintSource.INTERNAL]) == TrustLevel.TRUSTED


class TestTaintSourceDecorator:
    """Tests for @taint_source decorator."""

    def test_taint_source_decorator(self):
        """Verify @taint_source decorator marks output."""
        @taint_source(TaintSource.WEB)
        def fetch_data():
            return "some data"

        result = fetch_data()
        assert isinstance(result, TaintedValue)
        assert result.trust_level == TrustLevel.UNTRUSTED
        assert TaintSource.WEB in result.sources

    def test_taint_source_decorator_preserves_already_tainted(self):
        """Verify decorator preserves already-tainted values."""
        @taint_source(TaintSource.WEB)
        def fetch_tainted():
            return TaintedValue.from_user("user data")

        result = fetch_tainted()
        assert isinstance(result, TaintedValue)
        # Should preserve original taint (USER_FREE_TEXT), not override with WEB
        assert TaintSource.USER_FREE_TEXT in result.sources

    def test_taint_source_decorator_with_args(self):
        """Verify decorator works with function arguments."""
        @taint_source(TaintSource.SEARCH)
        def search(query: str):
            return f"results for {query}"

        result = search("test query")
        assert isinstance(result, TaintedValue)
        assert result.value == "results for test query"
        assert TaintSource.SEARCH in result.sources
