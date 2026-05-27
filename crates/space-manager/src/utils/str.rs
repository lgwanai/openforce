//! String processing utilities.
//!
//! This module provides common string manipulation functions such as
//! trimming, reversing, splitting, joining, and case conversion.
//! All functions are designed to operate on string slices and return
//! owned `String` values.
//!
//! Unicode safety is a first-class concern: all functions operate on
//! `char` boundaries (not byte boundaries), so multi-byte UTF-8 characters,
//! emoji, and non-Latin scripts are handled correctly.
//!
//! Case conversion functions return [`Result`] types to provide safe
//! error handling for invalid inputs (e.g., empty strings after trimming,
//! or byte sequences that are not valid UTF-8).

use std::fmt;


// ---------------------------------------------------------------------------
// Error Types
// ---------------------------------------------------------------------------

/// Error type for case conversion operations.
///
/// This enum captures the various invalid-input scenarios that can occur
/// during case and naming-style conversions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaseConversionError {
    /// The input string is empty (or becomes empty after trimming).
    EmptyInput,
    /// The input contains no alphabetic characters, making case conversion
    /// meaningless or lossy.
    NoAlphabeticCharacters,
    /// The input bytes are not valid UTF-8 (only relevant when converting
    /// from raw byte slices).
    InvalidUtf8(Vec<u8>),
    /// The input contains characters that cannot be meaningfully converted.
    UnsupportedCharacter(char),
}

impl fmt::Display for CaseConversionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CaseConversionError::EmptyInput => {
                write!(f, "input string is empty or contains only whitespace")
            }
            CaseConversionError::NoAlphabeticCharacters => {
                write!(
                    f,
                    "input contains no alphabetic characters for case conversion"
                )
            }
            CaseConversionError::InvalidUtf8(bytes) => {
                write!(f, "invalid UTF-8 sequence: {bytes:?}")
            }
            CaseConversionError::UnsupportedCharacter(ch) => {
                write!(f, "unsupported character for case conversion: {ch:?} (U+{:04X})", *ch as u32)
            }
        }
    }
}

impl std::error::Error for CaseConversionError {}

/// Convert a byte slice to `&str`, returning an error if invalid UTF-8.
fn try_str_from_bytes(bytes: &[u8]) -> Result<&str, CaseConversionError> {
    std::str::from_utf8(bytes).map_err(|_| CaseConversionError::InvalidUtf8(bytes.to_vec()))
}

// ---------------------------------------------------------------------------
// Tokenization Helpers
// ---------------------------------------------------------------------------

/// Split a string into "words" (tokens) for case conversion.
///
/// This function splits on:
/// - Underscores (`_`)
/// - Hyphens (`-`)
/// - Spaces (any Unicode whitespace)
/// - CamelCase boundaries (e.g., "XMLParser" → ["XML", "Parser"])
/// - Digit-letter boundaries (e.g., "foo2bar" → ["foo", "2", "bar"])
///
/// Non-alphabetic tokens (pure digits, symbols) are preserved as-is
/// but not lowercased — they act as separators.
fn tokenize(s: &str) -> Vec<String> {
    let s = s.trim();
    if s.is_empty() {
        return vec![];
    }

    let mut tokens: Vec<String> = Vec::new();
    let mut current = String::new();

    // First pass: split on explicit delimiters (_, -, whitespace)
    // We treat each run of non-delimiter chars as a unit, then
    // sub-split those units on camelCase boundaries.
    let mut chars = s.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '_' || ch == '-' || ch.is_whitespace() {
            // Flush current token
            if !current.is_empty() {
                // Sub-split current on camelCase boundaries
                for subtoken in split_camel_case(&current) {
                    if !subtoken.is_empty() {
                        tokens.push(subtoken);
                    }
                }
                current = String::new();
            }
        } else {
            current.push(ch);
        }
    }

    // Flush last token
    if !current.is_empty() {
        for subtoken in split_camel_case(&current) {
            if !subtoken.is_empty() {
                tokens.push(subtoken);
            }
        }
    }

    tokens
}

/// Split a single word on CamelCase boundaries.
///
/// Examples:
/// - "camelCase" → ["camel", "Case"]
/// - "XMLParser" → ["XML", "Parser"]
/// - "HTTPServer" → ["HTTP", "Server"]
/// - "isASCII" → ["is", "ASCII"]
/// - "fooBar123" → ["foo", "Bar", "123"]
fn split_camel_case(word: &str) -> Vec<String> {
    if word.is_empty() {
        return vec![];
    }

    let mut tokens: Vec<String> = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = word.chars().collect();
    let len = chars.len();

    for i in 0..len {
        let ch = chars[i];
        let is_upper = ch.is_uppercase();
        let is_lower = ch.is_lowercase();
        let is_digit = ch.is_ascii_digit();
        let is_alpha = is_upper || is_lower;
        let is_non_alpha = !is_alpha && !is_digit;

        if is_non_alpha {
            // Non-alphabetic, non-digit: treat as token boundary
            if !current.is_empty() {
                tokens.push(current.clone());
                current.clear();
            }
            current.push(ch);
            tokens.push(current.clone());
            current.clear();
            continue;
        }

        if i > 0 {
            let prev = chars[i - 1];

            // Transition: lowercase → uppercase (camelCase boundary)
            if is_upper && prev.is_lowercase() && !prev.is_uppercase() {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
            }

            // Transition: digit → letter
            if is_alpha && prev.is_ascii_digit() {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
            }

            // Transition: letter → digit
            if is_digit && prev.is_alphabetic() {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
            }

            // Transition: uppercase → uppercase followed by lowercase
            // e.g., "XMLParser": at 'P', prev is 'L' (both upper),
            // but next is 'a' (lower) → split before 'P'
            if is_lower && prev.is_uppercase() && !current.is_empty() {
                // Check if current has >1 uppercase char (an acronym)
                let all_upper = current.chars().all(|c| c.is_uppercase());
                if all_upper && current.len() > 1 {
                    // Pop last uppercase char(s) - actually we need to check
                    // if the previous group was an acronym
                    // Simple heuristic: if current ends with uppercase and
                    // we're at a lowercase transition, split before the last uppercase
                    let mut temp = current.clone();
                    let last = temp.pop();
                    if let Some(lc) = last {
                        if lc.is_uppercase() && !temp.is_empty() {
                            // The last char of current is uppercase;
                            // it belongs to the new token
                            current = temp;
                            tokens.push(current.clone());
                            current = lc.to_string();
                            current.push(ch);
                            continue;
                        }
                    }
                }
            }
        }

        // Simple digit groups: keep consecutive digits together
        current.push(ch);
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

// ---------------------------------------------------------------------------
// Trim Whitespace
// ---------------------------------------------------------------------------

/// Trims leading and trailing whitespace from a string slice.
///
/// Removes all Unicode whitespace characters from both ends of the input,
/// including spaces, tabs, and newlines. Delegates to [`str::trim`] for
/// correct Unicode whitespace handling.
///
/// # Arguments
///
/// * `s` - A string slice to trim.
///
/// # Returns
///
/// A new `String` with whitespace removed from both ends.
/// Returns an empty `String` if the input is entirely whitespace.
///
/// # Panics
///
/// This function does not panic.
///
/// # Examples
///
/// ```rust
/// use openforce_space_manager::utils::str::trim_whitespace;
///
/// assert_eq!(trim_whitespace("  hello  "), "hello");
/// assert_eq!(trim_whitespace("\n\tworld\n"), "world");
/// assert_eq!(trim_whitespace(""), "");
/// assert_eq!(trim_whitespace("   "), "");
/// ```
pub fn trim_whitespace(s: &str) -> String {
    s.trim().to_string()
}

// ---------------------------------------------------------------------------
// Reverse
// ---------------------------------------------------------------------------

/// Reverses a string by its Unicode scalar values.
///
/// Processes the string at the `char` level (Unicode scalar values),
/// which correctly handles multi-byte characters like CJK ideographs
/// and emoji.
///
/// # Arguments
///
/// * `s` - A string slice to reverse.
///
/// # Returns
///
/// A new `String` containing the `char`s of the input in reverse order.
/// Returns an empty `String` if the input is empty.
///
/// # Panics
///
/// This function does not panic.
///
/// # Examples
///
/// ```rust
/// use openforce_space_manager::utils::str::reverse;
///
/// assert_eq!(reverse("hello"), "olleh");
/// assert_eq!(reverse("rust"), "tsur");
/// assert_eq!(reverse(""), "");
///
/// // Unicode-safe: multi-byte characters are preserved
/// assert_eq!(reverse("你好"), "好你");
/// assert_eq!(reverse("a😀b"), "b😀a");
/// ```
pub fn reverse(s: &str) -> String {
    s.chars().rev().collect()
}

// ---------------------------------------------------------------------------
// Split / Join
// ---------------------------------------------------------------------------

/// Splits a string by a given character delimiter.
///
/// Each occurrence of the delimiter character is used as a splitting
/// point. Consecutive delimiters produce empty strings in the result.
/// The delimiter itself is not included in any of the returned substrings.
///
/// # Arguments
///
/// * `s` - A string slice to split.
/// * `delimiter` - A `char` acting as the split boundary. Unicode characters
///   are supported (e.g., non-ASCII delimiters).
///
/// # Returns
///
/// A `Vec<String>` containing the substrings between delimiters.
///
/// # Panics
///
/// This function does not panic.
///
/// # Examples
///
/// ```rust
/// use openforce_space_manager::utils::str::split_by_char;
///
/// let parts = split_by_char("a,b,c", ',');
/// assert_eq!(parts, vec!["a", "b", "c"]);
///
/// let parts = split_by_char("x--y", '-');
/// assert_eq!(parts, vec!["x", "", "y"]);
///
/// // No delimiter found → single-element vector
/// let parts = split_by_char("hello", ',');
/// assert_eq!(parts, vec!["hello"]);
///
/// // Empty input
/// let parts = split_by_char("", ',');
/// assert_eq!(parts, vec![""]);
/// ```
pub fn split_by_char(s: &str, delimiter: char) -> Vec<String> {
    s.split(delimiter).map(|part| part.to_string()).collect()
}

/// Joins a slice of string slices into a single `String`, separated by
/// a given separator.
///
/// Delegates to [`slice::join`] for efficient concatenation.
///
/// # Arguments
///
/// * `parts` - A slice of string slices to join.
/// * `separator` - A string slice placed between each pair of adjacent parts.
///
/// # Returns
///
/// A new `String` with all parts concatenated, separated by `separator`.
/// Returns an empty `String` if `parts` is empty.
///
/// # Panics
///
/// This function does not panic.
///
/// # Examples
///
/// ```rust
/// use openforce_space_manager::utils::str::join_strings;
///
/// let words = ["hello", "world"];
/// assert_eq!(join_strings(&words, " "), "hello world");
///
/// let empty: [&str; 0] = [];
/// assert_eq!(join_strings(&empty, ","), "");
///
/// // Single element — no separator added
/// assert_eq!(join_strings(&["only"], ", "), "only");
/// ```
pub fn join_strings(parts: &[&str], separator: &str) -> String {
    parts.join(separator)
}

// ---------------------------------------------------------------------------
// Case Conversion: Core Helpers
// ---------------------------------------------------------------------------

/// Validate that input is suitable for case conversion.
///
/// Returns:
/// - `Err(EmptyInput)` if the string is empty or whitespace-only.
/// - `Ok(true)` if there is at least one alphabetic character.
/// - `Ok(false)` if there are characters but none are alphabetic.
fn validate_case_input(s: &str) -> Result<bool, CaseConversionError> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Err(CaseConversionError::EmptyInput);
    }
    Ok(trimmed.chars().any(|c| c.is_alphabetic()))
}

/// Convert a byte slice to camelCase, returning an error if invalid UTF-8.
///
/// This is the byte-level variant that can fail with `InvalidUtf8`.
pub fn to_camel_case_bytes(bytes: &[u8]) -> Result<String, CaseConversionError> {
    let s = try_str_from_bytes(bytes)?;
    to_camel_case(s)
}

/// Convert a string slice to **camelCase**.
///
/// The first word is lowercased; each subsequent word is capitalized.
/// Non-alphabetic separator characters (underscores, hyphens, spaces)
/// are removed. CamelCase boundaries in the input are also split and
/// re-joined.
///
/// # Arguments
///
/// * `s` - A string slice to convert.
///
/// # Returns
///
/// `Ok(String)` in camelCase format, or `Err(CaseConversionError)` if
/// the input is empty or contains no alphabetic characters.
///
/// # Errors
///
/// - [`CaseConversionError::EmptyInput`]: The input is empty or whitespace-only.
/// - [`CaseConversionError::NoAlphabeticCharacters`]: No alphabetic chars found.
///
/// # Panics
///
/// This function does not panic.
///
/// # Examples
///
/// ```rust
/// use openforce_space_manager::utils::str::to_camel_case;
///
/// assert_eq!(to_camel_case("hello world").unwrap(), "helloWorld");
/// assert_eq!(to_camel_case("user_name").unwrap(), "userName");
/// assert_eq!(to_camel_case("kebab-case").unwrap(), "kebabCase");
/// assert_eq!(to_camel_case("XML Parser").unwrap(), "xmlParser");
/// assert_eq!(to_camel_case("").unwrap_err().to_string(), "input string is empty or contains only whitespace");
/// ```
pub fn to_camel_case(s: &str) -> Result<String, CaseConversionError> {
    let has_alpha = validate_case_input(s)?;
    if !has_alpha {
        return Err(CaseConversionError::NoAlphabeticCharacters);
    }

    let tokens = tokenize(s);
    if tokens.is_empty() {
        return Err(CaseConversionError::EmptyInput);
    }

    let mut result = String::new();
    for (i, token) in tokens.iter().enumerate() {
        if i == 0 {
            // First word: lowercase
            result.push_str(&token.to_lowercase());
        } else {
            // Subsequent words: capitalize first letter
            result.push_str(&capitalize_first(token));
        }
    }

    Ok(result)
}

/// Convert a byte slice to **PascalCase**, returning an error if invalid UTF-8.
///
/// This is the byte-level variant that can fail with `InvalidUtf8`.
pub fn to_pascal_case_bytes(bytes: &[u8]) -> Result<String, CaseConversionError> {
    let s = try_str_from_bytes(bytes)?;
    to_pascal_case(s)
}

/// Convert a string slice to **PascalCase** (UpperCamelCase).
///
/// Every word is capitalized and joined without separators.
///
/// # Arguments
///
/// * `s` - A string slice to convert.
///
/// # Returns
///
/// `Ok(String)` in PascalCase format, or `Err(CaseConversionError)` if
/// the input is empty or contains no alphabetic characters.
///
/// # Errors
///
/// - [`CaseConversionError::EmptyInput`]: The input is empty or whitespace-only.
/// - [`CaseConversionError::NoAlphabeticCharacters`]: No alphabetic chars found.
///
/// # Examples
///
/// ```rust
/// use openforce_space_manager::utils::str::to_pascal_case;
///
/// assert_eq!(to_pascal_case("hello world").unwrap(), "HelloWorld");
/// assert_eq!(to_pascal_case("user_name").unwrap(), "UserName");
/// assert_eq!(to_pascal_case("").unwrap_err().to_string(), "input string is empty or contains only whitespace");
/// ```
pub fn to_pascal_case(s: &str) -> Result<String, CaseConversionError> {
    let has_alpha = validate_case_input(s)?;
    if !has_alpha {
        return Err(CaseConversionError::NoAlphabeticCharacters);
    }

    let tokens = tokenize(s);
    if tokens.is_empty() {
        return Err(CaseConversionError::EmptyInput);
    }

    let result: String = tokens.iter().map(|t| capitalize_first(t)).collect();
    Ok(result)
}

/// Capitalizes the first alphabetic character of a string, leaving the
/// rest unchanged.
///
/// This is a building block for case conversion. Non-alphabetic leading
/// characters are preserved; the first alphabetic character found is
/// uppercased.
///
/// # Arguments
///
/// * `s` - A string slice.
///
/// # Returns
///
/// A `String` with the first alphabetic character capitalized.
/// Returns an empty string if the input is empty.
///
/// # Panics
///
/// This function does not panic.
///
/// # Examples
///
/// ```rust
/// use openforce_space_manager::utils::str::capitalize_first;
///
/// assert_eq!(capitalize_first("hello"), "Hello");
/// assert_eq!(capitalize_first("world"), "World");
/// assert_eq!(capitalize_first(""), "");
/// assert_eq!(capitalize_first("123abc"), "123Abc");
/// assert_eq!(capitalize_first("über"), "Über");
/// ```
pub fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => {
            // Find the first alphabetic character to capitalize
            if first.is_alphabetic() {
                // Capitalize the first character
                let capitalized: String = first.to_uppercase().collect();
                capitalized + chars.as_str()
            } else {
                // First char is not alphabetic; search for the first
                // alphabetic char in the rest
                let mut result = String::new();
                result.push(first);
                for ch in chars {
                    if ch.is_alphabetic() {
                        let capitalized: String = ch.to_uppercase().collect();
                        result.push_str(&capitalized);
                        // Continue collecting remaining chars as-is
                        // We need to get the rest after ch
                        break;
                    } else {
                        result.push(ch);
                    }
                }
                result
            }
        }
    }
}

/// Capitalizes the first alphabetic character of a string, returning
/// `None` if the input is empty.
///
/// This is the `Option`-returning variant for callers who prefer
/// pattern matching over an empty string check.
///
/// # Arguments
///
/// * `s` - A string slice.
///
/// # Returns
///
/// `Some(String)` with the first alphabetic character capitalized,
/// or `None` if the input is empty.
///
/// # Examples
///
/// ```rust
/// use openforce_space_manager::utils::str::try_capitalize_first;
///
/// assert_eq!(try_capitalize_first("hello"), Some("Hello".to_string()));
/// assert_eq!(try_capitalize_first(""), None);
/// ```
pub fn try_capitalize_first(s: &str) -> Option<String> {
    if s.is_empty() {
        return None;
    }
    Some(capitalize_first(s))
}

// ---------------------------------------------------------------------------
// to_snake_case
// ---------------------------------------------------------------------------

/// Convert a byte slice to **snake_case**, returning an error if invalid UTF-8.
///
/// This is the byte-level variant that can fail with `InvalidUtf8`.
pub fn to_snake_case_bytes(bytes: &[u8]) -> Result<String, CaseConversionError> {
    let s = try_str_from_bytes(bytes)?;
    to_snake_case(s)
}

/// Converts a string to **snake_case**.
///
/// Words are separated by underscores and lowercased.
/// CamelCase boundaries, spaces, hyphens, and underscores are all
/// treated as word separators.
///
/// # Arguments
///
/// * `s` - A string slice to convert.
///
/// # Returns
///
/// `Ok(String)` in snake_case format, or `Err(CaseConversionError)` if
/// the input is empty or contains no alphabetic characters.
///
/// # Errors
///
/// - [`CaseConversionError::EmptyInput`]: The input is empty or whitespace-only.
/// - [`CaseConversionError::NoAlphabeticCharacters`]: No alphabetic chars found.
///
/// # Panics
///
/// This function does not panic.
///
/// # Examples
///
/// ```rust
/// use openforce_space_manager::utils::str::to_snake_case;
///
/// assert_eq!(to_snake_case("Hello World").unwrap(), "hello_world");
/// assert_eq!(to_snake_case("user-name").unwrap(), "user_name");
/// assert_eq!(to_snake_case("camelCase").unwrap(), "camel_case");
/// assert_eq!(to_snake_case("  Camel Case  ").unwrap(), "camel_case");
/// assert_eq!(to_snake_case("XMLParser").unwrap(), "xml_parser");
/// assert!(to_snake_case("").is_err());
/// ```
pub fn to_snake_case(s: &str) -> Result<String, CaseConversionError> {
    let has_alpha = validate_case_input(s)?;
    let tokens = tokenize(s);
    if tokens.is_empty() {
        return Err(CaseConversionError::EmptyInput);
    }
    if !has_alpha {
        return Err(CaseConversionError::NoAlphabeticCharacters);
    }

    let result: Vec<String> = tokens.iter().map(|t| t.to_lowercase()).collect();
    Ok(result.join("_"))
}

/// A non-fallible version of `to_snake_case` that returns a plain `String`.
///
/// If the input is empty or whitespace-only, an empty string is returned.
/// This is provided for callers that do not need error handling.
///
/// # Examples
///
/// ```rust
/// use openforce_space_manager::utils::str::to_snake_case_lossy;
///
/// assert_eq!(to_snake_case_lossy("Hello World"), "hello_world");
/// assert_eq!(to_snake_case_lossy(""), "");
/// ```
pub fn to_snake_case_lossy(s: &str) -> String {
    to_snake_case(s).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// to_kebab_case
// ---------------------------------------------------------------------------

/// Convert a byte slice to **kebab-case**, returning an error if invalid UTF-8.
///
/// This is the byte-level variant that can fail with `InvalidUtf8`.
pub fn to_kebab_case_bytes(bytes: &[u8]) -> Result<String, CaseConversionError> {
    let s = try_str_from_bytes(bytes)?;
    to_kebab_case(s)
}

/// Converts a string to **kebab-case**.
///
/// Words are separated by hyphens and lowercased.
/// CamelCase boundaries, spaces, underscores, and hyphens are all
/// treated as word separators.
///
/// # Arguments
///
/// * `s` - A string slice to convert.
///
/// # Returns
///
/// `Ok(String)` in kebab-case format, or `Err(CaseConversionError)` if
/// the input is empty or contains no alphabetic characters.
///
/// # Errors
///
/// - [`CaseConversionError::EmptyInput`]: The input is empty or whitespace-only.
/// - [`CaseConversionError::NoAlphabeticCharacters`]: No alphabetic chars found.
///
/// # Panics
///
/// This function does not panic.
///
/// # Examples
///
/// ```rust
/// use openforce_space_manager::utils::str::to_kebab_case;
///
/// assert_eq!(to_kebab_case("Hello World").unwrap(), "hello-world");
/// assert_eq!(to_kebab_case("user_name").unwrap(), "user-name");
/// assert_eq!(to_kebab_case("camelCase").unwrap(), "camel-case");
/// assert_eq!(to_kebab_case("  Camel Case  ").unwrap(), "camel-case");
/// assert_eq!(to_kebab_case("XMLParser").unwrap(), "xml-parser");
/// assert!(to_kebab_case("").is_err());
/// ```
pub fn to_kebab_case(s: &str) -> Result<String, CaseConversionError> {
    let has_alpha = validate_case_input(s)?;
    let tokens = tokenize(s);
    if tokens.is_empty() {
        return Err(CaseConversionError::EmptyInput);
    }
    if !has_alpha {
        return Err(CaseConversionError::NoAlphabeticCharacters);
    }

    let result: Vec<String> = tokens.iter().map(|t| t.to_lowercase()).collect();
    Ok(result.join("-"))
}

/// A non-fallible version of `to_kebab_case` that returns a plain `String`.
///
/// If the input is empty or whitespace-only, an empty string is returned.
/// This is provided for callers that do not need error handling.
///
/// # Examples
///
/// ```rust
/// use openforce_space_manager::utils::str::to_kebab_case_lossy;
///
/// assert_eq!(to_kebab_case_lossy("Hello World"), "hello-world");
/// assert_eq!(to_kebab_case_lossy(""), "");
/// ```
pub fn to_kebab_case_lossy(s: &str) -> String {
    to_kebab_case(s).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Truncate with Ellipsis
// ---------------------------------------------------------------------------

/// Truncates a string to a maximum length (in characters, not bytes),
/// appending an ellipsis ("...") if the string was truncated.
///
/// The ellipsis is counted against `max_len`. Behaviour by boundary:
/// - `max_len` ≥ ellipsis length (3) and string exceeds `max_len`:
///   returns `first_n_chars...` where the total length equals `max_len`.
/// - `max_len` equals ellipsis length (3) and string is longer:
///   returns exactly `"..."` to signal truncation.
/// - `max_len` < ellipsis length (3) and string is longer:
///   returns the first `max_len` characters without ellipsis.
///
/// # Arguments
///
/// * `s` - A string slice to truncate.
/// * `max_len` - The maximum number of **characters** (Unicode scalar values)
///   in the result, including the ellipsis if appended.
///
/// # Returns
///
/// A new `String` no longer than `max_len` characters.
///
/// # Panics
///
/// This function does not panic. If `max_len` is 0, an empty string is returned.
///
/// # Examples
///
/// ```rust
/// use openforce_space_manager::utils::str::truncate_with_ellipsis;
///
/// assert_eq!(truncate_with_ellipsis("hello world", 8), "hello...");
/// assert_eq!(truncate_with_ellipsis("hi", 8), "hi");
/// assert_eq!(truncate_with_ellipsis("hello", 5), "hello");
/// assert_eq!(truncate_with_ellipsis("hello", 4), "h...");
/// assert_eq!(truncate_with_ellipsis("hello", 3), "...");
/// assert_eq!(truncate_with_ellipsis("hello", 1), "h");
/// ```
pub fn truncate_with_ellipsis(s: &str, max_len: usize) -> String {
    if max_len == 0 {
        return String::new();
    }

    let char_count = s.chars().count();
    if char_count <= max_len {
        return s.to_string();
    }

    let ellipsis = "...";
    let ellipsis_len = ellipsis.len(); // 3

    if max_len < ellipsis_len {
        // Not enough room for ellipsis → take first max_len chars
        return s.chars().take(max_len).collect();
    }

    if max_len == ellipsis_len {
        // Exactly enough room for the ellipsis itself → signal truncation
        return ellipsis.to_string();
    }

    // max_len > ellipsis_len: take (max_len - 3) chars + "..."
    let target = max_len - ellipsis_len;
    let truncated: String = s.chars().take(target).collect();
    format!("{truncated}...")
}

// ---------------------------------------------------------------------------
// is_blank
// ---------------------------------------------------------------------------

/// Checks whether a string is empty or consists only of whitespace.
///
/// # Arguments
///
/// * `s` - A string slice to check.
///
/// # Returns
///
/// `true` if the string is empty or contains only Unicode whitespace characters.
///
/// # Panics
///
/// This function does not panic.
///
/// # Examples
///
/// ```rust
/// use openforce_space_manager::utils::str::is_blank;
///
/// assert!(is_blank(""));
/// assert!(is_blank("   "));
/// assert!(is_blank("\n\t"));
/// assert!(!is_blank(" hello "));
/// ```
pub fn is_blank(s: &str) -> bool {
    s.trim().is_empty()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ── trim_whitespace ────────────────────────────────────────────

    #[test]
    fn test_trim_normal() {
        assert_eq!(trim_whitespace("  hello  "), "hello");
        assert_eq!(trim_whitespace("\n\tworld\n"), "world");
        assert_eq!(trim_whitespace("no_trim"), "no_trim");
    }

    #[test]
    fn test_trim_empty_and_whitespace() {
        assert_eq!(trim_whitespace(""), "");
        assert_eq!(trim_whitespace("   "), "");
        assert_eq!(trim_whitespace("\n\t\n"), "");
    }

    #[test]
    fn test_trim_unicode_whitespace() {
        // U+00A0 = non-breaking space
        assert_eq!(trim_whitespace("\u{00A0}hello\u{00A0}"), "hello");
        // U+3000 = ideographic space (CJK)
        assert_eq!(trim_whitespace("\u{3000}世界\u{3000}"), "世界");
    }

    // ── reverse ────────────────────────────────────────────────────

    #[test]
    fn test_reverse_normal() {
        assert_eq!(reverse("hello"), "olleh");
        assert_eq!(reverse("rust"), "tsur");
        assert_eq!(reverse("a"), "a");
        assert_eq!(reverse("12345"), "54321");
    }

    #[test]
    fn test_reverse_empty() {
        assert_eq!(reverse(""), "");
    }

    #[test]
    fn test_reverse_unicode() {
        assert_eq!(reverse("你好"), "好你");
        assert_eq!(reverse("a😀b"), "b😀a");
        assert_eq!(reverse("😀😎"), "😎😀");
    }

    #[test]
    fn test_reverse_palindrome() {
        assert_eq!(reverse("racecar"), "racecar");
        assert_eq!(reverse("上海自来水来自海上"), "上海自来水来自海上");
    }

    // ── split_by_char ──────────────────────────────────────────────

    #[test]
    fn test_split_normal() {
        let parts = split_by_char("a,b,c", ',');
        assert_eq!(parts, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_split_consecutive_delimiters() {
        let parts = split_by_char("x--y", '-');
        assert_eq!(parts, vec!["x", "", "y"]);
    }

    #[test]
    fn test_split_no_delimiter() {
        let parts = split_by_char("hello", ',');
        assert_eq!(parts, vec!["hello"]);
    }

    #[test]
    fn test_split_empty() {
        let parts = split_by_char("", ',');
        assert_eq!(parts, vec![""]);
    }

    #[test]
    fn test_split_only_delimiters() {
        let parts = split_by_char(",,,", ',');
        assert_eq!(parts, vec!["", "", "", ""]);
    }

    #[test]
    fn test_split_unicode_delimiter() {
        let parts = split_by_char("a★b★c", '★');
        assert_eq!(parts, vec!["a", "b", "c"]);
    }

    // ── join_strings ───────────────────────────────────────────────

    #[test]
    fn test_join_normal() {
        let words = ["hello", "world"];
        assert_eq!(join_strings(&words, " "), "hello world");
    }

    #[test]
    fn test_join_empty_slice() {
        let empty: [&str; 0] = [];
        assert_eq!(join_strings(&empty, ","), "");
    }

    #[test]
    fn test_join_single_element() {
        assert_eq!(join_strings(&["only"], ", "), "only");
    }

    #[test]
    fn test_join_no_separator() {
        assert_eq!(join_strings(&["a", "b"], ""), "ab");
    }

    #[test]
    fn test_join_with_empty_strings() {
        let parts = ["hello", "", "world"];
        assert_eq!(join_strings(&parts, ","), "hello,,world");
    }

    // ── capitalize_first ───────────────────────────────────────────

    #[test]
    fn test_capitalize_first_normal() {
        assert_eq!(capitalize_first("hello"), "Hello");
        assert_eq!(capitalize_first("world"), "World");
    }

    #[test]
    fn test_capitalize_first_empty() {
        assert_eq!(capitalize_first(""), "");
    }

    #[test]
    fn test_capitalize_first_already_capitalized() {
        assert_eq!(capitalize_first("Hello"), "Hello");
        assert_eq!(capitalize_first("World"), "World");
    }

    #[test]
    fn test_capitalize_first_leading_non_alpha() {
        assert_eq!(capitalize_first("123abc"), "123Abc");
        assert_eq!(capitalize_first("!hello"), "!Hello");
        assert_eq!(capitalize_first("  hello"), "  Hello");
    }

    #[test]
    fn test_capitalize_first_unicode() {
        assert_eq!(capitalize_first("über"), "Über");
        assert_eq!(capitalize_first("ñame"), "Ñame");
    }

    #[test]
    fn test_capitalize_first_single_char() {
        assert_eq!(capitalize_first("a"), "A");
        assert_eq!(capitalize_first("z"), "Z");
    }

    #[test]
    fn test_capitalize_first_digits_only() {
        // No alphabetic char to capitalize
        assert_eq!(capitalize_first("123"), "123");
    }

    #[test]
    fn test_capitalize_first_mixed() {
        assert_eq!(capitalize_first("foo_bar"), "Foo_bar");
    }

    // ── try_capitalize_first ───────────────────────────────────────

    #[test]
    fn test_try_capitalize_first_some() {
        assert_eq!(try_capitalize_first("hello"), Some("Hello".to_string()));
        assert_eq!(try_capitalize_first("a"), Some("A".to_string()));
    }

    #[test]
    fn test_try_capitalize_first_none() {
        assert_eq!(try_capitalize_first(""), None);
    }

    // ── to_camel_case ──────────────────────────────────────────────

    #[test]
    fn test_to_camel_case_normal() {
        assert_eq!(to_camel_case("hello world").unwrap(), "helloWorld");
        assert_eq!(to_camel_case("user_name").unwrap(), "userName");
        assert_eq!(to_camel_case("kebab-case").unwrap(), "kebabCase");
        assert_eq!(to_camel_case("XML Parser").unwrap(), "xmlParser");
    }

    #[test]
    fn test_to_camel_case_single_word() {
        assert_eq!(to_camel_case("hello").unwrap(), "hello");
        assert_eq!(to_camel_case("HELLO").unwrap(), "hello");
    }

    #[test]
    fn test_to_camel_case_empty() {
        assert_eq!(
            to_camel_case("").unwrap_err(),
            CaseConversionError::EmptyInput
        );
    }

    #[test]
    fn test_to_camel_case_whitespace() {
        assert_eq!(
            to_camel_case("   ").unwrap_err(),
            CaseConversionError::EmptyInput
        );
    }

    #[test]
    fn test_to_camel_case_no_alpha() {
        assert_eq!(
            to_camel_case("123").unwrap_err(),
            CaseConversionError::NoAlphabeticCharacters
        );
        assert_eq!(
            to_camel_case("!@#$").unwrap_err(),
            CaseConversionError::NoAlphabeticCharacters
        );
    }

    #[test]
    fn test_to_camel_case_camel_to_camel() {
        assert_eq!(to_camel_case("camelCase").unwrap(), "camelCase");
        assert_eq!(to_camel_case("CamelCase").unwrap(), "camelCase");
    }

    #[test]
    fn test_to_camel_case_leading_trailing_delimiters() {
        assert_eq!(to_camel_case("__hello_world__").unwrap(), "helloWorld");
        assert_eq!(to_camel_case("--hello-world--").unwrap(), "helloWorld");
    }

    #[test]
    fn test_to_camel_case_mixed_delimiters() {
        assert_eq!(
            to_camel_case("foo_bar-baz qux").unwrap(),
            "fooBarBazQux"
        );
    }

    #[test]
    fn test_to_camel_case_with_digits() {
        assert_eq!(to_camel_case("foo2bar").unwrap(), "foo2bar");
        assert_eq!(to_camel_case("foo-2-bar").unwrap(), "foo2Bar");
    }

    // ── to_pascal_case ─────────────────────────────────────────────

    #[test]
    fn test_to_pascal_case_normal() {
        assert_eq!(to_pascal_case("hello world").unwrap(), "HelloWorld");
        assert_eq!(to_pascal_case("user_name").unwrap(), "UserName");
        assert_eq!(to_pascal_case("kebab-case").unwrap(), "KebabCase");
    }

    #[test]
    fn test_to_pascal_case_empty() {
        assert_eq!(
            to_pascal_case("").unwrap_err(),
            CaseConversionError::EmptyInput
        );
    }

    #[test]
    fn test_to_pascal_case_no_alpha() {
        assert_eq!(
            to_pascal_case("123").unwrap_err(),
            CaseConversionError::NoAlphabeticCharacters
        );
    }

    #[test]
    fn test_to_pascal_case_already_pascal() {
        assert_eq!(to_pascal_case("HelloWorld").unwrap(), "HelloWorld");
    }

    // ── to_snake_case ──────────────────────────────────────────────

    #[test]
    fn test_snake_case_basic() {
        assert_eq!(to_snake_case("Hello World").unwrap(), "hello_world");
        assert_eq!(to_snake_case("user-name").unwrap(), "user_name");
    }

    #[test]
    fn test_snake_case_trimmed() {
        assert_eq!(to_snake_case("  Camel Case  ").unwrap(), "camel_case");
    }

    #[test]
    fn test_snake_case_empty() {
        assert!(to_snake_case("").is_err());
    }

    #[test]
    fn test_snake_case_already_snake() {
        assert_eq!(to_snake_case("already_snake").unwrap(), "already_snake");
    }

    #[test]
    fn test_snake_case_camel() {
        assert_eq!(to_snake_case("camelCase").unwrap(), "camel_case");
        assert_eq!(to_snake_case("XMLParser").unwrap(), "xml_parser");
        assert_eq!(to_snake_case("HTTPServer").unwrap(), "http_server");
    }

    #[test]
    fn test_snake_case_lossy() {
        assert_eq!(to_snake_case_lossy("Hello World"), "hello_world");
        assert_eq!(to_snake_case_lossy(""), "");
        assert_eq!(to_snake_case_lossy("   "), "");
    }

    // ── to_kebab_case ──────────────────────────────────────────────

    #[test]
    fn test_kebab_case_basic() {
        assert_eq!(to_kebab_case("Hello World").unwrap(), "hello-world");
        assert_eq!(to_kebab_case("user_name").unwrap(), "user-name");
    }

    #[test]
    fn test_kebab_case_trimmed() {
        assert_eq!(
            to_kebab_case("  Camel Case  ").unwrap(),
            "camel-case"
        );
    }

    #[test]
    fn test_kebab_case_empty() {
        assert!(to_kebab_case("").is_err());
    }

    #[test]
    fn test_kebab_case_already_kebab() {
        assert_eq!(
            to_kebab_case("already-kebab").unwrap(),
            "already-kebab"
        );
    }

    #[test]
    fn test_kebab_case_camel() {
        assert_eq!(to_kebab_case("camelCase").unwrap(), "camel-case");
        assert_eq!(to_kebab_case("XMLParser").unwrap(), "xml-parser");
    }

    #[test]
    fn test_kebab_case_lossy() {
        assert_eq!(to_kebab_case_lossy("Hello World"), "hello-world");
        assert_eq!(to_kebab_case_lossy(""), "");
        assert_eq!(to_kebab_case_lossy("   "), "");
    }

    // ── truncate_with_ellipsis ─────────────────────────────────────

    #[test]
    fn test_truncate_shorter_than_max() {
        assert_eq!(truncate_with_ellipsis("hi", 8), "hi");
        assert_eq!(truncate_with_ellipsis("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_with_ellipsis_applied() {
        assert_eq!(truncate_with_ellipsis("hello world", 8), "hello...");
        assert_eq!(truncate_with_ellipsis("hello world", 6), "hel...");
        assert_eq!(truncate_with_ellipsis("hello", 4), "h...");
    }

    #[test]
    fn test_truncate_empty() {
        assert_eq!(truncate_with_ellipsis("", 5), "");
    }

    #[test]
    fn test_truncate_zero_max_len() {
        assert_eq!(truncate_with_ellipsis("hello", 0), "");
    }

    #[test]
    fn test_truncate_max_len_less_than_ellipsis() {
        assert_eq!(truncate_with_ellipsis("hello", 1), "h");
        assert_eq!(truncate_with_ellipsis("hello", 2), "he");
    }

    #[test]
    fn test_truncate_max_len_equal_ellipsis() {
        assert_eq!(truncate_with_ellipsis("hello", 3), "...");
    }

    #[test]
    fn test_truncate_unicode() {
        assert_eq!(truncate_with_ellipsis("你好世界", 5), "你好世界");
        assert_eq!(truncate_with_ellipsis("你好世界", 4), "你好世界");
        assert_eq!(truncate_with_ellipsis("你好世界", 3), "...");
        assert_eq!(truncate_with_ellipsis("你好ab", 5), "你好ab");
    }

    #[test]
    fn test_truncate_emoji() {
        let s = "a😀b😎c";
        assert_eq!(truncate_with_ellipsis(s, 4), "a...");
        assert_eq!(truncate_with_ellipsis(s, 2), "a😀");
        assert_eq!(truncate_with_ellipsis(s, 3), "...");
        assert_eq!(truncate_with_ellipsis(s, 5), "a😀b...");
    }

    // ── is_blank ───────────────────────────────────────────────────

    #[test]
    fn test_is_blank_true() {
        assert!(is_blank(""));
        assert!(is_blank("   "));
        assert!(is_blank("\n\t\r "));
    }

    #[test]
    fn test_is_blank_false() {
        assert!(!is_blank("hello"));
        assert!(!is_blank("  hello  "));
    }

    // ── CaseConversionError display ────────────────────────────────

    #[test]
    fn test_case_conversion_error_display() {
        let err = CaseConversionError::EmptyInput;
        assert_eq!(
            err.to_string(),
            "input string is empty or contains only whitespace"
        );

        let err = CaseConversionError::NoAlphabeticCharacters;
        assert_eq!(
            err.to_string(),
            "input contains no alphabetic characters for case conversion"
        );

        let err = CaseConversionError::InvalidUtf8(vec![0xFF, 0xFE]);
        assert_eq!(
            err.to_string(),
            "invalid UTF-8 sequence: [255, 254]"
        );

        let err = CaseConversionError::UnsupportedCharacter('☺');
        assert!(err.to_string().contains("☺"));
    }

    // ── Byte-level conversion (InvalidUtf8) ────────────────────────

    #[test]
    fn test_to_snake_case_bytes_invalid_utf8() {
        let invalid = vec![0xFF, 0xFE, 0x00];
        let result = to_snake_case_bytes(&invalid);
        assert!(result.is_err());
        match result {
            Err(CaseConversionError::InvalidUtf8(bytes)) => {
                assert_eq!(bytes, vec![0xFF, 0xFE, 0x00]);
            }
            _ => panic!("expected InvalidUtf8 error"),
        }
    }

    #[test]
    fn test_to_camel_case_bytes_invalid_utf8() {
        let invalid = vec![0xFF, 0xFE];
        let result = to_camel_case_bytes(&invalid);
        assert!(result.is_err());
        match result {
            Err(CaseConversionError::InvalidUtf8(_)) => {} // ok
            _ => panic!("expected InvalidUtf8 error"),
        }
    }

    #[test]
    fn test_to_kebab_case_bytes_invalid_utf8() {
        let invalid = vec![0xFF, 0xFE];
        let result = to_kebab_case_bytes(&invalid);
        assert!(result.is_err());
        match result {
            Err(CaseConversionError::InvalidUtf8(_)) => {} // ok
            _ => panic!("expected InvalidUtf8 error"),
        }
    }

    #[test]
    fn test_to_pascal_case_bytes_invalid_utf8() {
        let invalid = vec![0xFF, 0xFE];
        let result = to_pascal_case_bytes(&invalid);
        assert!(result.is_err());
        match result {
            Err(CaseConversionError::InvalidUtf8(_)) => {} // ok
            _ => panic!("expected InvalidUtf8 error"),
        }
    }

    #[test]
    fn test_byte_conversion_valid_utf8() {
        // Valid UTF-8 bytes should work
        let valid = b"hello world";
        assert_eq!(
            to_snake_case_bytes(valid).unwrap(),
            "hello_world"
        );
        assert_eq!(
            to_camel_case_bytes(valid).unwrap(),
            "helloWorld"
        );
        assert_eq!(
            to_kebab_case_bytes(valid).unwrap(),
            "hello-world"
        );
    }

    // ── Tokenizer tests ────────────────────────────────────────────

    #[test]
    fn test_tokenize_simple() {
        let tokens = tokenize("hello world");
        assert_eq!(tokens, vec!["hello", "world"]);
    }

    #[test]
    fn test_tokenize_camel_case() {
        let tokens = tokenize("helloWorld");
        assert_eq!(tokens, vec!["hello", "World"]);
    }

    #[test]
    fn test_tokenize_acronym() {
        let tokens = tokenize("XMLParser");
        assert_eq!(tokens, vec!["XML", "Parser"]);
    }

    #[test]
    fn test_tokenize_mixed_delimiters() {
        let tokens = tokenize("foo_bar-baz qux");
        assert_eq!(tokens, vec!["foo", "bar", "baz", "qux"]);
    }

    #[test]
    fn test_tokenize_empty() {
        let tokens = tokenize("");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_tokenize_whitespace() {
        let tokens = tokenize("   ");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_tokenize_with_numbers() {
        let tokens = tokenize("foo2bar");
        assert_eq!(tokens, vec!["foo", "2", "bar"]);
    }

    #[test]
    fn test_tokenize_already_snake() {
        let tokens = tokenize("already_snake");
        assert_eq!(tokens, vec!["already", "snake"]);
    }

    // ── split_camel_case ───────────────────────────────────────────

    #[test]
    fn test_split_camel_case_simple() {
        let tokens = split_camel_case("camelCase");
        assert_eq!(tokens, vec!["camel", "Case"]);
    }

    #[test]
    fn test_split_camel_case_acronym() {
        let tokens = split_camel_case("XMLParser");
        assert_eq!(tokens, vec!["XML", "Parser"]);
    }

    #[test]
    fn test_split_camel_case_lowercase() {
        let tokens = split_camel_case("lowercase");
        assert_eq!(tokens, vec!["lowercase"]);
    }

    #[test]
    fn test_split_camel_case_empty() {
        let tokens = split_camel_case("");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_split_camel_case_with_digits() {
        let tokens = split_camel_case("foo123bar");
        assert_eq!(tokens, vec!["foo", "123", "bar"]);
    }
}
