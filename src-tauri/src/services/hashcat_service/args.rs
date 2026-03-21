use std::io::Write;
use std::path::{Path, PathBuf};

use crate::domain::recovery::AttackMode;

/// build_attack_args 的返回值：
///   - args: 最终要传给 hashcat 的命令行参数
///   - temp_files: 这次运行生成的临时文件，结束后统一清理
///   - outfile_path: hashcat 输出明文密码的结果文件
pub struct HashcatArgs {
    pub args: Vec<String>,
    pub temp_files: Vec<PathBuf>,
    pub outfile_path: PathBuf,
}

pub fn build_attack_args(
    mode: &AttackMode,
    hash_mode: u32,
    hash_string: &str,
    session_name: &str,
    temp_dir: &Path,
) -> Result<HashcatArgs, String> {
    let mut args = vec![
        "-m".to_string(),
        hash_mode.to_string(),
        "--status".to_string(),
        "--status-json".to_string(),
        "--status-timer=1".to_string(),
        "--session".to_string(),
        session_name.to_string(),
        "--potfile-disable".to_string(),
        "--restore-disable".to_string(),
        "--outfile-format".to_string(),
        "2".to_string(),
        "-o".to_string(),
    ];
    let mut temp_files = Vec::new();

    let outfile_path = temp_dir.join(format!("{}.out", session_name));
    args.push(outfile_path.to_string_lossy().to_string());
    temp_files.push(outfile_path.clone());

    let hash_file_path = temp_dir.join(format!("{}.hash", session_name));
    std::fs::write(&hash_file_path, format!("{}\n", hash_string))
        .map_err(|error| format!("写入 hash 临时文件失败: {}", error))?;
    args.push(hash_file_path.to_string_lossy().to_string());
    temp_files.push(hash_file_path);

    match mode {
        AttackMode::Dictionary { wordlist } => {
            args.push("-a".to_string());
            args.push("0".to_string());

            let wordlist_path = temp_dir.join(format!("{}.wordlist", session_name));
            let mut file = std::fs::File::create(&wordlist_path)
                .map_err(|error| format!("创建字典临时文件失败: {}", error))?;
            for word in wordlist {
                writeln!(file, "{}", word)
                    .map_err(|error| format!("写入字典临时文件失败: {}", error))?;
            }
            args.push(wordlist_path.to_string_lossy().to_string());
            temp_files.push(wordlist_path);
        }
        AttackMode::BruteForce {
            charset,
            min_length,
            max_length,
        } => {
            args.push("-a".to_string());
            args.push("3".to_string());
            args.push("-1".to_string());
            args.push(charset.clone());
            args.push("-i".to_string());
            args.push(format!("--increment-min={}", min_length));
            args.push(format!("--increment-max={}", max_length));
            args.push("?1".repeat(*max_length));
        }
        AttackMode::Mask { mask } => {
            args.push("-a".to_string());
            args.push("3".to_string());
            args.push(translate_mask(mask)?);
        }
    }

    Ok(HashcatArgs {
        args,
        temp_files,
        outfile_path,
    })
}

fn translate_mask(mask: &str) -> Result<String, String> {
    let mut translated = String::new();
    let chars: Vec<char> = mask.chars().collect();
    let mut index = 0;

    while index < chars.len() {
        if chars[index] == '?' {
            let token = chars
                .get(index + 1)
                .copied()
                .ok_or_else(|| "掩码以单独的 ? 结尾，无法翻译到 hashcat".to_string())?;
            match token {
                'l' | 'u' | 'd' | 's' | 'a' | '?' => {
                    translated.push('?');
                    translated.push(token);
                    index += 2;
                }
                other => {
                    return Err(format!("GPU 后端暂不支持掩码标记 ?{}", other));
                }
            }
        } else {
            translated.push(chars[index]);
            index += 1;
        }
    }

    Ok(translated)
}

#[cfg(test)]
mod tests {
    use super::{build_attack_args, translate_mask};
    use crate::domain::recovery::AttackMode;

    #[test]
    fn translate_mask_accepts_hashcat_tokens_and_literal_question_marks() {
        assert_eq!(translate_mask("?d?dAB").unwrap(), "?d?dAB");
        assert_eq!(translate_mask("??").unwrap(), "??");
    }

    #[test]
    fn translate_mask_rejects_unknown_tokens() {
        assert!(translate_mask("?x").is_err());
    }

    #[test]
    fn build_attack_args_creates_dictionary_wordlist_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let args = build_attack_args(
            &AttackMode::Dictionary {
                wordlist: vec!["alpha".to_string(), "beta".to_string()],
            },
            13600,
            "$zip2$hash",
            "session",
            temp_dir.path(),
        )
        .unwrap();

        assert!(args.args.iter().any(|arg| arg == "-a"));
        assert!(args.args.iter().any(|arg| arg == "0"));
        assert_eq!(args.temp_files.len(), 3);
    }

    #[test]
    fn build_attack_args_creates_bruteforce_mask() {
        let temp_dir = tempfile::tempdir().unwrap();
        let args = build_attack_args(
            &AttackMode::BruteForce {
                charset: "0123456789".to_string(),
                min_length: 1,
                max_length: 4,
            },
            13600,
            "$zip2$hash",
            "session",
            temp_dir.path(),
        )
        .unwrap();

        assert!(args.args.iter().any(|arg| arg == "?1?1?1?1"));
        assert!(args.args.iter().any(|arg| arg == "--increment-max=4"));
    }
}
