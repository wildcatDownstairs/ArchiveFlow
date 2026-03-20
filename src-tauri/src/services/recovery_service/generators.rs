// 这个文件负责"生成候选密码"。
// 它只处理枚举逻辑，不接触线程、数据库或 Tauri 事件。
// 把候选生成单独拆出来后，后续增加新攻击模式会更直接。

use crate::domain::recovery::AttackMode;

pub struct BruteForceIterator {
    charset: Vec<char>,
    min_len: usize,
    max_len: usize,
    current_len: usize,
    indices: Vec<usize>,
    exhausted: bool,
    done: bool,
}

impl BruteForceIterator {
    pub fn new(charset: &str, min_len: usize, max_len: usize) -> Self {
        let chars: Vec<char> = charset.chars().collect();
        let actual_min = if min_len == 0 { 1 } else { min_len };
        let actual_max = if max_len < actual_min {
            actual_min
        } else {
            max_len
        };

        Self {
            charset: chars,
            min_len: actual_min,
            max_len: actual_max,
            current_len: actual_min,
            indices: vec![0; actual_min],
            exhausted: false,
            done: false,
        }
    }

    pub fn total_combinations(charset_len: usize, min_len: usize, max_len: usize) -> u64 {
        let actual_min = if min_len == 0 { 1 } else { min_len };
        let actual_max = if max_len < actual_min {
            actual_min
        } else {
            max_len
        };
        let base = charset_len as u64;
        let mut total: u64 = 0;
        for len in actual_min..=actual_max {
            total = total.saturating_add(base.saturating_pow(len as u32));
        }
        total
    }

    pub fn skip_to(&mut self, mut n: u64) {
        if self.done || self.charset.is_empty() {
            return;
        }

        let base = self.charset.len() as u64;
        let mut len = self.min_len;
        loop {
            let count = base.saturating_pow(len as u32);
            if n < count {
                break;
            }
            n -= count;
            len += 1;
            if len > self.max_len {
                self.done = true;
                return;
            }
        }

        self.current_len = len;
        self.indices = vec![0usize; len];
        self.exhausted = false;

        let base_usize = self.charset.len();
        let mut remaining = n;
        for i in (0..len).rev() {
            self.indices[i] = (remaining as usize) % base_usize;
            remaining /= base_usize as u64;
        }
    }
}

impl Iterator for BruteForceIterator {
    type Item = String;

    fn next(&mut self) -> Option<String> {
        if self.done || self.charset.is_empty() {
            return None;
        }

        if self.exhausted {
            self.current_len += 1;
            if self.current_len > self.max_len {
                self.done = true;
                return None;
            }
            self.indices = vec![0; self.current_len];
            self.exhausted = false;
        }

        let password: String = self.indices.iter().map(|&i| self.charset[i]).collect();

        let charset_len = self.charset.len();
        let mut carry = true;
        for i in (0..self.indices.len()).rev() {
            if carry {
                self.indices[i] += 1;
                if self.indices[i] >= charset_len {
                    self.indices[i] = 0;
                } else {
                    carry = false;
                }
            }
        }

        if carry {
            self.exhausted = true;
        }

        Some(password)
    }
}

#[allow(dead_code)]
pub fn generate_bruteforce_passwords(
    charset: &str,
    min_len: usize,
    max_len: usize,
) -> BruteForceIterator {
    BruteForceIterator::new(charset, min_len, max_len)
}

const MASK_LOWERCASE: &str = "abcdefghijklmnopqrstuvwxyz";
const MASK_UPPERCASE: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const MASK_DIGITS: &str = "0123456789";
const MASK_SPECIAL: &str = "!@#$%^&*()_+-=[]{}|;:',.<>?/~`\"\\";
const MASK_ALL: &str = concat!(
    "abcdefghijklmnopqrstuvwxyz",
    "ABCDEFGHIJKLMNOPQRSTUVWXYZ",
    "0123456789",
    "!@#$%^&*()_+-=[]{}|;:',.<>?/~`\"\\"
);

pub(crate) fn parse_mask(mask: &str) -> Result<Vec<Vec<char>>, String> {
    if mask.is_empty() {
        return Err("掩码不能为空".to_string());
    }

    let chars: Vec<char> = mask.chars().collect();
    let mut slots = Vec::new();
    let mut index = 0;

    while index < chars.len() {
        if chars[index] == '?' {
            let Some(token) = chars.get(index + 1) else {
                return Err("掩码以未完成的 ? 结尾".to_string());
            };
            let charset = match token {
                'l' => MASK_LOWERCASE.chars().collect(),
                'u' => MASK_UPPERCASE.chars().collect(),
                'd' => MASK_DIGITS.chars().collect(),
                's' => MASK_SPECIAL.chars().collect(),
                'a' => MASK_ALL.chars().collect(),
                '?' => vec!['?'],
                _ => {
                    return Err(format!("不支持的掩码标记: ?{}", token));
                }
            };
            slots.push(charset);
            index += 2;
        } else {
            slots.push(vec![chars[index]]);
            index += 1;
        }
    }

    if slots.is_empty() {
        return Err("掩码至少需要一个位置".to_string());
    }

    Ok(slots)
}

pub struct MaskIterator {
    charsets: Vec<Vec<char>>,
    indices: Vec<usize>,
    done: bool,
}

impl MaskIterator {
    pub fn new(mask: &str) -> Result<Self, String> {
        let charsets = parse_mask(mask)?;
        let indices = vec![0; charsets.len()];
        Ok(Self {
            charsets,
            indices,
            done: false,
        })
    }

    pub fn total_combinations(mask: &str) -> Result<u64, String> {
        let charsets = parse_mask(mask)?;
        Ok(charsets.iter().fold(1_u64, |total, charset| {
            total.saturating_mul(charset.len() as u64)
        }))
    }

    pub fn skip_to(&mut self, mut n: u64) {
        if self.done {
            return;
        }

        let total = self.charsets.iter().fold(1_u64, |total, charset| {
            total.saturating_mul(charset.len() as u64)
        });
        if n >= total {
            self.done = true;
            return;
        }

        for i in (0..self.charsets.len()).rev() {
            let base = self.charsets[i].len() as u64;
            self.indices[i] = (n % base) as usize;
            n /= base;
        }
    }
}

impl Iterator for MaskIterator {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        let password: String = self
            .indices
            .iter()
            .enumerate()
            .map(|(i, &index)| self.charsets[i][index])
            .collect();

        let mut carry = true;
        for i in (0..self.indices.len()).rev() {
            if carry {
                self.indices[i] += 1;
                if self.indices[i] >= self.charsets[i].len() {
                    self.indices[i] = 0;
                } else {
                    carry = false;
                }
            }
        }

        if carry {
            self.done = true;
        }

        Some(password)
    }
}

/// 为给定分片 [shard_start, shard_end) 构建密码迭代器。
pub(crate) fn shard_passwords(
    mode: &AttackMode,
    shard_start: u64,
    shard_end: u64,
) -> Box<dyn Iterator<Item = String> + Send + '_> {
    match mode {
        AttackMode::Dictionary { wordlist } => Box::new(
            wordlist
                .iter()
                .skip(shard_start as usize)
                .take((shard_end - shard_start) as usize)
                .cloned(),
        ),
        AttackMode::BruteForce {
            charset,
            min_length,
            max_length,
        } => {
            let mut iter = BruteForceIterator::new(charset, *min_length, *max_length);
            iter.skip_to(shard_start);
            Box::new(iter.take((shard_end - shard_start) as usize))
        }
        AttackMode::Mask { mask } => {
            let mut iter = MaskIterator::new(mask)
                .expect("mask mode should be validated before worker sharding");
            iter.skip_to(shard_start);
            Box::new(iter.take((shard_end - shard_start) as usize))
        }
    }
}
