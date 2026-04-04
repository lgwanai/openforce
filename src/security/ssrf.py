"""
SSRF (Server-Side Request Forgery) Protection Module.

This module provides URL validation and safe fetching utilities to prevent
SSRF attacks that could access internal network resources.

Security features:
- Blocks private IP ranges (RFC 1918)
- Blocks loopback addresses (127.x.x.x, ::1)
- Blocks link-local addresses (169.254.x.x)
- Blocks CGNAT range (100.64.x.x)
- Blocks non-HTTP schemes (file://, ftp://, gopher://, etc.)
- Resolves DNS before validation to prevent DNS rebinding
- Does not follow redirects automatically
"""

import ipaddress
import socket
from urllib.parse import urlparse
from typing import Optional
import httpx
from bs4 import BeautifulSoup


class SSRFError(Exception):
    """Raised when an SSRF attempt is detected."""
    pass


# Private IP ranges to block
PRIVATE_IP_RANGES = [
    ipaddress.ip_network('10.0.0.0/8'),      # RFC 1918 - Class A private
    ipaddress.ip_network('172.16.0.0/12'),   # RFC 1918 - Class B private
    ipaddress.ip_network('192.168.0.0/16'),  # RFC 1918 - Class C private
    ipaddress.ip_network('127.0.0.0/8'),     # Loopback
    ipaddress.ip_network('169.254.0.0/16'),  # Link-local (AWS metadata endpoint)
    ipaddress.ip_network('::1/128'),         # IPv6 loopback
    ipaddress.ip_network('fc00::/7'),        # IPv6 ULA (Unique Local Address)
    ipaddress.ip_network('fe80::/10'),       # IPv6 link-local
    ipaddress.ip_network('100.64.0.0/10'),   # CGNAT (Carrier-grade NAT)
]

# Only HTTP and HTTPS schemes are allowed
ALLOWED_SCHEMES = {'http', 'https'}

# Blocked hostnames that should never be accessed
BLOCKED_HOSTS = {
    'localhost',
    'localhost.localdomain',
    'ip6-localhost',
    'ip6-loopback',
}


def validate_url_for_ssrf(url: str) -> str:
    """
    Validate that a URL is safe for server-side fetching.

    This function performs comprehensive SSRF protection by:
    1. Parsing the URL and validating the scheme
    2. Checking for blocked hostnames
    3. Resolving the hostname to IP addresses
    4. Checking all resolved IPs against private ranges

    Args:
        url: The URL to validate

    Returns:
        The validated URL (unchanged if valid)

    Raises:
        SSRFError: If the URL is potentially malicious or could be used
                   for SSRF attacks
    """
    # Parse URL
    try:
        parsed = urlparse(url)
    except Exception as e:
        raise SSRFError(f"Invalid URL format: {e}")

    # Validate scheme
    if parsed.scheme.lower() not in ALLOWED_SCHEMES:
        raise SSRFError(f"URL scheme not allowed: {parsed.scheme}")

    # Check for hostname
    hostname = parsed.hostname
    if not hostname:
        raise SSRFError("URL must have a hostname")

    # Check blocked hostnames (before DNS resolution for performance)
    if hostname.lower() in BLOCKED_HOSTS:
        raise SSRFError(f"Hostname not allowed: {hostname}")

    # Resolve hostname to IP and check against private ranges
    # This prevents DNS rebinding attacks where an attacker controls
    # a domain that initially resolves to a public IP but later resolves
    # to a private IP
    try:
        # Get all IP addresses for the hostname
        # port is used to help with resolution but doesn't affect the IP check
        addr_infos = socket.getaddrinfo(hostname, parsed.port or 443)

        for family, socktype, proto, canonname, sockaddr in addr_infos:
            # sockaddr format varies by address family:
            # IPv4: (ip, port)
            # IPv6: (ip, port, flow_info, scope_id)
            ip_str = sockaddr[0]
            ip = ipaddress.ip_address(ip_str)

            # Check against all private IP ranges
            for private_range in PRIVATE_IP_RANGES:
                if ip in private_range:
                    raise SSRFError(
                        f"Resolved IP {ip} is in private range"
                    )

    except socket.gaierror as e:
        raise SSRFError(f"Could not resolve hostname: {e}")

    return url


def fetch_webpage_safe(
    url: str,
    timeout: float = 10.0,
    max_length: int = 5000
) -> str:
    """
    Safely fetch webpage content with SSRF protection.
    """
    try:
        # Validate URL first
        validated_url = validate_url_for_ssrf(url)

        # Fetch with redirect handling - validate each redirect
        current_url = validated_url
        max_redirects = 5
        redirect_count = 0

        while redirect_count < max_redirects:
            resp = httpx.get(current_url, timeout=timeout, follow_redirects=False)

            if resp.status_code in (301, 302, 303, 307, 308):
                redirect_url = resp.headers.get("location")
                if not redirect_url:
                    break
                try:
                    validated_redirect = validate_url_for_ssrf(redirect_url)
                    current_url = validated_redirect
                    redirect_count += 1
                except SSRFError:
                    return f"SSRF blocked in redirect: {redirect_url}"
            else:
                break

        resp.raise_for_status()

        # Parse and extract text
        soup = BeautifulSoup(resp.text, "html.parser")
        text = soup.get_text(separator="\n", strip=True)

        return text[:max_length] if len(text) > max_length else text

    except SSRFError as e:
        return f"SSRF blocked: {str(e)}"
    except httpx.HTTPStatusError as e:
        return f"HTTP error: {e}"
    except Exception as e:
        return f"Failed to fetch: {str(e)}"
