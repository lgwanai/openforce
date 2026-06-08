//! String Processing Performance Benchmarks
//!
//! This benchmark suite measures the performance of all string utility functions
//! in `openforce-space-manager::utils::str`.  It covers:
//!
//! - **Concatenation**   – `join_strings` with various input sizes and separators
//! - **Splitting**       – `split_by_char` (CSV, consecutive delimiters, unicode)
//! - **Search / Check**  – `is_blank`, `trim_whitespace`, case conversion
//! - **Formatting**      – `to_snake_case`, `to_kebab_case`, `truncate_with_ellipsis`
//! - **High-risk**       – Huge strings, deeply-nested unicode, repeated allocation stress
//!
//! Performance targets (SLA):
//!   - Average latency < 2 ms  OR  throughput > 10 000 ops/s
//!   - Memory allocation stable across 10 runs (variance < 10 %)

use criterion::{
    black_box, criterion_group, criterion_main, BenchmarkId, Criterion, SamplingMode, Throughput,
};
use openforce_space_manager::utils::str::*;

// ============================================================================
//  Helper data generators
// ============================================================================

/// Generate an ASCII string of length `n` (repeating "abcdefghij").
fn ascii_string(n: usize) -> String {
    let pattern = "abcdefghij";
    pattern.chars().cycle().take(n).collect()
}

/// Generate a Unicode string (CJK + emoji) of approximately `n` chars.
fn unicode_string(n: usize) -> String {
    let pattern = "你好世界😀🚀";
    pattern.chars().cycle().take(n).collect()
}

/// Generate a CSV-line with `n` columns separated by `','`.
fn csv_line(n: usize) -> String {
    let cols: Vec<String> = (0..n).map(|i| format!("col_{}", i)).collect();
    cols.join(",")
}

/// Generate a "path-like" string with `n` segments.
fn path_string(n: usize) -> String {
    let segs: Vec<&str> = std::iter::repeat("segment").take(n).collect();
    segs.join("/")
}

// ============================================================================
//  1.  CONCATENATION  (join)
// ============================================================================

fn bench_join_small(c: &mut Criterion) {
    let parts: Vec<&str> = vec!["hello", "world", "foo", "bar"];
    let sep = ", ";
    c.bench_with_input(BenchmarkId::new("join_strings", "small"), &parts, |b, p| {
        b.iter(|| join_strings(black_box(p), black_box(sep)))
    });
}

fn bench_join_large(c: &mut Criterion) {
    let parts: Vec<String> = (0..10_000).map(|i| format!("item_{}", i)).collect();
    let refs: Vec<&str> = parts.iter().map(|s| s.as_str()).collect();
    let sep = ",";
    let mut group = c.benchmark_group("join_strings_large");
    group.sampling_mode(SamplingMode::Auto);
    group.throughput(Throughput::Elements(refs.len() as u64));
    group.bench_with_input(BenchmarkId::new("join_strings", "10k"), &refs, |b, p| {
        b.iter(|| join_strings(black_box(p), black_box(sep)))
    });
    group.finish();
}

// ============================================================================
//  2.  SPLITTING
// ============================================================================

fn bench_split_small(c: &mut Criterion) {
    let input = "a,b,c,d,e";
    c.bench_with_input(
        BenchmarkId::new("split_by_char", "small-CSV"),
        &input,
        |b, s| b.iter(|| split_by_char(black_box(s), ',')),
    );
}

fn bench_split_large_csv(c: &mut Criterion) {
    let input = csv_line(5_000);
    let mut group = c.benchmark_group("split_by_char_large");
    group.sampling_mode(SamplingMode::Auto);
    group.throughput(Throughput::Bytes(input.len() as u64));
    group.bench_with_input(
        BenchmarkId::new("split_by_char", "5k-CSV"),
        &input,
        |b, s| b.iter(|| split_by_char(black_box(s), ',')),
    );
    group.finish();
}

fn bench_split_consecutive_delimiters(c: &mut Criterion) {
    let input = "a,,b,,,c,d,,e,,,,f";
    c.bench_with_input(
        BenchmarkId::new("split_by_char", "consecutive"),
        &input,
        |b, s| b.iter(|| split_by_char(black_box(s), ',')),
    );
}

fn bench_split_unicode_delimiter(c: &mut Criterion) {
    let input = unicode_string(1_000).chars().collect::<String>();
    c.bench_with_input(
        BenchmarkId::new("split_by_char", "unicode-delim"),
        &input,
        |b, s| b.iter(|| split_by_char(black_box(s), '😀')),
    );
}

// ============================================================================
//  3.  SEARCH / CHECK
// ============================================================================

fn bench_is_blank_empty(c: &mut Criterion) {
    c.bench_function("is_blank_empty", |b| b.iter(|| is_blank(black_box(""))));
}

fn bench_is_blank_whitespace(c: &mut Criterion) {
    let s = "   \n\t  \u{00A0}  \u{3000}  ";
    c.bench_function("is_blank_whitespace", |b| b.iter(|| is_blank(black_box(s))));
}

fn bench_is_blank_nonblank(c: &mut Criterion) {
    c.bench_function("is_blank_nonblank", |b| {
        b.iter(|| is_blank(black_box("hello world")))
    });
}

fn bench_trim_small(c: &mut Criterion) {
    let s = "  hello world  ";
    c.bench_function("trim_whitespace_small", |b| {
        b.iter(|| trim_whitespace(black_box(s)))
    });
}

fn bench_trim_unicode(c: &mut Criterion) {
    let s = "\u{00A0}\u{3000}  你好世界  \u{3000}\u{00A0}";
    c.bench_function("trim_whitespace_unicode", |b| {
        b.iter(|| trim_whitespace(black_box(s)))
    });
}

fn bench_trim_large(c: &mut Criterion) {
    let s = format!("  {}  ", ascii_string(100_000));
    let mut group = c.benchmark_group("trim_whitespace_large");
    group.sampling_mode(SamplingMode::Auto);
    group.throughput(Throughput::Bytes(s.len() as u64));
    group.bench_with_input(BenchmarkId::new("trim_whitespace", "100k"), &s, |b, s| {
        b.iter(|| trim_whitespace(black_box(s)))
    });
    group.finish();
}

// ============================================================================
//  4.  FORMATTING  (case conversion, truncation)
// ============================================================================

fn bench_to_snake_case_short(c: &mut Criterion) {
    let s = "HelloWorld-User Name";
    c.bench_function("to_snake_case_short", |b| {
        b.iter(|| to_snake_case(black_box(s)))
    });
}

fn bench_to_snake_case_long(c: &mut Criterion) {
    let s = format!(
        "{} - {} - {}",
        ascii_string(1_000),
        unicode_string(500),
        "END"
    );
    let mut group = c.benchmark_group("to_snake_case_long");
    group.sampling_mode(SamplingMode::Auto);
    group.throughput(Throughput::Bytes(s.len() as u64));
    group.bench_with_input(BenchmarkId::new("to_snake_case", "1.5k"), &s, |b, s| {
        b.iter(|| to_snake_case(black_box(s)))
    });
    group.finish();
}

fn bench_to_kebab_case_short(c: &mut Criterion) {
    let s = "HelloWorld_User Name";
    c.bench_function("to_kebab_case_short", |b| {
        b.iter(|| to_kebab_case(black_box(s)))
    });
}

fn bench_truncate_no_trunc(c: &mut Criterion) {
    let s = "short";
    c.bench_function("truncate_no_truncation", |b| {
        b.iter(|| truncate_with_ellipsis(black_box(s), 10))
    });
}

fn bench_truncate_short(c: &mut Criterion) {
    let s = "hello world, this is a longer string that needs truncation";
    c.bench_function("truncate_short", |b| {
        b.iter(|| truncate_with_ellipsis(black_box(s), 20))
    });
}

fn bench_truncate_long(c: &mut Criterion) {
    let s = unicode_string(10_000);
    let mut group = c.benchmark_group("truncate_long");
    group.sampling_mode(SamplingMode::Auto);
    group.throughput(Throughput::Bytes(s.len() as u64));
    group.bench_with_input(BenchmarkId::new("truncate", "10k-unicode"), &s, |b, s| {
        b.iter(|| truncate_with_ellipsis(black_box(s), 50))
    });
    group.finish();
}

// ============================================================================
//  5.  HIGH-RISK SCENARIOS  –  these can expose O(n²) behaviour, alloc storms
// ============================================================================

/// Huge single-line CSV → tests split() on a very long string
fn bench_highrisk_huge_csv(c: &mut Criterion) {
    let input = csv_line(100_000); // 100k columns
    let mut group = c.benchmark_group("highrisk_huge_csv");
    group.sampling_mode(SamplingMode::Auto);
    group.throughput(Throughput::Bytes(input.len() as u64));
    group.bench_with_input(
        BenchmarkId::new("split_by_char", "100k-cols"),
        &input,
        |b, s| b.iter(|| split_by_char(black_box(s), ',')),
    );
    group.finish();
}

/// Deep Unicode path with many segments → join + split round-trip
fn bench_highrisk_deep_path(c: &mut Criterion) {
    let input = path_string(10_000);
    let mut group = c.benchmark_group("highrisk_deep_path");
    group.sampling_mode(SamplingMode::Auto);
    group.throughput(Throughput::Bytes(input.len() as u64));
    group.bench_with_input(BenchmarkId::new("roundtrip", "10k-path"), &input, |b, s| {
        b.iter(|| {
            let parts = split_by_char(black_box(s), '/');
            let _joined = join_strings(
                &parts.iter().map(|s| s.as_str()).collect::<Vec<&str>>(),
                "/",
            );
        })
    });
    group.finish();
}

/// Repeated allocation stress  –  trim + split + join cycle on large data
fn bench_highrisk_alloc_stress(c: &mut Criterion) {
    // Build a worst-case string: many whitespace-trimmed segments
    let raw: String = (0..5_000)
        .map(|i| format!("  segment_{}  ", i))
        .collect::<Vec<_>>()
        .join(",");
    let mut group = c.benchmark_group("highrisk_alloc_stress");
    group.sampling_mode(SamplingMode::Auto);
    group.throughput(Throughput::Bytes(raw.len() as u64));
    group.bench_with_input(BenchmarkId::new("trim+split+join", "5k"), &raw, |b, s| {
        b.iter(|| {
            let trimmed = trim_whitespace(black_box(s));
            let parts = split_by_char(&trimmed, ',');
            let trimmed_parts: Vec<&str> = parts.iter().map(|p| p.trim()).collect();
            let _result = join_strings(&trimmed_parts, "|");
        })
    });
    group.finish();
}

/// Truncation at every boundary – tests the `max_len` branching logic
fn bench_highrisk_truncation_edge_cases(c: &mut Criterion) {
    let s = "abcdefghijklmnopqrstuvwxyz";
    let mut group = c.benchmark_group("highrisk_truncation_edges");
    for max_len in [0, 1, 2, 3, 4, 5, 10, 15, 20, 26, 30] {
        group.bench_with_input(
            BenchmarkId::new("truncate", max_len),
            &(s, max_len),
            |b, (s, max)| b.iter(|| truncate_with_ellipsis(black_box(s), *max)),
        );
    }
    group.finish();
}

// ============================================================================
//  Register all benchmarks
// ============================================================================

criterion_group! {
    name = concatenation;
    config = Criterion::default().sample_size(100);
    targets = bench_join_small, bench_join_large
}

criterion_group! {
    name = splitting;
    config = Criterion::default().sample_size(100);
    targets = bench_split_small, bench_split_large_csv,
              bench_split_consecutive_delimiters, bench_split_unicode_delimiter
}

criterion_group! {
    name = search_check;
    config = Criterion::default().sample_size(100);
    targets = bench_is_blank_empty, bench_is_blank_whitespace, bench_is_blank_nonblank,
              bench_trim_small, bench_trim_unicode, bench_trim_large
}

criterion_group! {
    name = formatting;
    config = Criterion::default().sample_size(100);
    targets = bench_to_snake_case_short, bench_to_snake_case_long,
              bench_to_kebab_case_short,
              bench_truncate_no_trunc, bench_truncate_short, bench_truncate_long
}

criterion_group! {
    name = high_risk;
    config = Criterion::default().sample_size(50);  // fewer samples for heavy benches
    targets = bench_highrisk_huge_csv, bench_highrisk_deep_path,
              bench_highrisk_alloc_stress, bench_highrisk_truncation_edge_cases
}

criterion_main!(
    concatenation,
    splitting,
    search_check,
    formatting,
    high_risk
);
