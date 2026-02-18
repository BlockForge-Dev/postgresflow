use std::fs;

fn read_matrix() -> String {
    // When running `cargo test -p postgresflow`, the working dir is often `crates/postgresflow`.
    // But some runners use repo root. So support both.
    let candidates = ["tests/MATRIX.md", "crates/postgresflow/tests/MATRIX.md"];

    for p in candidates {
        if let Ok(s) = fs::read_to_string(p) {
            return s;
        }
    }

    panic!("MATRIX.md missing: create crates/postgresflow/tests/MATRIX.md (and/or tests/MATRIX.md when running from crate dir)");
}

#[test]
fn matrix_file_exists_and_mentions_all_laws() {
    let s = read_matrix();

    for needle in ["Law 1", "Law 2", "Law 3", "Law 4", "Law 5"] {
        assert!(s.contains(needle), "MATRIX.md missing section: {needle}");
    }

    assert!(
        s.contains("Reliability"),
        "MATRIX.md should include Reliability section"
    );
    assert!(
        s.contains("Load"),
        "MATRIX.md should include Load & Cost section"
    );
    assert!(
        s.contains("tests/"),
        "MATRIX.md should reference real test files (tests/...)"
    );
}
