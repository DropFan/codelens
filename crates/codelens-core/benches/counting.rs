use criterion::{black_box, criterion_group, criterion_main, Criterion};

use codelens_core::analyzer::counter::count_stats;
use codelens_core::analyzer::trie::build_from_language;
use codelens_core::Language;

fn make_rust_lang() -> Language {
    let toml = r#"
        [rust]
        name = "Rust"
        extensions = [".rs"]
        line_comments = ["//"]
        block_comments = [["/*", "*/"]]
        nested_comments = true
    "#;
    let langs: std::collections::HashMap<String, Language> = toml::from_str(toml).unwrap();
    langs.into_values().next().unwrap()
}

fn generate_rust_code(lines: usize) -> String {
    let mut code = String::new();
    for i in 0..lines {
        match i % 10 {
            0 => code.push_str("// This is a comment line\n"),
            1 => code.push_str("fn some_function() {\n"),
            2 => code.push_str("    let x = \"hello world\";\n"),
            3 => code.push_str("    /* inline block comment */\n"),
            4 => code.push('\n'),
            5 => code.push_str("    if x > 0 {\n"),
            6 => code.push_str("        println!(\"value: {}\", x);\n"),
            7 => code.push_str("    }\n"),
            8 => code.push_str("}\n"),
            _ => code.push('\n'),
        }
    }
    code
}

fn bench_count_stats(c: &mut Criterion) {
    let lang = make_rust_lang();
    let code_1k = generate_rust_code(1_000);
    let code_10k = generate_rust_code(10_000);

    let (trie, mask) = build_from_language(&lang);

    c.bench_function("count_stats_1k_lines", |b| {
        b.iter(|| count_stats(black_box(code_1k.as_bytes()), &trie, mask))
    });

    c.bench_function("count_stats_10k_lines", |b| {
        b.iter(|| count_stats(black_box(code_10k.as_bytes()), &trie, mask))
    });
}

fn bench_trie_build(c: &mut Criterion) {
    let lang = make_rust_lang();

    c.bench_function("build_trie_rust", |b| {
        b.iter(|| build_from_language(black_box(&lang)))
    });
}

criterion_group!(benches, bench_count_stats, bench_trie_build);
criterion_main!(benches);
