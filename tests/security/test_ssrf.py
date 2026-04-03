import pytest


class TestSSRF:
    """Tests for SEC-04: SSRF protection."""

    def test_private_ip_blocked(self):
        """Verify private IPs are blocked."""
        # TODO: Implement after SEC-04 code changes
        pass

    def test_localhost_blocked(self):
        """Verify localhost is blocked."""
        # TODO: Implement after SEC-04 code changes
        pass

    def test_valid_url_allowed(self):
        """Verify valid public URLs are allowed."""
        # TODO: Implement after SEC-04 code changes
        pass
