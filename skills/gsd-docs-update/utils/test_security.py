"""Edge-case tests for utils security module."""
import sys
import os
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from utils import *

passed = 0
failed = 0

def test(name, ok, detail=""):
    global passed, failed
    if ok:
        passed += 1
        print(f"  PASS: {name}")
    else:
        failed += 1
        print(f"  FAIL: {name}  {detail}")

# ── Edge Case 1: Empty strings ──────────────────────────────────────────
print("\n═══ Empty strings ═══")

try:
    sanitize_identifier("")
    test("sanitize_identifier('') should raise", False, "no exception raised")
except InputTooShortError:
    test("sanitize_identifier('') raises InputTooShortError", True)
except SecurityInputError as e:
    test("sanitize_identifier('') raises SecurityInputError", True, f"type={type(e).__name__}")

try:
    sanitize_slug("")
    test("sanitize_slug('') should raise", False, "no exception raised")
except InvalidCharacterError:
    test("sanitize_slug('') raises InvalidCharacterError", True)
except SecurityInputError:
    test("sanitize_slug('') raises SecurityInputError", True)

try:
    sanitize_filename("")
    test("sanitize_filename('') should raise", False, "no exception raised")
except InvalidCharacterError:
    test("sanitize_filename('') raises InvalidCharacterError", True)

# Empty text should be ok with default min_length=0
r = sanitize_text("")
test(f"sanitize_text('') returns empty string: repr={repr(r)}", r == "")

# ── Edge Case 2: Overly long strings ────────────────────────────────────
print("\n═══ Overly long strings ═══")

try:
    sanitize_identifier("a" * 200)
    test("sanitize_identifier('a'*200) should raise", False)
except InputTooLongError:
    test("sanitize_identifier('a'*200) raises InputTooLongError", True)

try:
    sanitize_text("x" * 5000)
    test("sanitize_text('x'*5000) should raise", False)
except InputTooLongError:
    test("sanitize_text('x'*5000) raises InputTooLongError", True)

try:
    sanitize_slug("x" * 200)
    test("sanitize_slug('x'*200) should raise", False)
except InputTooLongError:
    test("sanitize_slug('x'*200) raises InputTooLongError", True)

try:
    sanitize_filename("x" * 300)
    test("sanitize_filename('x'*300) should raise", False)
except InputTooLongError:
    test("sanitize_filename('x'*300) raises InputTooLongError", True)

# ── Edge Case 3: Null bytes ─────────────────────────────────────────────
print("\n═══ Null bytes ═══")

# Null bytes should be rejected by whitelist validation
try:
    sanitize_identifier("hello\x00world")
    test("sanitize_identifier with null byte should raise", False)
except InvalidCharacterError:
    test("sanitize_identifier with null byte raises InvalidCharacterError", True)

try:
    sanitize_text("hello\x00world")
    test("sanitize_text with null byte should raise", False)
except InvalidCharacterError:
    test("sanitize_text with null byte raises InvalidCharacterError", True)

# strip_control_characters should remove null bytes
r = strip_control_characters("hello\x00world")
test(f"strip_control_characters removes null bytes: {repr(r)}", r == "helloworld")

# sanitize_filename should strip null bytes
r = sanitize_filename("file\x00name.txt")
test(f"sanitize_filename strips null bytes: {repr(r)}", r == "filename.txt")

# ── Edge Case 4: Special Unicode characters ─────────────────────────────
print("\n═══ Special Unicode characters ═══")

# Zero-width space (U+200B) should be rejected
try:
    sanitize_identifier("\u200b")
    test("sanitize_identifier(zero-width) should raise", False)
except InvalidCharacterError:
    test("sanitize_identifier(zero-width) raises InvalidCharacterError", True)

# Right-to-left override (U+202E) should be rejected
try:
    sanitize_identifier("hello\u202Eworld")
    test("sanitize_identifier(bidi override) should raise", False)
except InvalidCharacterError:
    test("sanitize_identifier(bidi override) raises InvalidCharacterError", True)

# Unicode homoglyph: Latin 'A' vs Cyrillic 'А'
try:
    sanitize_identifier("admin")  # using Latin 'a'
    sanitize_identifier("аdmin")  # using Cyrillic 'а' (U+0430)
    test("Cyrillic homoglyph in identifier should raise", False)
except InvalidCharacterError:
    test("Cyrillic homoglyph in identifier raises InvalidCharacterError", True)

# Unicode normalization: é can be precomposed (U+00E9) or decomposed (e + U+0301)
decomposed = "e\u0301"  # e + combining acute accent
r = sanitize_text(decomposed)
expected = "\u00e9"  # precomposed é
test(f"Unicode NFC normalization: {repr(decomposed)} -> {repr(r)}", 
     r == expected and unicodedata.is_normalized("NFC", r))

# ── Edge Case 5: Control characters ─────────────────────────────────────
print("\n═══ Control characters ═══")

for ctrl_name, ctrl_char in [("bell", "\x07"), ("tab", "\t"), ("newline", "\n"), ("escape", "\x1b")]:
    try:
        sanitize_text(f"hello{ctrl_char}world")
        if ctrl_char in ("\t", "\n"):
            # Tab and newline are in SAFE_TEXT_PATTERN via \s
            test(f"sanitize_text with {ctrl_name} (should be allowed in text)", True)
        else:
            test(f"sanitize_text with {ctrl_name} should raise", False)
    except InvalidCharacterError:
        if ctrl_char in ("\t", "\n"):
            test(f"sanitize_text with {ctrl_name} (should be allowed in text)", False, "should not raise for whitespace")
        else:
            test(f"sanitize_text with {ctrl_name} raises InvalidCharacterError", True)

# strip_control_characters
r = strip_control_characters("a\x07b\x1bc")
test(f"strip_control_characters: {repr(r)}", r == "abc")

# ── Edge Case 6: SQL injection variants ─────────────────────────────────
print("\n═══ SQL injection ═══")

# encode_for_sql should escape single quotes and backslashes
r = encode_for_sql("O'Brien")
test(f"encode_for_sql escapes single quote: {repr(r)}", r == "O\\'Brien")

r = encode_for_sql("test\\path")
test(f"encode_for_sql escapes backslash: {repr(r)}", r == "test\\\\path")

# ── Edge Case 7: Path traversal ────────────────────────────────────────
print("\n═══ Path traversal ═══")

for trav in ["../etc/passwd", "..\\windows\\system32", "foo/../../bar"]:
    try:
        sanitize_filename(trav)
        test(f"sanitize_filename('{trav}') should raise", False)
    except InvalidCharacterError:
        test(f"sanitize_filename('{trav}') raises InvalidCharacterError", True)

# ── Edge Case 8: HTML injection (XSS) ───────────────────────────────────
print("\n═══ HTML injection (XSS) ═══")

r = sanitize_for_html("<script>alert('xss')</script>")
expected = "&lt;script&gt;alert(&#x27;xss&#x27;)&lt;/script&gt;"
test(f"sanitize_for_html escapes HTML: {r}", r == expected)

r = encode_for_html_body("Hello & Welcome <bye>")
expected = "Hello &amp; Welcome &lt;bye&gt;"
test(f"encode_for_html_body: {r}", r == expected)

r = encode_for_html_attr('class="selected"')
expected = 'class=&quot;selected&quot;'
test(f"encode_for_html_attr: {r}", r == expected)

# ── Edge Case 9: Dangerous pattern detection ────────────────────────────
print("\n═══ Dangerous patterns ═══")

for dangerous in [
    "SELECT * FROM users",
    "DROP TABLE users",
    "<img src=x onerror=alert(1)>",
    "javascript:alert(1)",
]:
    try:
        sanitize_text(dangerous)
        test(f"sanitize_text('{dangerous[:20]}...') should raise", False)
    except DangerousPatternDetectedError:
        test(f"sanitize_text detects dangerous pattern: '{dangerous[:20]}...'", True)
    except InvalidCharacterError:
        test(f"sanitize_text rejects via whitelist: '{dangerous[:20]}...'", True)

# ── Edge Case 10: Constant-time compare ─────────────────────────────────
print("\n═══ Constant-time compare ═══")

test("const-time equal strings", constant_time_compare("abc123", "abc123") == True)
test("const-time different strings", constant_time_compare("abc123", "xyz789") == False)
test("const-time different lengths", constant_time_compare("abc", "abcd") == False)
test("const-time empty strings", constant_time_compare("", "") == True)
test("const-time empty vs non-empty", constant_time_compare("", "a") == False)

# ── Edge Case 11: Token generation ──────────────────────────────────────
print("\n═══ Token generation ═══")

t = generate_secure_token(32)
test(f"generate_secure_token(32) length={len(t)}", len(t) == 32)

t = generate_session_token(32)
test(f"generate_session_token(32) length={len(t)}", len(t) == 32)

try:
    generate_secure_token(0)
    test("generate_secure_token(0) should raise", False)
except ValueError:
    test("generate_secure_token(0) raises ValueError", True)

# ── Edge Case 12: Filename edge cases ───────────────────────────────────
print("\n═══ Filename edge cases ═══")

# Leading dots
r = sanitize_filename("..hidden")
test(f"sanitize_filename strips leading dots: {r}", not r.startswith("."))

# Just dots
try:
    sanitize_filename("...")
    test("sanitize_filename('...') should raise", False)
except InvalidCharacterError:
    test("sanitize_filename('...') raises InvalidCharacterError", True)

# Shell metacharacters in filename
try:
    sanitize_filename("file; rm -rf /")
    test("sanitize_filename with shell chars should raise", False)
except InvalidCharacterError:
    test("sanitize_filename with shell chars raises InvalidCharacterError", True)

# ── Edge Case 13: Shell safe validation ─────────────────────────────────
print("\n═══ Shell safety ═══")

r = encode_for_shell("hello-world.test_123")
test(f"encode_for_shell safe chars: {r}", "'hello-world.test_123'" in r or r == "hello-world.test_123")

for unsafe in ["hello; rm", "hello$(id)", "`id`", "hello|world", "hello&world"]:
    try:
        encode_for_shell(unsafe)
        test(f"encode_for_shell('{unsafe}') should raise", False)
    except InvalidCharacterError:
        test(f"encode_for_shell rejects '{unsafe}'", True)

# ── Edge Case 14: contains_sensitive_data ───────────────────────────────
print("\n═══ Sensitive data detection ═══")

test("Detects API key pattern", contains_sensitive_data("api_key=abc123def456") == True)
test("Detects OpenAI key", contains_sensitive_data("sk-abc123def456ghi789jklmno") == True)
test("Detects AWS key", contains_sensitive_data("AKIAIOSFODNN7EXAMPLE") == True)
test("Detects JWT", contains_sensitive_data("eyJ.eyJ.abc") == True)
test("No false positive on safe text", contains_sensitive_data("hello world") == False)

# ── Summary ─────────────────────────────────────────────────────────────
print(f"\n{'='*50}")
print(f"RESULTS: {passed} passed, {failed} failed out of {passed+failed} tests")
if failed:
    print("SOME TESTS FAILED!")
    sys.exit(1)
else:
    print("ALL TESTS PASSED!")
