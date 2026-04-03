import pytest
from unittest.mock import patch, MagicMock
import ipaddress

# Import the module to be tested (will fail initially)
from src.security.ssrf import (
    SSRFError,
    validate_url_for_ssrf,
    fetch_webpage_safe,
    PRIVATE_IP_RANGES,
    ALLOWED_SCHEMES,
    BLOCKED_HOSTS,
)


class TestSSRFConstants:
    """Tests for SSRF protection constants."""

    def test_private_ip_ranges_defined(self):
        """Verify private IP ranges are properly defined."""
        assert len(PRIVATE_IP_RANGES) >= 8  # At least RFC1918 + loopback + link-local

        # Check RFC 1918 ranges are included
        rfc1918_networks = [
            ipaddress.ip_network('10.0.0.0/8'),
            ipaddress.ip_network('172.16.0.0/12'),
            ipaddress.ip_network('192.168.0.0/16'),
        ]
        for network in rfc1918_networks:
            assert any(
                network == r or network.subnet_of(r)
                for r in PRIVATE_IP_RANGES
            ), f"RFC 1918 range {network} should be in PRIVATE_IP_RANGES"

    def test_allowed_schemes_http_https_only(self):
        """Verify only HTTP and HTTPS schemes are allowed."""
        assert 'http' in ALLOWED_SCHEMES
        assert 'https' in ALLOWED_SCHEMES
        assert 'file' not in ALLOWED_SCHEMES
        assert 'ftp' not in ALLOWED_SCHEMES
        assert 'gopher' not in ALLOWED_SCHEMES

    def test_blocked_hosts_includes_localhost(self):
        """Verify localhost variants are blocked."""
        assert 'localhost' in BLOCKED_HOSTS
        assert 'localhost.localdomain' in BLOCKED_HOSTS


class TestValidateUrlForSsrf:
    """Tests for validate_url_for_ssrf function."""

    def test_valid_public_url_allowed(self):
        """Verify valid public URLs are allowed."""
        # Use a public IP that won't resolve to private range
        with patch('socket.getaddrinfo') as mock_getaddrinfo:
            # Simulate a public IP (8.8.8.8 is Google DNS)
            mock_getaddrinfo.return_value = [
                (2, 1, 6, '', ('8.8.8.8', 443))
            ]
            result = validate_url_for_ssrf('https://example.com/path')
            assert result == 'https://example.com/path'

    def test_private_ip_10_range_blocked(self):
        """Verify 10.x.x.x private IPs are blocked."""
        with patch('socket.getaddrinfo') as mock_getaddrinfo:
            mock_getaddrinfo.return_value = [
                (2, 1, 6, '', ('10.0.0.1', 443))
            ]
            with pytest.raises(SSRFError, match='private range'):
                validate_url_for_ssrf('https://internal.example.com')

    def test_private_ip_172_range_blocked(self):
        """Verify 172.16-31.x.x private IPs are blocked."""
        with patch('socket.getaddrinfo') as mock_getaddrinfo:
            mock_getaddrinfo.return_value = [
                (2, 1, 6, '', ('172.16.0.1', 443))
            ]
            with pytest.raises(SSRFError, match='private range'):
                validate_url_for_ssrf('https://internal.example.com')

    def test_private_ip_192_168_range_blocked(self):
        """Verify 192.168.x.x private IPs are blocked."""
        with patch('socket.getaddrinfo') as mock_getaddrinfo:
            mock_getaddrinfo.return_value = [
                (2, 1, 6, '', ('192.168.1.1', 443))
            ]
            with pytest.raises(SSRFError, match='private range'):
                validate_url_for_ssrf('https://internal.example.com')

    def test_localhost_blocked(self):
        """Verify localhost is blocked (hostname check happens before DNS)."""
        # localhost is blocked by hostname check, not by IP check
        with pytest.raises(SSRFError, match='Hostname not allowed'):
            validate_url_for_ssrf('http://localhost:8080/admin')

    def test_localhost_hostname_blocked(self):
        """Verify localhost hostname is blocked before DNS resolution."""
        with pytest.raises(SSRFError, match='Hostname not allowed'):
            validate_url_for_ssrf('http://localhost/admin')

    def test_loopback_127_range_blocked(self):
        """Verify entire 127.x.x.x range is blocked."""
        with patch('socket.getaddrinfo') as mock_getaddrinfo:
            mock_getaddrinfo.return_value = [
                (2, 1, 6, '', ('127.0.0.1', 443))
            ]
            with pytest.raises(SSRFError, match='private range'):
                validate_url_for_ssrf('https://loopback.example.com')

    def test_link_local_169_254_blocked(self):
        """Verify link-local 169.254.x.x is blocked (AWS metadata)."""
        with patch('socket.getaddrinfo') as mock_getaddrinfo:
            mock_getaddrinfo.return_value = [
                (2, 1, 6, '', ('169.254.169.254', 80))
            ]
            with pytest.raises(SSRFError, match='private range'):
                validate_url_for_ssrf('http://metadata.aws/')

    def test_ipv6_loopback_blocked(self):
        """Verify IPv6 loopback ::1 is blocked."""
        with patch('socket.getaddrinfo') as mock_getaddrinfo:
            mock_getaddrinfo.return_value = [
                (10, 1, 6, '', ('::1', 443, 0, 0))
            ]
            with pytest.raises(SSRFError, match='private range'):
                validate_url_for_ssrf('https://ipv6-loopback.example.com')

    def test_file_scheme_blocked(self):
        """Verify file:// scheme is blocked."""
        with pytest.raises(SSRFError, match='scheme not allowed'):
            validate_url_for_ssrf('file:///etc/passwd')

    def test_ftp_scheme_blocked(self):
        """Verify ftp:// scheme is blocked."""
        with pytest.raises(SSRFError, match='scheme not allowed'):
            validate_url_for_ssrf('ftp://internal.server/file')

    def test_gopher_scheme_blocked(self):
        """Verify gopher:// scheme is blocked."""
        with pytest.raises(SSRFError, match='scheme not allowed'):
            validate_url_for_ssrf('gopher://internal.server')

    def test_invalid_url_format_raises_error(self):
        """Verify invalid URL format raises SSRFError."""
        # urlparse doesn't raise for "not a valid url" - it parses it as relative URL
        # The error will be on scheme check or missing hostname
        # URLs without scheme get empty scheme, which is not in ALLOWED_SCHEMES
        with pytest.raises(SSRFError, match='scheme not allowed'):
            validate_url_for_ssrf('not a valid url')

    def test_url_without_hostname_raises_error(self):
        """Verify URL without hostname raises SSRFError."""
        with pytest.raises(SSRFError, match='must have a hostname'):
            validate_url_for_ssrf('https:///path/only')

    def test_dns_resolution_failure_raises_error(self):
        """Verify DNS resolution failure raises SSRFError."""
        import socket
        with patch('socket.getaddrinfo') as mock_getaddrinfo:
            mock_getaddrinfo.side_effect = socket.gaierror('Name resolution failed')
            with pytest.raises(SSRFError, match='Could not resolve'):
                validate_url_for_ssrf('https://nonexistent.example.com')

    def test_cgnat_range_blocked(self):
        """Verify CGNAT range 100.64.0.0/10 is blocked."""
        with patch('socket.getaddrinfo') as mock_getaddrinfo:
            mock_getaddrinfo.return_value = [
                (2, 1, 6, '', ('100.64.0.1', 443))
            ]
            with pytest.raises(SSRFError, match='private range'):
                validate_url_for_ssrf('https://cgnat.example.com')


class TestFetchWebpageSafe:
    """Tests for fetch_webpage_safe function."""

    def test_valid_url_returns_content(self):
        """Verify valid URL returns webpage content."""
        mock_response = MagicMock()
        mock_response.text = '<html><body>Test Content</body></html>'
        mock_response.raise_for_status = MagicMock()

        with patch('socket.getaddrinfo') as mock_getaddrinfo, \
             patch('httpx.get') as mock_get:
            mock_getaddrinfo.return_value = [
                (2, 1, 6, '', ('8.8.8.8', 443))
            ]
            mock_get.return_value = mock_response

            result = fetch_webpage_safe('https://example.com')
            assert 'Test Content' in result

    def test_ssrf_blocked_returns_error_message(self):
        """Verify SSRF attempt returns error message, not exception."""
        result = fetch_webpage_safe('http://localhost:8080/admin')
        assert 'SSRF blocked' in result

    def test_file_scheme_returns_error_message(self):
        """Verify file:// scheme returns error message."""
        result = fetch_webpage_safe('file:///etc/passwd')
        assert 'SSRF blocked' in result

    def test_http_error_returns_error_message(self):
        """Verify HTTP error returns error message."""
        import httpx
        mock_response = MagicMock()
        mock_response.raise_for_status.side_effect = httpx.HTTPStatusError(
            'Not Found', request=MagicMock(), response=MagicMock(status_code=404)
        )

        with patch('socket.getaddrinfo') as mock_getaddrinfo, \
             patch('httpx.get') as mock_get:
            mock_getaddrinfo.return_value = [
                (2, 1, 6, '', ('8.8.8.8', 443))
            ]
            mock_get.return_value = mock_response

            result = fetch_webpage_safe('https://example.com/notfound')
            assert 'HTTP error' in result

    def test_content_truncated_to_max_length(self):
        """Verify content is truncated to max_length."""
        long_content = 'x' * 10000
        mock_response = MagicMock()
        mock_response.text = f'<html><body>{long_content}</body></html>'
        mock_response.raise_for_status = MagicMock()

        with patch('socket.getaddrinfo') as mock_getaddrinfo, \
             patch('httpx.get') as mock_get:
            mock_getaddrinfo.return_value = [
                (2, 1, 6, '', ('8.8.8.8', 443))
            ]
            mock_get.return_value = mock_response

            result = fetch_webpage_safe('https://example.com', max_length=100)
            assert len(result) <= 100

    def test_custom_timeout_used(self):
        """Verify custom timeout is passed to httpx."""
        mock_response = MagicMock()
        mock_response.text = '<html><body>Content</body></html>'
        mock_response.raise_for_status = MagicMock()

        with patch('socket.getaddrinfo') as mock_getaddrinfo, \
             patch('httpx.get') as mock_get:
            mock_getaddrinfo.return_value = [
                (2, 1, 6, '', ('8.8.8.8', 443))
            ]
            mock_get.return_value = mock_response

            fetch_webpage_safe('https://example.com', timeout=30.0)
            mock_get.assert_called_once()
            call_kwargs = mock_get.call_args[1]
            assert call_kwargs['timeout'] == 30.0

    def test_no_redirects_followed(self):
        """Verify redirects are not followed automatically."""
        mock_response = MagicMock()
        mock_response.text = '<html><body>Content</body></html>'
        mock_response.raise_for_status = MagicMock()

        with patch('socket.getaddrinfo') as mock_getaddrinfo, \
             patch('httpx.get') as mock_get:
            mock_getaddrinfo.return_value = [
                (2, 1, 6, '', ('8.8.8.8', 443))
            ]
            mock_get.return_value = mock_response

            fetch_webpage_safe('https://example.com')
            call_kwargs = mock_get.call_args[1]
            assert call_kwargs['follow_redirects'] is False
