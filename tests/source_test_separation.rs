use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn source_files_only_contain_test_module_hooks() {
    let mut violations = Vec::new();
    for source_root in source_roots() {
        visit_rs_files(&source_root, &mut |path| {
            let text = fs::read_to_string(path).expect("read source file");
            collect_source_test_violations(path, &text, &mut violations);
        });
    }

    assert!(
        violations.is_empty(),
        "test code must live outside src production files:\n{}",
        violations.join("\n")
    );
}

fn source_roots() -> Vec<PathBuf> {
    let mut roots = vec![PathBuf::from("src")];
    let crates_dir = Path::new("crates");
    if let Ok(entries) = fs::read_dir(crates_dir) {
        for entry in entries {
            let entry = entry.expect("read crate directory entry");
            let src = entry.path().join("src");
            if src.is_dir() {
                roots.push(src);
            }
        }
    }
    roots
}

fn visit_rs_files(dir: &Path, f: &mut impl FnMut(&Path)) {
    for entry in fs::read_dir(dir).expect("read source directory") {
        let entry = entry.expect("read directory entry");
        let path = entry.path();
        if path.is_dir() {
            visit_rs_files(&path, f);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            f(&path);
        }
    }
}

fn collect_source_test_violations(path: &Path, text: &str, violations: &mut Vec<String>) {
    for (line_number, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed == "#[test]" || trimmed == "#[tokio::test]" {
            violations.push(format!(
                "{}:{} contains a test function attribute",
                display_path(path),
                line_number + 1
            ));
        }

        if trimmed.starts_with("mod tests") && trimmed.ends_with('{') {
            violations.push(format!(
                "{}:{} contains an inline test module",
                display_path(path),
                line_number + 1
            ));
        }
    }

    let lines: Vec<_> = text.lines().collect();
    for index in 0..lines.len() {
        if lines[index].trim() != "#[cfg(test)]" {
            continue;
        }

        let following: Vec<_> = lines
            .iter()
            .skip(index + 1)
            .take_while(|line| {
                let trimmed = line.trim();
                trimmed.starts_with("#[") || trimmed.starts_with("mod ") || trimmed.is_empty()
            })
            .filter_map(|line| {
                let trimmed = line.trim();
                (!trimmed.is_empty()).then_some(trimmed)
            })
            .collect();

        let has_unit_path = following
            .iter()
            .any(|line| line.starts_with("#[path = ") && line.contains("tests/unit/"));
        let has_test_module = following.iter().any(|line| {
            line.starts_with("mod ") && line.ends_with("_tests;") || *line == "mod mod_tests;"
        });

        if !has_unit_path || !has_test_module {
            violations.push(format!(
                "{}:{} has #[cfg(test)] code that is not a tests/unit module hook",
                display_path(path),
                index + 1
            ));
        }
    }
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
