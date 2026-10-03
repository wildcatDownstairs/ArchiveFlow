//! 与渲染无关的偏好、筛选、文件树和字典规则。
use archiveflow_core::domain::{
    archive::ArchiveEntry,
    task::{ArchiveType, Task, TaskStatus},
};
use chrono::Datelike;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    path::Path,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub english: bool,
    pub dark: bool,
    pub charset: String,
    pub min_length: usize,
    pub max_length: usize,
    pub priority: i32,
    pub concurrency: usize,
    pub hashcat_path: String,
    pub filename_patterns: bool,
    pub clear_dictionary: bool,
    pub mask_results: bool,
    pub mask_exports: bool,
    pub export_audit: bool,
    pub dictionary_draft: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            english: false,
            dark: true,
            charset: "abcdefghijklmnopqrstuvwxyz0123456789".into(),
            min_length: 1,
            max_length: 4,
            priority: 0,
            concurrency: 1,
            hashcat_path: String::new(),
            filename_patterns: false,
            clear_dictionary: false,
            mask_results: false,
            mask_exports: false,
            export_audit: true,
            dictionary_draft: String::new(),
        }
    }
}

impl Settings {
    pub fn load(dir: &Path) -> anyhow::Result<Self> {
        match std::fs::read(dir.join("native-settings.json")) {
            Ok(data) => {
                let settings: Self = serde_json::from_slice(&data)?;
                settings.validate()?;
                Ok(settings)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.charset.is_empty(),
            "字符集不能为空 / Character set is empty"
        );
        anyhow::ensure!(
            self.min_length > 0 && self.max_length >= self.min_length && self.max_length <= 32,
            "长度范围必须在 1–32 以内 / Length must be between 1 and 32"
        );
        anyhow::ensure!(
            (1..=16).contains(&self.concurrency),
            "并发数量必须在 1–16 以内 / Concurrency must be 1–16"
        );
        Ok(())
    }

    pub fn save(&self, dir: &Path) -> anyhow::Result<()> {
        use std::io::Write;
        self.validate()?;
        let mut file = tempfile::NamedTempFile::new_in(dir)?;
        file.write_all(&serde_json::to_vec_pretty(self)?)?;
        file.as_file().sync_all()?;
        file.persist(dir.join("native-settings.json"))?;
        Ok(())
    }
}

pub fn text(english: bool, zh: &'static str, en: &'static str) -> &'static str {
    if english { en } else { zh }
}

pub fn status_label(status: &TaskStatus, en: bool) -> &'static str {
    match status {
        TaskStatus::Ready => text(en, "就绪", "Ready"),
        TaskStatus::Processing => text(en, "恢复中", "Running"),
        TaskStatus::Succeeded => text(en, "已成功", "Succeeded"),
        TaskStatus::Exhausted => text(en, "已穷尽", "Exhausted"),
        TaskStatus::Cancelled => text(en, "已取消", "Cancelled"),
        TaskStatus::Failed => text(en, "失败", "Failed"),
        TaskStatus::Unsupported => text(en, "不支持", "Unsupported"),
        TaskStatus::Interrupted => text(en, "已中断", "Interrupted"),
    }
}

pub fn archive_type_label(kind: &ArchiveType) -> &'static str {
    match kind {
        ArchiveType::Zip => "ZIP",
        ArchiveType::SevenZ => "7Z",
        ArchiveType::Rar => "RAR",
        ArchiveType::Unknown => "FILE",
    }
}

pub fn size(bytes: u64) -> String {
    let mut n = bytes as f64;
    let mut unit = "B";
    for next in ["KiB", "MiB", "GiB", "TiB"] {
        if n < 1024.0 {
            break;
        }
        n /= 1024.0;
        unit = next;
    }
    if unit == "B" {
        format!("{bytes} B")
    } else {
        format!("{n:.1} {unit}")
    }
}

pub fn speed(value: f64) -> String {
    if value >= 1_000_000.0 {
        format!("{:.1}M /s", value / 1_000_000.0)
    } else if value >= 1_000.0 {
        format!("{:.1}K /s", value / 1_000.0)
    } else {
        format!("{value:.0} /s")
    }
}

pub fn matches_task(task: &Task, query: &str, filter: &str) -> bool {
    (filter == "all" || task.status.as_str() == filter)
        && (query.is_empty()
            || task
                .file_name
                .to_lowercase()
                .contains(&query.to_lowercase())
            || task
                .file_path
                .to_lowercase()
                .contains(&query.to_lowercase()))
}

#[derive(Default, Clone, Debug)]
pub struct FileNode {
    pub name: String,
    pub path: String,
    pub directory: bool,
    pub size: u64,
    pub encrypted: bool,
    pub children: BTreeMap<String, FileNode>,
}

pub fn file_tree(entries: &[ArchiveEntry]) -> FileNode {
    let mut root = FileNode::default();
    for entry in entries {
        let normalized = entry.path.replace('\\', "/");
        let parts: Vec<_> = normalized.split('/').filter(|p| !p.is_empty()).collect();
        let mut current = &mut root;
        let mut path = String::new();
        for (i, part) in parts.iter().enumerate() {
            if !path.is_empty() {
                path.push('/');
            }
            path.push_str(part);
            let directory = i + 1 < parts.len() || entry.is_directory;
            current = current
                .children
                .entry(part.to_string())
                .or_insert_with(|| FileNode {
                    name: part.to_string(),
                    path: path.clone(),
                    directory,
                    ..Default::default()
                });
            current.directory |= directory;
            if i + 1 == parts.len() {
                current.size = entry.size;
                current.encrypted = entry.is_encrypted;
            }
        }
    }
    root
}

pub const RULES: [(&str, &str); 10] = [
    ("转为大写", "Uppercase"),
    ("首字母大写", "Capitalize"),
    ("Leet 替换", "Leetspeak"),
    ("反转", "Reverse"),
    ("重复", "Duplicate"),
    ("年份后缀", "Year suffix"),
    ("组合分隔符", "Separators"),
    ("常见后缀", "Common suffix"),
    ("词语组合", "Combine words"),
    ("文件名模式", "Filename patterns"),
];

/// 保持原界面的 50,000 候选上限、去重和稳定顺序。
pub fn dictionary_candidates(
    input: &str,
    filename: &str,
    rules: &[bool; 10],
    year: i32,
) -> Vec<String> {
    let mut seeds: Vec<String> = input
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
    if rules[9] {
        let stem = filename.rsplit_once('.').map_or(filename, |(stem, _)| stem);
        seeds.extend(
            stem.split(|c: char| !c.is_ascii_alphanumeric())
                .filter(|s| s.len() >= 2)
                .map(str::to_owned),
        );
    }
    let mut seen = HashSet::new();
    let mut output = Vec::new();
    fn add(value: String, seen: &mut HashSet<String>, output: &mut Vec<String>) {
        let value = value.trim().to_string();
        if !value.is_empty() && output.len() < 50_000 && seen.insert(value.clone()) {
            output.push(value);
        }
    }
    for seed in seeds {
        if output.len() >= 50_000 {
            break;
        }
        let capitalize = {
            let mut chars = seed.chars();
            chars
                .next()
                .map(|c| c.to_uppercase().to_string() + &chars.as_str().to_lowercase())
                .unwrap_or_default()
        };
        add(seed.clone(), &mut seen, &mut output);
        if rules[0] {
            add(seed.to_uppercase(), &mut seen, &mut output);
        }
        if rules[1] {
            add(capitalize.clone(), &mut seen, &mut output);
        }
        if rules[2] {
            add(
                seed.chars()
                    .map(|c| match c.to_ascii_lowercase() {
                        'a' => '@',
                        'e' => '3',
                        'i' => '1',
                        'o' => '0',
                        's' => '$',
                        _ => c,
                    })
                    .collect(),
                &mut seen,
                &mut output,
            );
        }
        if rules[3] {
            add(seed.chars().rev().collect(), &mut seen, &mut output);
        }
        if rules[4] {
            add(seed.repeat(2), &mut seen, &mut output);
        }
        if rules[5] {
            for y in (year - 2..=year).rev() {
                for suffix in [y.to_string(), format!("{:02}", y % 100)] {
                    add(format!("{seed}{suffix}"), &mut seen, &mut output);
                    add(format!("{capitalize}{suffix}"), &mut seen, &mut output);
                }
            }
        }
        if rules[7] {
            for suffix in [
                "1".into(),
                "12".into(),
                "123".into(),
                "1234".into(),
                year.to_string(),
                (year - 1).to_string(),
                (year - 2).to_string(),
                "!".into(),
                "@123".into(),
            ] {
                add(format!("{seed}{suffix}"), &mut seen, &mut output);
            }
        }
    }
    if rules[8] {
        let source: Vec<_> = output.iter().take(200).cloned().collect();
        let separators: &[&str] = if rules[6] {
            &["", "-", "_", "."]
        } else {
            &[""]
        };
        for (i, a) in source.iter().enumerate() {
            for (j, b) in source.iter().enumerate() {
                if i != j {
                    for sep in separators {
                        add(format!("{a}{sep}{b}"), &mut seen, &mut output);
                    }
                    if output.len() >= 50_000 {
                        return output;
                    }
                }
            }
        }
    }
    output
}

pub fn current_year() -> i32 {
    chrono::Local::now().year()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn candidates_deduplicate_and_preserve_unicode() {
        let mut rules = [false; 10];
        rules[3] = true;
        rules[5] = true;
        let values = dictionary_candidates("密码\n密码\n hello ", "test.zip", &rules, 2026);
        assert_eq!(values[0], "密码");
        assert!(values.contains(&"码密".into()));
        assert!(values.contains(&"hello2026".into()));
        assert_eq!(values.iter().collect::<HashSet<_>>().len(), values.len());
    }
    #[test]
    fn candidates_obey_limit() {
        let input = (0..300)
            .map(|i| format!("word{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(
            dictionary_candidates(&input, "x.zip", &[true; 10], 2026).len(),
            50_000
        );
    }
    #[test]
    fn settings_survive_restart_and_invalid_json_is_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let settings = Settings {
            english: true,
            dark: false,
            ..Default::default()
        };
        settings.save(dir.path()).unwrap();
        let loaded = Settings::load(dir.path()).unwrap();
        assert!(loaded.english);
        assert!(!loaded.dark);
        std::fs::write(dir.path().join("native-settings.json"), "broken").unwrap();
        assert!(Settings::load(dir.path()).is_err());
    }
    #[test]
    fn tree_merges_implicit_and_explicit_directories() {
        let entry = |path: &str, directory| ArchiveEntry {
            path: path.into(),
            size: 4,
            compressed_size: 2,
            is_directory: directory,
            is_encrypted: false,
            last_modified: None,
        };
        let tree = file_tree(&[
            entry("docs\\a.txt", false),
            entry("docs/", true),
            entry("docs/b.txt", false),
        ]);
        assert!(tree.children["docs"].directory);
        assert_eq!(tree.children["docs"].children.len(), 2);
    }
}
