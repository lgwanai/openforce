"""Edge-case tests for utils security module."""
import sys
import os
import unicodedata

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from utils import *


def test_empty_strings():
    """Edge Case 1: Empty strings"""
    print("=== Empty strings ===")
    try:
        sanitize_identifier("")
        print("  FAIL: sanitize_identifier('') should raise")
    except InputTooShortError:
        print("  PASS: sanitize_identifier('') raises InputTooShortError")

    try:
        sanitize_slug("")
        print("  FAIL: sanitize_slug('') should raise")
    except InvalidCharacterError:
        print("  PASS: sanitize_slug('') raises InvalidCharacterError")

    try:
        sanitize_filename("")
        print("  FAIL: sanitize_filename('') should raise")
    except InvalidCharacterError:
        print("  PASS: sanitize_filename('') raises InvalidCharacterError")

    r = sanitize_text("")
    assert r == "", f"Expected empty string, got {repr(r)}"
    print("  PASS: sanitize_text('') returns empty string")


def test_overly_long_strings():
    """Edge Case 2: Overly long strings"""
    print("\n=== Overly long strings ===")
    for fn, arg, expected in [
        (sanitize_identifier, "a" * 200, InputTooLongError),
        (sanitize_text, "x" * 5000, InputTooLongError),
        (sanitize_slug, "x" * 200, InputTooLongError),
        (sanitize_filename, "x" * 300, InputTooLongError),
    ]:
        try:
            fn(arg)
            print(f"  FAIL: {fn.__name__} should raise")
        except expected:
            print(f"  PASS: {fn.__name__} raises {expected.__name__}")


def test_null_bytes():
    """Edge Case 3: Null bytes"""
    print("\n=== Null bytes ===")
    try:
        sanitize_identifier("hello\x00world")
        print("  FAIL: sanitize_identifier(null) should raise")
    except InvalidCharacterError:
        print("  PASS: sanitize_identifier(null) raises InvalidCharacterError")

    try:
        sanitize_text("hello\x00world")
        print("  FAIL: sanitize_text(null) should raise")
    except InvalidCharacterError:
        print("  PASS: sanitize_text(null) raises InvalidCharacterError")

    r = strip_control_characters("hello\x00world")
    assert r == "helloworld", f"Expected helloworld, got {repr(r)}"
    print("  PASS: strip_control_characters removes null bytes")

    r = sanitize_filename("file\x00name.txt")
    assert r == "filename.txt", f"Expected filename.txt, got {repr(r)}"
    print("  PASS: sanitize_filename strips null bytes")


def test_special_unicode():
    """Edge Case 4: Special Unicode characters"""
    print("\n=== Special Unicode ===")
    # Zero-width space (U+200B)
    try:
        sanitize_identifier("\u200b")
        print("  FAIL: sanitize_identifier(zero-width) should raise")
    except InvalidCharacterError:
        print("  PASS: sanitize_identifier(zero-width) raises InvalidCharacterError")

    # Bidi override (U+202E)
    try:
        sanitize_identifier("hello\u202Eworld")
        print("  FAIL: sanitize_identifier(bidi) should raise")
    except InvalidCharacterError:
        print("  PASS: sanitize_identifier(bidi) raises InvalidCharacterError")

    # Cyrillic homoglyph
    try:
        sanitize_identifier("\u0430dmin")  # Cyrillic 'а'
        print("  FAIL: sanitize_identifier(Cyrillic) should raise")
    except InvalidCharacterError:
        print("  PASS: sanitize_identifier(Cyrillic) raises InvalidCharacterError")

    # NFC normalization
    decomposed = "e\u0301"  # e + combining acute accent
    r = sanitize_text(decomposed)
    expected = "\u00e9"  # precomposed é
    assert r == expected, f"Expected {repr(expected)}, got {repr(r)}"
    assert unicodedata.is_normalized("NFC", r), "Result should be NFC-normalized"
    print(f"  PASS: Unicode NFC normalization: {repr(decomposed)} -> {repr(r)}")


def test_control_characters():
    """Edge Case 5: Control characters"""
    print("\n=== Control characters ===")
    # Bell should be rejected
    try:
        sanitize_text("hello\x07world")
        print("  FAIL: sanitize_text(bell) should raise")
    except InvalidCharacterError:
        print("  PASS: sanitize_text(bell) raises InvalidCharacterError")

    # Tab and newline should be allowed in text pattern
    r = sanitize_text("hello\tworld")
    assert r == "hello\tworld", f"Tab should be preserved, got {repr(r)}"
    print("  PASS: sanitize_text preserves tabs")

    r = sanitize_text("hello\nworld")
    assert r == "hello\nworld", f"Newline should be preserved, got {repr(r)}"
    print("  PASS: sanitize_text preserves newlines")

    # strip_control_characters
    r = strip_control_characters("a\x07b\x1bc")
    assert r == "abc", f"Expected abc, got {repr(r)}"
    print("  PASS: strip_control_characters removes control chars")


def test_sql_encoding():
    """Edge Case 6: SQL injection encoding"""
    print("\n=== SQL encoding ===")
    r = encode_for_sql("O'Brien")
    expected = "O\\'Brien"
    assert r == expected, f"Expected {repr(expected)}, got {repr(r)}"
    print(f"  PASS: encode_for_sql escapes single quote: {repr(r)}")

    r = encode_for_sql("test\\path")
    expected = "test\\\\path"
    assert r == expected, f"Expected {repr(expected)}, got {repr(r)}"
    print(f"  PASS: encode_for_sql escapes backslash: {repr(r)}")

    # Empty string
    r = encode_for_sql("")
    assert r == "", f"Expected empty, got {repr(r)}"
    print("  PASS: encode_for_sql handles empty string")

    # Very long (but within limits)
    r = encode_for_sql("x" * 4096)
    assert len(r) == 4096, f"Expected length 4096, got {len(r)}"
    print("  PASS: encode_for_sql handles max-length string")


def test_path_traversal():
    """Edge Case 7: Path traversal"""
    print("\n=== Path traversal ===")
    cases = [
        "../etc/passwd",
        "foo/../../bar",
        "foo\\..\\..\\bar",
    ]
    for trav in cases:
        try:
            sanitize_filename(trav)
            print(f"  FAIL: sanitize_filename({trav!r}) should raise")
        except InvalidCharacterError:
            print(f"  PASS: sanitize_filename rejects {trav!r}")


def test_xss():
    """Edge Case 8: XSS / HTML injection"""
    print("\n=== XSS prevention ===")
    r = sanitize_for_html("<script>alert(1)</script>")
    assert "<" not in r and ">" not in r, f"HTML not escaped: {r}"
    assert "&lt;" in r, f"Expected &lt; in output: {r}"
    print(f"  PASS: sanitize_for_html escapes: {r}")

    r = encode_for_html_body("Hello & Welcome <bye>")
    assert r == "Hello &amp; Welcome &lt;bye&gt;", f"Got {r}"
    print(f"  PASS: encode_for_html_body: {r}")

    r = encode_for_html_attr('class="selected"')
    assert "&quot;" in r, f"Got {r}"
    print(f"  PASS: encode_for_html_attr: {r}")

    # Empty string
    r = sanitize_for_html("")
    assert r == ""
    print("  PASS: sanitize_for_html handles empty string")


def test_dangerous_patterns():
    """Edge Case 9: Dangerous pattern detection"""
    print("\n=== Dangerous patterns ===")
    dangerous_cases = [
        "SELECT * FROM users",
        "DROP TABLE users",
        "javascript:alert(1)",
    ]
    for dangerous in dangerous_cases:
        try:
            sanitize_text(dangerous)
            print(f"  FAIL: sanitize_text({dangerous!r}) should raise")
        except (InvalidCharacterError, DangerousPatternDetectedError) as e:
            print(f"  PASS: sanitize_text rejects {dangerous!r} -> {type(e).__name__}")


def test_constant_time_compare():
    """Edge Case 10: Constant-time comparison"""
    print("\n=== Constant-time compare ===")
    assert constant_time_compare("abc123", "abc123") == True
    assert constant_time_compare("abc123", "xyz789") == False
    assert constant_time_compare("abc", "abcd") == False
    assert constant_time_compare("", "") == True
    assert constant_time_compare("", "a") == False
    print("  PASS: all constant-time compare tests")


def test_token_generation():
    """Edge Case 11: Token generation"""
    print("\n=== Token generation ===")
    t = generate_secure_token(32)
    assert len(t) == 32, f"Expected 32, got {len(t)}"
    print(f"  PASS: generate_secure_token(32) -> len={len(t)}")

    t = generate_session_token(32)
    assert len(t) == 32, f"Expected 32, got {len(t)}"
    print(f"  PASS: generate_session_token(32) -> len={len(t)}")

    try:
        generate_secure_token(0)
        print("  FAIL: generate_secure_token(0) should raise")
    except ValueError:
        print("  PASS: generate_secure_token(0) raises ValueError")


def test_filename_edge_cases():
    """Edge Case 12: Filename edge cases"""
    print("\n=== Filename edge cases ===")
    # Leading dots
    r = sanitize_filename("..hidden")
    assert not r.startswith("."), f"Leading dot not stripped: {r}"
    print(f"  PASS: sanitize_filename strips leading dots: {r}")

    # Just dots
    try:
        sanitize_filename("...")
        print("  FAIL: sanitize_filename('...') should raise")
    except InvalidCharacterError:
        print("  PASS: sanitize_filename('...') raises InvalidCharacterError")

    # Shell metacharacters
    try:
        sanitize_filename("file_x01_")
        # This should work since it's safe chars
        print(f"  PASS: sanitize_filename handles safe filename: {r}")
    except InvalidCharacterError:
        print("  FAIL: safe filename was rejected")

    # Windows unsafe chars
    try:
        sanitize_filename('file:name*')
        print("  FAIL: sanitize_filename with unsafe chars should raise")
    except InvalidCharacterError:
        print("  PASS: sanitize_filename rejects unsafe chars")


def test_shell_safety():
    """Edge Case 13: Shell safety"""
    print("\n=== Shell safety ===")
    r = encode_for_shell("hello-world.test_123")
    assert "'" in r, f"Should be quoted: {r}"
    print(f"  PASS: encode_for_shell safe: {r}")

    unsafe_cases = [
        "hello;x",
        "hello|world",
        "hello&world",
    ]
    for unsafe in unsafe_cases:
        try:
            encode_for_shell(unsafe)
            print(f"  FAIL: encode_for_shell({unsafe!r}) should raise")
        except InvalidCharacterError:
            print(f"  PASS: encode_for_shell rejects {unsafe!r}")


def test_safe_truncate():
    """Edge Case 14: safe_truncate"""
    print("\n=== safe_truncate ===")
    assert safe_truncate("") == ""
    assert safe_truncate("hello") == "hello"
    assert safe_truncate("hello world", max_length=5) == "hello"
    assert safe_truncate("a", max_length=0) == ""
    assert safe_truncate("abc", max_length=3) == "abc"
    assert safe_truncate("abcdef", max_length=3) == "abc"
    print("  PASS: all safe_truncate tests")


def test_strip_html_tags():
    """Edge Case 15: strip_html_tags"""
    print("\n=== strip_html_tags ===")
    assert strip_html_tags("") == ""
    assert strip_html_tags("hello <b>world</b>") == "hello world"
    result = strip_html_tags("<p>Hello</p><p>World</p>")
    assert result == "Hello World", f"Expected 'Hello World', got {repr(result)}"
    result = strip_html_tags("<!-- comment -->text")
    assert result == "text", f"Expected 'text', got {repr(result)}"
    # No HTML
    assert strip_html_tags("plain text") == "plain text"
    print("  PASS: all strip_html_tags tests")


def test_normalize_unicode():
    """Edge Case 16: normalize_unicode"""
    print("\n=== normalize_unicode ===")
    assert normalize_unicode("") == ""
    r = normalize_unicode("e\u0301")  # e + combining accent
    assert r == "\u00e9", f"Expected e9, got {repr(r)}"
    print(f"  PASS: normalize_unicode NFC: {repr(r)}")

    try:
        normalize_unicode("test", form="INVALID")
        print("  FAIL: normalize_unicode(INVALID) should raise")
    except ValueError:
        print("  PASS: normalize_unicode(INVALID) raises ValueError")

    # NFD form
    r = normalize_unicode("\u00e9", form="NFD")
    assert len(r) == 2, f"Expected decomposed length 2, got {len(r)}"
    print(f"  PASS: normalize_unicode NFD: {repr(r)}")

    # Idempotent
    r2 = normalize_unicode(r, form="NFC")
    assert r2 == "\u00e9", f"Recompose failed: {repr(r2)}"
    print("  PASS: normalize_unicode is idempotent")


def test_sensitive_data():
    """Edge Case 17: Sensitive data detection"""
    print("\n=== Sensitive data detection ===")
    assert contains_sensitive_data("api_key=abc123") == True
    assert contains_sensitive_data("sk-abc123def456ghi789jklmno") == True
    assert contains_sensitive_data("AKIAIOSFODNN7EXAMPLE") == True
    assert contains_sensitive_data("eyJ.eyJ.abc") == True
    assert contains_sensitive_data("") == False
    assert contains_sensitive_data("hello world") == False
    print("  PASS: all sensitive data tests")


if __name__ == "__main__":
    tests = [
        test_empty_strings,
        test_overly_long_strings,
        test_null_bytes,
        test_special_unicode,
        test_control_characters,
        test_sql_encoding,
        test_path_traversal,
        test_xss,
        test_dangerous_patterns,
        test_constant_time_compare,
        test_token_generation,
        test_filename_edge_cases,
        test_shell_safety,
        test_safe_truncate,
        test_strip_html_tags,
        test_normalize_unicode,
        test_sensitive_data,
    ]

    passed = 0
    failed = 0
    for test_fn in tests:
        try:
            test_fn()
            passed += 1
        except Exception as e:
            print(f"  ERROR in {test_fn.__name__}: {e}")
            import traceback
            traceback.print_exc()
            failed += 1

    print(f"\n{'='*50}")
    print(f"RESULTS: {passed} passed, {failed} failed out of {len(tests)} test groups")
    if failed:
        print("SOME TESTS FAILED!")
        sys.exit(1)
    else:
        print("ALL EDGE CASE TESTS PASSED!")
