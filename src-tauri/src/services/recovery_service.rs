// std::any::Any 用于在 panic handler 中将 panic payload 向下转型为具体类型
use std::any::Any;
// Read + Seek 是 trait（类似接口），泛型约束让函数可以接受任何实现了这两个 trait 的类型
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};
// AtomicBool / AtomicU64：原子类型，多线程下无需 Mutex 即可安全读写
// Ordering::Relaxed：最宽松的内存顺序，适合只需要计数而不关心顺序语义的场景
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
// mpsc = multi-producer single-consumer，多生产者单消费者通道
// Arc = Atomically Reference Counted，引用计数指针，允许多线程共享所有权
use std::sync::{mpsc, Arc};
// JoinHandle 是 thread::spawn 返回的句柄，用于等待线程完成
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use chrono::Utc;
use sevenz_rust::{Error as SevenZError, Password, SevenZReader};
// Emitter：向前端发送事件；Manager：读取 Tauri 管理的 State
use tauri::{Emitter, Manager};

use crate::db::Database;
use crate::domain::recovery::{
    AttackMode, RecoveryCheckpoint, RecoveryConfig, RecoveryProgress, RecoveryStatus,
};
use crate::domain::task::ArchiveType;
use crate::services::archive_service;

// ─── 密码验证 ─────────────────────────────────────────────────────

/// 在已打开的 ZipArchive 上，用给定密码尝试解密指定索引的条目。
///
/// # 为什么复用 archive？
/// 每次打开文件都需要解析文件头（有 IO 开销），而 ZipArchive 可以复用。
/// 多线程场景下每个 worker 线程拥有自己的 ZipArchive 实例，避免锁竞争。
///
/// # 泛型参数 `R: Read + Seek`
/// 这是 trait bounds（特征约束）。意思是：R 可以是任何同时实现了
/// `Read`（可读取字节）和 `Seek`（可随机定位）的类型，
/// 例如 `std::fs::File`、`std::io::Cursor<Vec<u8>>` 等。
/// 这让函数既能用于真实文件，也能用于内存缓冲区（方便测试）。
pub fn try_password_on_archive<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    index: usize,
    password: &str,
) -> bool {
    // .as_bytes() 将 &str 转换为 &[u8]（字节切片），ZIP 库要求字节形式的密码
    let result = archive.by_index_decrypt(index, password.as_bytes());
    let mut zip_file = match result {
        Ok(f) => f,
        Err(_) => return false, // InvalidPassword 或其他 IO 错误
    };

    // by_index_decrypt 成功不代表密码一定正确：
    // ZipCrypto 算法有约 1/256 的概率误判（CRC 头字节碰巧匹配）。
    // 只有实际读取全部数据、CRC 校验通过，才能确认密码正确。
    // read_to_end 会把数据读入 Vec<u8>；这里不需要保留数据，只是触发校验。
    let mut buf = Vec::new();
    match zip_file.read_to_end(&mut buf) {
        Ok(_) => true,
        Err(_) => false, // CRC 校验失败 → 密码错误
    }
}

/// 独立版本：每次都重新打开文件，尝试解密第一个加密条目。
/// 适合单次测试调用，不适合热路径（每次调用都有文件 IO 开销）。
///
/// `#[allow(dead_code)]`：告诉编译器即使这个函数在当前 crate 内没被调用，
/// 也不要发出"未使用"的警告（供测试或外部调用使用）。
#[allow(dead_code)]
pub fn try_password_zip(file_path: &Path, password: &str) -> bool {
    let file = match std::fs::File::open(file_path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let mut archive = match zip::ZipArchive::new(file) {
        Ok(a) => a,
        Err(_) => return false,
    };

    // Iterator::find 返回第一个满足条件的元素（包装在 Option 中）
    // 这里找到第一个"加密的非目录条目"的索引
    let encrypted_index = (0..archive.len()).find(|&i| {
        archive
            .by_index_raw(i)
            // map 在 Result::Ok 时变换内部值；unwrap_or 在 Err 时给默认值
            .map(|entry| entry.encrypted() && !entry.is_dir())
            .unwrap_or(false)
    });

    let index = match encrypted_index {
        Some(i) => i,
        None => return false, // 没有加密条目，无需破解
    };

    try_password_on_archive(&mut archive, index, password)
}

/// 尝试用给定密码打开 7z 文件。
/// 每次调用都会重新打开文件（sevenz_rust 库的 SevenZReader 不支持复用）。
///
/// # 7z 的两种加密模式
/// - **内容加密**（Content Encryption）：文件头未加密，内容加密。
///   空密码可以打开文件头但无法解压内容。
/// - **头部加密**（Header Encryption）：文件头也加密。
///   错误密码连文件头都解不开，直接报错。
///
/// 验证逻辑：
/// 1. 用指定密码打开并验证 payload
/// 2. 再用空密码验证一次：如果空密码也能通过，说明文件根本没加密
pub fn try_password_7z(file_path: &Path, password: &str) -> bool {
    match SevenZReader::open(file_path, Password::from(password)) {
        Ok(mut reader) => match validate_7z_payload(&mut reader) {
            Ok(true) => {
                // 双重确认：空密码也能通过 → 文件未加密 → 返回 false
                if let Ok(mut empty_reader) = SevenZReader::open(file_path, Password::empty()) {
                    if let Ok(true) = validate_7z_payload(&mut empty_reader) {
                        return false;
                    }
                }
                true
            }
            _ => false,
        },
        Err(_) => false,
    }
}

/// 尝试用给定密码解密 RAR 文件中第一个加密条目。
///
/// # RAR 的处理模式
/// unrar 库使用"流式处理"：打开后依次读取条目头，
/// 对每个条目要么处理（test）要么跳过（skip）。
/// 这里找到第一个加密的非目录条目后调用 `test()`，
/// test() 内部会解密并验证数据完整性。
pub fn try_password_rar(file_path: &Path, password: &str) -> bool {
    use unrar::Archive as RarArchive;

    // to_string_lossy() 将可能含非 UTF-8 字节的路径安全地转换为字符串：
    // 遇到无效 UTF-8 字节时用 U+FFFD（替换字符）替代，而不是 panic。
    let file_path_str = file_path.to_string_lossy().to_string();

    let open_result = RarArchive::with_password(&file_path_str, password).open_for_processing();

    let archive = match open_result {
        Ok(a) => a,
        Err(_) => return false,
    };

    // unrar 的 API 是"移动所有权"风格：read_header 消耗 archive 返回 entry，
    // entry.skip() 消耗 entry 再返回新的 archive。
    // 用 `let mut archive = archive` 让变量可变，方便循环中重新绑定。
    let mut archive = archive;
    loop {
        let entry = match archive.read_header() {
            Ok(Some(entry)) => entry,
            Ok(None) => return false, // 遍历完毕，没找到加密条目
            Err(_) => return false,
        };

        let should_test = {
            let header = entry.entry();
            header.is_encrypted() && !header.is_directory()
        };

        if should_test {
            // entry.test() 解密并校验，成功则密码正确
            return entry.test().is_ok();
        }

        // 跳过当前条目，移动所有权到下一个 archive 状态
        archive = match entry.skip() {
            Ok(next) => next,
            Err(_) => return false,
        };
    }
}

/// 判断 7z 错误是否属于"密码相关"错误。
/// `matches!` 宏是模式匹配的简洁写法，等价于 `if let ... { true } else { false }`。
#[allow(dead_code)]
fn is_7z_password_error(error: &SevenZError) -> bool {
    matches!(
        error,
        SevenZError::PasswordRequired | SevenZError::MaybeBadPassword(_)
    )
}

/// 验证 7z 文件的 payload（读取所有文件条目触发解密校验）。
///
/// # 为什么用 `std::io::sink()`？
/// `sink()` 返回一个"黑洞"写入目标：接受所有数据但直接丢弃。
/// 这里不需要保存解压内容，只需要触发解密+校验流程即可。
/// 如果密码错误，`io::copy` 会在校验失败时返回 Err。
///
/// # 返回值
/// - `Ok(true)`：至少验证了一个文件条目（说明文件有内容且可解密）
/// - `Ok(false)`：文件是空的（没有任何文件流）
/// - `Err(...)`：解密失败（密码错误或文件损坏）
fn validate_7z_payload<R: Read + Seek>(reader: &mut SevenZReader<R>) -> Result<bool, SevenZError> {
    let mut validated_any_file = false;

    // for_each_entries 接受一个闭包（匿名函数），对每个条目调用一次
    // 闭包参数：entry（条目元数据），entry_reader（条目数据流）
    // 返回 Ok(true) 表示继续迭代，Ok(false) 表示提前停止
    reader.for_each_entries(|entry, entry_reader| {
        // 跳过目录和空条目（它们没有数据流，无需解密）
        if entry.is_directory() || !entry.has_stream() {
            return Ok(true);
        }

        validated_any_file = true;
        // io::copy 将 entry_reader 的数据复制到 sink()（黑洞），触发解密校验
        // `?` 操作符：如果 copy 返回 Err，立即从当前闭包返回该错误
        std::io::copy(entry_reader, &mut std::io::sink())?;
        Ok(true)
    })?;

    Ok(validated_any_file)
}

/// 在恢复开始前，验证目标文件确实是加密的。
/// 避免对未加密文件浪费 CPU 做破解尝试。
fn validate_recovery_target(path: &Path, archive_type: &ArchiveType) -> Result<(), String> {
    // 根据不同格式调用对应的检查函数
    // inspect_* 函数返回 Result<ArchiveInfo, String>，? 在失败时提前返回错误
    let is_encrypted = match archive_type {
        ArchiveType::Zip => archive_service::inspect_zip(path)?.is_encrypted,
        ArchiveType::SevenZ => archive_service::inspect_7z(path)?.is_encrypted,
        ArchiveType::Rar => archive_service::inspect_rar(path)?.is_encrypted,
        ArchiveType::Unknown => {
            return Err("未知的归档类型，无法进行密码恢复".to_string());
        }
    };

    if is_encrypted {
        Ok(())
    } else {
        Err("当前归档没有可恢复的加密内容".to_string())
    }
}

// ─── 暴力破解密码生成器 ────────────────────────────────────────────

/// 暴力破解密码迭代器：按字典序生成指定字符集在 [min_len, max_len] 范围内的所有组合。
///
/// # 设计思路
/// 使用"进位计数"算法：把密码看成一个多位数，每位的"基数"是字符集大小。
/// 从最右位 +1，满字符集大小则进位（类似十进制进位，但基数是 charset.len()）。
///
/// # 自定义迭代器
/// Rust 的 `Iterator` trait 要求实现 `next(&mut self) -> Option<Self::Item>`。
/// 返回 `Some(item)` 表示还有元素，返回 `None` 表示迭代结束。
pub struct BruteForceIterator {
    /// 字符集：所有候选字符的列表
    charset: Vec<char>,
    min_len: usize,
    max_len: usize,
    /// 当前正在枚举的密码长度
    current_len: usize,
    /// 当前密码在字符集中的索引数组（indices[i] 是第 i 位字符在 charset 的下标）
    indices: Vec<usize>,
    /// 当前长度的所有组合是否已穷尽，需要切换到下一个长度
    exhausted: bool,
    /// 所有长度都穷尽，整个迭代器结束
    done: bool,
}

impl BruteForceIterator {
    pub fn new(charset: &str, min_len: usize, max_len: usize) -> Self {
        // .chars().collect() 将字符串按 Unicode 字符分割成 Vec<char>
        let chars: Vec<char> = charset.chars().collect();
        // 保护性钳位：min_len=0 视为 1，max_len < min_len 视为 min_len
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
            // vec![0; n] 创建长度为 n、每个元素为 0 的 Vec
            indices: vec![0; actual_min],
            exhausted: false,
            done: false,
        }
    }

    /// 计算总组合数，用于前端显示进度。
    /// `saturating_add` / `saturating_pow`：溢出时返回类型最大值而不是 panic，
    /// 避免字符集很大、长度很长时整数溢出。
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

    /// 快速跳到全局第 `n` 个密码，而不逐个生成中间结果。
    ///
    /// # 用途：断点续传
    /// 上次任务跑到第 K 个密码时被取消，下次恢复时直接跳到 K，
    /// 不需要重新从头迭代，节省大量时间。
    ///
    /// # 算法：混合进制解码
    /// 把全局偏移 n 看作多进制数：
    /// 1. 先减去各个"长度段"的总数，确定 n 落在哪个长度段
    /// 2. 在该长度段内，把 n 解码为多进制数（每位基数 = charset.len()）
    ///    类似十进制转二进制，但基数可变
    pub fn skip_to(&mut self, mut n: u64) {
        if self.done || self.charset.is_empty() {
            return;
        }

        let base = self.charset.len() as u64;

        // 第一步：找到 n 所在的长度段
        let mut len = self.min_len;
        loop {
            let count = base.saturating_pow(len as u32);
            if n < count {
                break; // n 在当前长度段内
            }
            n -= count;
            len += 1;
            if len > self.max_len {
                self.done = true;
                return;
            }
        }

        // 第二步：在长度 len 的段内，将 n 解码为各位索引
        self.current_len = len;
        self.indices = vec![0usize; len];
        self.exhausted = false;

        // 从最右位开始，逐位取余数（类似十进制数转换为各位数字）
        let base_usize = self.charset.len();
        let mut remaining = n;
        for i in (0..len).rev() {
            self.indices[i] = (remaining as usize) % base_usize;
            remaining /= base_usize as u64;
        }
    }
}

/// 为 BruteForceIterator 实现 Iterator trait。
/// 实现 trait 类似实现接口，但 Rust 的 trait 是静态分发，无运行时开销。
impl Iterator for BruteForceIterator {
    /// 关联类型：这个迭代器产生的元素类型是 String
    type Item = String;

    fn next(&mut self) -> Option<String> {
        if self.done || self.charset.is_empty() {
            return None; // None 表示迭代结束
        }

        if self.exhausted {
            // 当前长度的所有组合已穷尽，切换到下一个长度
            self.current_len += 1;
            if self.current_len > self.max_len {
                self.done = true;
                return None;
            }
            self.indices = vec![0; self.current_len];
            self.exhausted = false;
        }

        // 根据当前 indices 生成密码字符串
        // Iterator::map + collect：将每个索引映射为对应字符，再收集成 String
        let password: String = self.indices.iter().map(|&i| self.charset[i]).collect();

        // 进位算法：从最右位（最低位）开始 +1，满字符集大小则清零并向左进位
        let charset_len = self.charset.len();
        let mut carry = true; // 初始进位 = 1（相当于 +1）
        for i in (0..self.indices.len()).rev() {
            if carry {
                self.indices[i] += 1;
                if self.indices[i] >= charset_len {
                    self.indices[i] = 0;
                    // 进位继续向左传播
                } else {
                    carry = false; // 没有溢出，进位结束
                }
            }
        }

        if carry {
            // 最左位也溢出了，说明当前长度所有组合已穷尽
            self.exhausted = true;
        }

        Some(password) // Some(x) 表示还有元素
    }
}

/// 创建暴力破解密码迭代器（对外的便捷包装函数）
#[allow(dead_code)]
pub fn generate_bruteforce_passwords(
    charset: &str,
    min_len: usize,
    max_len: usize,
) -> BruteForceIterator {
    BruteForceIterator::new(charset, min_len, max_len)
}

// ─── 掩码攻击 ─────────────────────────────────────────────────────
// 掩码攻击：用户指定密码的"模板"，每个位置可以是固定字符或某类字符集。
// 例如：`?d?dAB` 表示"两位数字 + 固定字符 AB"，等价于 00AB ~ 99AB。

/// 掩码字符集常量（各类字符集的定义）
const MASK_LOWERCASE: &str = "abcdefghijklmnopqrstuvwxyz";
const MASK_UPPERCASE: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const MASK_DIGITS: &str = "0123456789";
const MASK_SPECIAL: &str = "!@#$%^&*()_+-=[]{}|;:',.<>?/~`\"\\";
/// concat! 宏：在编译期将多个字符串字面量拼接为一个，无运行时开销
const MASK_ALL: &str = concat!(
    "abcdefghijklmnopqrstuvwxyz",
    "ABCDEFGHIJKLMNOPQRSTUVWXYZ",
    "0123456789",
    "!@#$%^&*()_+-=[]{}|;:',.<>?/~`\"\\"
);

/// 将掩码字符串解析为"每个位置的候选字符集"。
///
/// 掩码语法：
/// - `?l` → 小写字母
/// - `?u` → 大写字母
/// - `?d` → 数字
/// - `?s` → 特殊字符
/// - `?a` → 所有字符
/// - `??` → 字面量 `?`
/// - 其他字符 → 该字符本身（固定位）
///
/// 返回：每个槽位对应的 `Vec<char>`，槽位数 = 密码长度
fn parse_mask(mask: &str) -> Result<Vec<Vec<char>>, String> {
    if mask.is_empty() {
        return Err("掩码不能为空".to_string());
    }

    let chars: Vec<char> = mask.chars().collect();
    let mut slots = Vec::new();
    let mut index = 0;

    while index < chars.len() {
        if chars[index] == '?' {
            // `let Some(token) = ... else { return Err(...) }` 是 let-else 语法：
            // 如果解构失败（None）则执行 else 块（通常是提前返回）
            let Some(token) = chars.get(index + 1) else {
                return Err("掩码以未完成的 ? 结尾".to_string());
            };
            let charset = match token {
                'l' => MASK_LOWERCASE.chars().collect(),
                'u' => MASK_UPPERCASE.chars().collect(),
                'd' => MASK_DIGITS.chars().collect(),
                's' => MASK_SPECIAL.chars().collect(),
                'a' => MASK_ALL.chars().collect(),
                '?' => vec!['?'], // 转义：?? → 字面量 ?
                _ => {
                    return Err(format!("不支持的掩码标记: ?{}", token));
                }
            };
            slots.push(charset);
            index += 2; // 跳过 '?' 和后续字符
        } else {
            // 普通字符：固定位，只有一个候选字符
            slots.push(vec![chars[index]]);
            index += 1;
        }
    }

    if slots.is_empty() {
        return Err("掩码至少需要一个位置".to_string());
    }

    Ok(slots)
}

/// 掩码攻击迭代器：根据掩码模板枚举所有可能的密码。
/// 与 BruteForceIterator 类似，但每个位置的字符集可以不同（由掩码决定）。
pub struct MaskIterator {
    /// 每个槽位的候选字符集
    charsets: Vec<Vec<char>>,
    /// 当前各槽位的索引
    indices: Vec<usize>,
    /// 是否已穷尽所有组合
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

    /// 计算掩码的总组合数（各槽位字符集大小的乘积）。
    /// `fold` 是函数式编程中的"归约"操作：从初始值出发，逐步将每个元素折叠进去。
    /// 这里相当于：1 * charset[0].len() * charset[1].len() * ...
    pub fn total_combinations(mask: &str) -> Result<u64, String> {
        let charsets = parse_mask(mask)?;
        Ok(charsets.iter().fold(1_u64, |total, charset| {
            total.saturating_mul(charset.len() as u64)
        }))
    }

    /// 快速跳到第 n 个密码（断点续传用）。
    /// 算法：把 n 解码为各槽位的索引（类似不同进制混合的数字转换）。
    pub fn skip_to(&mut self, mut n: u64) {
        if self.done {
            return;
        }

        // 计算总数，n >= total 则直接标记为完成
        let total = self.charsets.iter().fold(1_u64, |total, charset| {
            total.saturating_mul(charset.len() as u64)
        });
        if n >= total {
            self.done = true;
            return;
        }

        // 从最右槽位开始解码：每位取余数得到该位的索引，然后整除进入上一位
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

        // enumerate() 同时给出索引和值：(0, &indices[0]), (1, &indices[1]), ...
        let password: String = self
            .indices
            .iter()
            .enumerate()
            .map(|(i, &index)| self.charsets[i][index])
            .collect();

        // 与 BruteForceIterator 相同的进位算法
        let mut carry = true;
        for i in (0..self.indices.len()).rev() {
            if carry {
                self.indices[i] += 1;
                if self.indices[i] >= self.charsets[i].len() {
                    self.indices[i] = 0;
                    // carry 继续
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

// ─── 并行 Worker ──────────────────────────────────────────────────

/// Worker 每处理这么多密码后，将批次计数刷入共享原子计数器并检查一次取消标志。
/// 较大的批次值减少原子操作频率（原子操作有轻微开销），但会让取消响应稍慢一点。
/// 1000 是经验上的权衡值。
const BATCH_SIZE: u64 = 1_000;

/// 为给定分片 [shard_start, shard_end) 构建密码迭代器。
///
/// # 分片设计
/// 将总密码空间切分为 N 段（N = worker 数），每个 worker 只负责自己的段，
/// 彼此互不重叠、也无需通信，是典型的"尴尬并行"（embarrassingly parallel）。
///
/// 返回 `Box<dyn Iterator<...> + Send + '_>`：
/// - `Box<dyn ...>`：装箱的 trait 对象，允许在运行时根据 mode 选择不同的迭代器类型
/// - `Send`：标记 trait，表示可以安全跨线程传送
/// - `'_`：生命周期省略，绑定到 mode 的生命周期
fn shard_passwords(
    mode: &AttackMode,
    shard_start: u64,
    shard_end: u64,
) -> Box<dyn Iterator<Item = String> + Send + '_> {
    match mode {
        AttackMode::Dictionary { wordlist } => Box::new(
            wordlist
                .iter()
                // skip 跳过前 shard_start 个元素，take 只取 (end - start) 个
                .skip(shard_start as usize)
                .take((shard_end - shard_start) as usize)
                // .cloned() 将 &String 克隆为 String（从引用变为拥有所有权的值）
                .cloned(),
        ),
        AttackMode::BruteForce {
            charset,
            min_length,
            max_length,
        } => {
            let mut iter = BruteForceIterator::new(charset, *min_length, *max_length);
            iter.skip_to(shard_start); // 快速跳到分片起始位置
            Box::new(iter.take((shard_end - shard_start) as usize))
        }
        AttackMode::Mask { mask } => {
            let mut iter = MaskIterator::new(mask)
                // expect：如果掩码在到达这里之前已被验证，则不应 panic
                .expect("mask mode should be validated before worker sharding");
            iter.skip_to(shard_start);
            Box::new(iter.take((shard_end - shard_start) as usize))
        }
    }
}

/// 所有格式 Worker 共用的核心循环。
///
/// # 参数
/// - `passwords`：当前分片的密码迭代器
/// - `cancel_flag`：取消标志，worker 定期检查；AtomicBool 无需锁
/// - `tried_counter`：共享计数器，多个 worker 原子累加；AtomicU64 无需锁
/// - `result_tx`：发现密码时通过通道发送给主线程
/// - `try_fn`：对每个密码执行的验证函数（闭包），返回 true 表示密码正确
///
/// # 为什么用 `FnMut`？
/// 验证函数（如 `try_password_on_archive`）可能需要修改内部状态（如 archive 位置），
/// 所以要求 `FnMut`（可变借用闭包），而不是 `Fn`（不可变借用）。
fn run_worker_inner<F>(
    passwords: impl Iterator<Item = String>,
    cancel_flag: &AtomicBool,
    tried_counter: &AtomicU64,
    result_tx: &mpsc::SyncSender<String>,
    mut try_fn: F,
) where
    F: FnMut(&str) -> bool,
{
    let mut batch_count: u64 = 0;
    for pw in passwords {
        // 每个批次开始时检查取消标志
        // Ordering::Relaxed：只保证原子性，不需要内存顺序保证（这里足够）
        if cancel_flag.load(Ordering::Relaxed) {
            // 将本批次已尝试数刷入共享计数器，然后退出
            tried_counter.fetch_add(batch_count, Ordering::Relaxed);
            return;
        }
        // 批次满了：刷新计数器，重置批次计数
        if batch_count >= BATCH_SIZE {
            tried_counter.fetch_add(batch_count, Ordering::Relaxed);
            batch_count = 0;
        }

        batch_count += 1;

        if try_fn(&pw) {
            // 找到密码：先刷新计数，再通过通道发送结果
            tried_counter.fetch_add(batch_count, Ordering::Relaxed);
            // `let _ = ...`：明确忽略返回值；send 可能失败（接收方已丢弃），
            // 但此时主线程已收到结果或任务已取消，失败无害
            let _ = result_tx.send(pw);
            return;
        }
    }
    // 分片遍历完毕，刷新剩余的批次计数
    tried_counter.fetch_add(batch_count, Ordering::Relaxed);
}

/// 将 worker 线程的 panic payload（`Box<dyn Any + Send>`）转换为可读字符串。
///
/// Rust 的 panic 可以携带任意类型的 payload（默认是 `&str` 或 `String`）。
/// `downcast_ref::<T>()` 尝试将 `dyn Any` 向下转型为具体类型 T，
/// 失败时返回 None（类似其他语言的 instanceof 检查）。
fn describe_worker_panic(payload: Box<dyn Any + Send + 'static>) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

/// 等待所有 worker 线程结束，收集 panic 信息。
///
/// `handle.join()` 返回 `Result<T, Box<dyn Any + Send>>`：
/// - `Ok(t)`：线程正常结束，返回线程函数的返回值
/// - `Err(payload)`：线程 panic 了，返回 panic 的 payload
fn join_worker_handles(handles: Vec<JoinHandle<()>>) -> Vec<String> {
    let mut panic_messages = Vec::new();

    for handle in handles {
        if let Err(payload) = handle.join() {
            panic_messages.push(describe_worker_panic(payload));
        }
    }

    panic_messages
}

/// 将多个 panic 信息格式化为一条错误消息（用于上报给前端）。
fn format_worker_panic_error(panic_messages: &[String]) -> String {
    if panic_messages.is_empty() {
        return "恢复 worker 异常退出".to_string();
    }

    if panic_messages.len() == 1 {
        return format!("恢复 worker 异常退出: {}", panic_messages[0]);
    }

    format!(
        "{} 个恢复 worker 异常退出: {}",
        panic_messages.len(),
        panic_messages.join("; ")
    )
}

/// 创建有界同步通道，容量为 worker 数量。
///
/// # 为什么用 `sync_channel` 而不是 `channel`？
/// - `channel`（异步通道）：发送方永不阻塞，缓冲区无限增长
/// - `sync_channel(n)`（同步通道）：缓冲区最多存 n 条消息，超出则发送方阻塞
/// 这里每个 worker 最多发送一条结果（找到密码即退出），
/// 容量设为 worker 数量可保证所有 worker 都能非阻塞地发送结果。
fn create_result_channel<T>(num_workers: u64) -> (mpsc::SyncSender<T>, mpsc::Receiver<T>) {
    let capacity = usize::try_from(num_workers).unwrap_or(usize::MAX).max(1);
    mpsc::sync_channel(capacity)
}

/// 从数据库读取断点，判断它是否适用于这次恢复。
///
/// 两层保护：
/// 1. 归档类型必须一致（避免切换格式后误用旧进度）
/// 2. 攻击模式必须一致（避免不同参数之间错误地"续跑"）
///
/// Tauri 的 `app_handle.state::<T>()` 是依赖注入机制：
/// 在 `lib.rs` 中注册的 State 可以在任意地方通过类型 T 取出。
fn load_resume_offset(
    app_handle: &tauri::AppHandle,
    task_id: &str,
    mode: &AttackMode,
    archive_type: &ArchiveType,
    total: u64,
) -> u64 {
    let db = app_handle.state::<Database>();
    match db.get_recovery_checkpoint(task_id) {
        // `if` guard in match arm：附加条件，只有 checkpoint 匹配当前模式和类型才使用
        Ok(Some(checkpoint))
            if checkpoint.archive_type == *archive_type && checkpoint.mode == *mode =>
        {
            // 用 min 防止数据库中存的值超过当前总数（防御性编程）
            checkpoint.tried.min(total)
        }
        Ok(_) => 0, // 没有断点，或模式不匹配，从头开始
        Err(error) => {
            log::error!("读取恢复断点失败: task={} error={}", task_id, error);
            0
        }
    }
}

/// 将当前进度写入数据库作为断点（UPSERT：存在则更新，不存在则插入）。
///
/// 主线程负责定期调用此函数（约每 500ms 一次），
/// worker 线程不直接操作数据库，避免频繁锁竞争。
fn persist_recovery_checkpoint(
    app_handle: &tauri::AppHandle,
    task_id: &str,
    mode: &AttackMode,
    archive_type: &ArchiveType,
    tried: u64,
    total: u64,
) {
    let db = app_handle.state::<Database>();
    let checkpoint = RecoveryCheckpoint {
        task_id: task_id.to_string(),
        // .clone() 克隆 mode 和 archive_type，因为 checkpoint 需要拥有它们的所有权
        mode: mode.clone(),
        archive_type: archive_type.clone(),
        tried,
        total,
        updated_at: Utc::now(),
    };

    if let Err(error) = db.upsert_recovery_checkpoint(&checkpoint) {
        log::error!("写入恢复断点失败: task={} error={}", task_id, error);
    }
}

/// ZIP 格式的单个 worker 分片函数。
///
/// # 为什么 ZIP 需要单独的函数？
/// `zip::ZipArchive` 没有实现 `Send`（不能跨线程传递），
/// 因此每个 worker 线程必须独立打开文件、创建自己的 ZipArchive 实例。
/// 这与 7z/RAR 的"每次密码尝试都重新打开"不同（ZIP 可以复用 archive 对象）。
fn run_zip_worker_shard(
    path: PathBuf,
    // Arc<AttackMode>：多个 worker 共享同一个 AttackMode，无需克隆整个 wordlist
    mode: Arc<AttackMode>,
    shard_start: u64,
    shard_end: u64,
    cancel_flag: Arc<AtomicBool>,
    tried_counter: Arc<AtomicU64>,
    result_tx: mpsc::SyncSender<String>,
) {
    // 在线程内部独立打开文件
    let file = match std::fs::File::open(&path) {
        Ok(f) => f,
        Err(_) => return, // 文件打开失败，静默退出（主线程会从错误路径处理）
    };
    let mut archive = match zip::ZipArchive::new(file) {
        Ok(a) => a,
        Err(_) => return,
    };
    // 找到第一个加密条目的索引（复用，避免每次密码尝试都重新查找）
    let encrypted_index = match (0..archive.len()).find(|&i| {
        archive
            .by_index_raw(i)
            .map(|entry| entry.encrypted() && !entry.is_dir())
            .unwrap_or(false)
    }) {
        Some(i) => i,
        None => return, // 没有加密条目
    };

    let passwords = shard_passwords(&mode, shard_start, shard_end);
    // 闭包捕获 archive 和 encrypted_index 的可变/不可变引用
    run_worker_inner(passwords, &cancel_flag, &tried_counter, &result_tx, |pw| {
        try_password_on_archive(&mut archive, encrypted_index, pw)
    });
}

/// 7z / RAR 格式的 worker 分片函数（无状态，每次密码尝试独立打开文件）。
///
/// 之所以叫 "stateless"，是因为每次 `try_fn` 调用都重新打开文件，
/// 没有可以复用的 archive 对象。相比 ZIP 的复用方式，IO 开销更大，
/// 但 sevenz_rust 和 unrar 库的 API 不支持复用。
fn run_stateless_worker_shard(
    path: PathBuf,
    archive_type: ArchiveType,
    mode: Arc<AttackMode>,
    shard_start: u64,
    shard_end: u64,
    cancel_flag: Arc<AtomicBool>,
    tried_counter: Arc<AtomicU64>,
    result_tx: mpsc::SyncSender<String>,
) {
    let passwords = shard_passwords(&mode, shard_start, shard_end);
    run_worker_inner(
        passwords,
        &cancel_flag,
        &tried_counter,
        &result_tx,
        // 闭包中根据 archive_type 分发到对应的验证函数
        |pw| match archive_type {
            ArchiveType::SevenZ => try_password_7z(&path, pw),
            ArchiveType::Rar => try_password_rar(&path, pw),
            _ => false,
        },
    );
}

// ─── 恢复主循环（多线程并行） ────────────────────────────────────

/// 主线程向前端发送进度事件的间隔（毫秒）
const PROGRESS_INTERVAL_MS: u64 = 500;

/// 恢复任务的三种终态（枚举）。
///
/// Rust 的枚举（enum）比其他语言强大得多：每个变体可以携带数据。
/// 这里 `Found` 携带找到的密码字符串，其余变体不携带额外数据。
#[derive(Debug)]
pub enum RecoveryResult {
    /// 成功找到密码，携带密码字符串
    Found(String),
    /// 穷尽所有候选密码，未找到
    Exhausted,
    /// 用户主动取消
    Cancelled,
}

/// 运行密码恢复任务（多线程并行版本）。
///
/// # 并行策略
/// 1. 计算总候选数（字典大小 / 暴力破解组合数 / 掩码组合数）
/// 2. 读取断点（如有），计算剩余候选数
/// 3. 将剩余空间均匀切分为 N 个分片（N ≈ CPU 数 - 1）
/// 4. 每个分片启动一个 worker 线程，线程间通过 Arc 共享只读数据
/// 5. 主线程轮询结果通道 + 进度更新，直到找到密码 / 穷尽 / 取消
///
/// # 返回值
/// - `Ok(RecoveryResult)` — 三种正常终态之一
/// - `Err(String)` — worker panic 或其他错误
pub fn run_recovery(
    config: RecoveryConfig,
    file_path: String,
    archive_type: ArchiveType,
    app_handle: tauri::AppHandle,
    cancel_flag: Arc<AtomicBool>,
) -> Result<RecoveryResult, String> {
    let path = Path::new(&file_path);
    if !path.exists() {
        return Err(format!("文件不存在: {}", file_path));
    }
    let path_buf = path.to_path_buf();
    let task_id = config.task_id.clone();
    // Arc::new 将 mode 放入引用计数指针，允许多个 worker 共享
    let mode = Arc::new(config.mode);

    // 提前验证文件是加密的，避免浪费资源
    validate_recovery_target(path, &archive_type)?;

    // 计算总候选密码数（用于进度显示和分片计算）
    let total = match mode.as_ref() {
        AttackMode::Dictionary { wordlist } => wordlist.len() as u64,
        AttackMode::BruteForce {
            charset,
            min_length,
            max_length,
        } => BruteForceIterator::total_combinations(
            charset.chars().count(),
            *min_length,
            *max_length,
        ),
        // `?` 把 MaskIterator::total_combinations 的 Err 向上传播
        AttackMode::Mask { mask } => MaskIterator::total_combinations(mask)?,
    };

    // 断点续传：从上次保存的进度继续，而非从 0 开始
    let resume_from =
        load_resume_offset(&app_handle, &task_id, mode.as_ref(), &archive_type, total);
    let remaining = total.saturating_sub(resume_from);

    // 计算 worker 数量：CPU 数 - 1（留一个核给主线程和 UI），但至少 1 个
    let num_workers = {
        let cpus = num_cpus::get() as u64;
        let n = std::cmp::max(1, cpus.saturating_sub(1));
        // 不要比剩余密码数还多的 worker（避免空分片）
        std::cmp::min(n, remaining.max(1))
    };
    let shard_size = remaining / num_workers;

    // 计算每个分片的 [start, end) 区间
    let shards: Vec<(u64, u64)> = (0..num_workers)
        .map(|i| {
            let start = resume_from + i * shard_size;
            // 最后一个分片取到 total（吸收整除余数）
            let end = if i == num_workers - 1 {
                total
            } else {
                start + shard_size
            };
            (start, end)
        })
        .collect();

    log::info!(
        "开始并行恢复: task={}, workers={}, total={}, resume_from={}, archive_type={:?}",
        task_id,
        num_workers,
        total,
        resume_from,
        archive_type
    );

    // 共享状态：tried_counter 从 resume_from 起步，这样进度是累计值而非"本次重启后的值"
    let tried_counter = Arc::new(AtomicU64::new(resume_from));
    let (result_tx, result_rx) = create_result_channel::<String>(num_workers);

    // 记录初始断点（即使 resume_from = 0，也保证 checkpoint 存在）
    persist_recovery_checkpoint(
        &app_handle,
        &task_id,
        mode.as_ref(),
        &archive_type,
        resume_from,
        total,
    );

    // 发送初始进度事件给前端（speed=0，让 UI 立即显示进度条）
    let start_time = Instant::now();
    let _ = app_handle.emit(
        "recovery-progress",
        RecoveryProgress {
            task_id: task_id.clone(),
            tried: resume_from,
            total,
            speed: 0.0,
            status: RecoveryStatus::Running,
            found_password: None,
            elapsed_seconds: 0.0,
        },
    );

    // 启动 worker 线程
    // `handles` 用 Option 包裹，方便后面 `.take()` 消费它（避免二次 join）
    let mut handles = Some(Vec::new());
    for (shard_start, shard_end) in shards {
        // 每个线程需要自己的数据副本：
        // - path_buf.clone()：克隆 PathBuf（廉价，因为是堆分配的字符串）
        // - Arc::clone(...)：只复制引用计数指针，不复制底层数据（O(1)）
        let path_clone = path_buf.clone();
        let mode_clone = Arc::clone(&mode);
        let cancel_clone = Arc::clone(&cancel_flag);
        let tried_clone = Arc::clone(&tried_counter);
        let tx_clone = result_tx.clone();
        let archive_type_clone = archive_type.clone();

        // std::thread::spawn 启动一个新线程，闭包前的 `move` 表示
        // 将所有捕获的变量移入线程（转移所有权），而不是借用
        let handle = std::thread::spawn(move || match archive_type_clone {
            ArchiveType::Zip => run_zip_worker_shard(
                path_clone,
                mode_clone,
                shard_start,
                shard_end,
                cancel_clone,
                tried_clone,
                tx_clone,
            ),
            ArchiveType::SevenZ | ArchiveType::Rar => run_stateless_worker_shard(
                path_clone,
                archive_type_clone,
                mode_clone,
                shard_start,
                shard_end,
                cancel_clone,
                tried_clone,
                tx_clone,
            ),
            ArchiveType::Unknown => {}
        });
        handles
            .as_mut()
            .expect("worker handles should be available before joining")
            .push(handle);
    }
    // 丢弃原始 sender：当所有 worker 的 tx_clone 也被丢弃时，
    // result_rx 会收到 Disconnected 错误，主线程由此知道所有 worker 已结束。
    drop(result_tx);

    // ─── 主线程轮询循环 ─────────────────────────────────────────────
    // 主线程每 50ms 醒来一次，检查结果通道 + 取消标志 + 进度上报
    let mut last_tried: u64 = 0;
    let mut last_poll_time = Instant::now();
    let poll_interval = Duration::from_millis(PROGRESS_INTERVAL_MS);

    // `loop { break value }` 是 Rust 的 loop-break-with-value：
    // loop 本身是一个表达式，break 携带的值就是整个 loop 的值
    let result = loop {
        std::thread::sleep(Duration::from_millis(50));

        let current_tried = tried_counter.load(Ordering::Relaxed);

        // 检查结果通道：非阻塞地尝试接收
        match result_rx.try_recv() {
            Ok(password) => {
                // 找到密码：通知所有 worker 退出，等待它们结束
                cancel_flag.store(true, Ordering::Relaxed);
                if let Some(worker_handles) = handles.take() {
                    let panic_messages = join_worker_handles(worker_handles);
                    for message in panic_messages {
                        log::error!("恢复 worker 在成功收敛后 panic: {}", message);
                    }
                }
                let elapsed = start_time.elapsed().as_secs_f64();
                let speed = if elapsed > 0.0 {
                    current_tried as f64 / elapsed
                } else {
                    0.0
                };
                let _ = app_handle.emit(
                    "recovery-progress",
                    RecoveryProgress {
                        task_id: task_id.clone(),
                        tried: current_tried,
                        total,
                        speed,
                        status: RecoveryStatus::Found,
                        found_password: Some(password.clone()),
                        elapsed_seconds: elapsed,
                    },
                );
                log::info!(
                    "密码已找到: {} (尝试 {} 次, 耗时 {:.1}s, 速度 {:.0} p/s)",
                    task_id,
                    current_tried,
                    elapsed,
                    speed
                );
                break Ok(RecoveryResult::Found(password));
            }
            // Disconnected：所有发送方（worker）都已关闭，通道没有更多消息
            Err(mpsc::TryRecvError::Disconnected) => {
                if let Some(worker_handles) = handles.take() {
                    let panic_messages = join_worker_handles(worker_handles);
                    if !panic_messages.is_empty() {
                        break Err(format_worker_panic_error(&panic_messages));
                    }
                }

                // 所有 worker 正常结束（未找到密码），根据取消标志判断终态
                break if cancel_flag.load(Ordering::Relaxed) {
                    Ok(RecoveryResult::Cancelled)
                } else {
                    Ok(RecoveryResult::Exhausted)
                };
            }
            // Empty：通道暂时没有消息，继续等待
            Err(mpsc::TryRecvError::Empty) => {}
        }

        // 检查外部取消信号（由 cancel_recovery 命令设置）
        if cancel_flag.load(Ordering::Relaxed) {
            if let Some(worker_handles) = handles.take() {
                let panic_messages = join_worker_handles(worker_handles);
                for message in panic_messages {
                    log::error!("恢复 worker 在取消收敛后 panic: {}", message);
                }
            }
            let elapsed = start_time.elapsed().as_secs_f64();
            let speed = if elapsed > 0.0 {
                current_tried as f64 / elapsed
            } else {
                0.0
            };
            // 取消时保存断点，下次可以从当前进度继续
            persist_recovery_checkpoint(
                &app_handle,
                &task_id,
                mode.as_ref(),
                &archive_type,
                current_tried,
                total,
            );
            let _ = app_handle.emit(
                "recovery-progress",
                RecoveryProgress {
                    task_id: task_id.clone(),
                    tried: current_tried,
                    total,
                    speed,
                    status: RecoveryStatus::Cancelled,
                    found_password: None,
                    elapsed_seconds: elapsed,
                },
            );
            log::info!(
                "恢复任务已取消: {} (已尝试 {} 个密码)",
                task_id,
                current_tried
            );
            break Ok(RecoveryResult::Cancelled);
        }

        // 按间隔上报进度（速度 = 本轮新增密码数 / 本轮耗时）
        let now = Instant::now();
        if now.duration_since(last_poll_time) >= poll_interval {
            let elapsed = start_time.elapsed().as_secs_f64();
            let delta = current_tried.saturating_sub(last_tried);
            let interval_secs = now.duration_since(last_poll_time).as_secs_f64();
            let speed = if interval_secs > 0.0 {
                delta as f64 / interval_secs
            } else {
                0.0
            };
            last_tried = current_tried;
            last_poll_time = now;
            // 定期持久化断点，减少意外中断的损失
            persist_recovery_checkpoint(
                &app_handle,
                &task_id,
                mode.as_ref(),
                &archive_type,
                current_tried,
                total,
            );

            let _ = app_handle.emit(
                "recovery-progress",
                RecoveryProgress {
                    task_id: task_id.clone(),
                    tried: current_tried,
                    total,
                    speed,
                    status: RecoveryStatus::Running,
                    found_password: None,
                    elapsed_seconds: elapsed,
                },
            );
        }
    };

    // 循环结束后，针对"穷尽"情况发送最终进度事件
    // Found 和 Cancelled 在循环内部已发送，无需重复
    if let Ok(ref r) = result {
        match r {
            RecoveryResult::Exhausted => {
                let elapsed = start_time.elapsed().as_secs_f64();
                let current_tried = tried_counter.load(Ordering::Relaxed);
                let speed = if elapsed > 0.0 {
                    current_tried as f64 / elapsed
                } else {
                    0.0
                };
                let _ = app_handle.emit(
                    "recovery-progress",
                    RecoveryProgress {
                        task_id: task_id.clone(),
                        tried: current_tried,
                        total,
                        speed,
                        status: RecoveryStatus::Exhausted,
                        found_password: None,
                        elapsed_seconds: elapsed,
                    },
                );
                log::info!(
                    "密码穷尽: {} (尝试 {} 次, 耗时 {:.1}s, 速度 {:.0} p/s)",
                    task_id,
                    current_tried,
                    elapsed,
                    speed
                );
            }
            _ => {} // Found 和 Cancelled 已在循环内处理
        }
    }

    result
}

// ─── 单元测试 ────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    // ─── 测试辅助函数 ───────────────────────────────────────────────

    /// 创建一个内容加密（非头部加密）的 7z 测试文件
    fn make_content_encrypted_7z(password: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("hello.txt");
        let archive = dir.path().join("content-encrypted.7z");
        std::fs::write(&source, "secret payload").unwrap();
        sevenz_rust::compress_to_path_encrypted(&source, &archive, Password::from(password))
            .unwrap();
        (dir, archive)
    }

    /// `env!("CARGO_MANIFEST_DIR")` 是编译期宏，返回 Cargo.toml 所在目录的路径。
    /// 用于定位测试用的 fixture 文件（预先准备好的测试用压缩包）。
    fn zip_fixtures_dir() -> std::path::PathBuf {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest_dir.parent().unwrap().join("fixtures").join("zip")
    }

    fn sevenz_fixtures_dir() -> std::path::PathBuf {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest_dir.parent().unwrap().join("fixtures").join("7z")
    }

    fn rar_fixtures_dir() -> std::path::PathBuf {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest_dir.parent().unwrap().join("fixtures").join("rar")
    }

    /// 创建一个内容为指定字节的临时"假"压缩包（用于测试错误处理）
    fn write_fake_archive(name: &str, bytes: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(bytes).unwrap();
        (dir, path)
    }

    // ─── BruteForceIterator 测试 ────────────────────────────────────

    #[test]
    fn bruteforce_abc_1_2_produces_12_items() {
        let items: Vec<String> = BruteForceIterator::new("abc", 1, 2).collect();
        assert_eq!(
            items,
            vec!["a", "b", "c", "aa", "ab", "ac", "ba", "bb", "bc", "ca", "cb", "cc"]
        );
        assert_eq!(items.len(), 12);
    }

    #[test]
    fn bruteforce_ab_1_1_produces_a_b() {
        let items: Vec<String> = BruteForceIterator::new("ab", 1, 1).collect();
        assert_eq!(items, vec!["a", "b"]);
    }

    #[test]
    fn bruteforce_empty_charset_produces_nothing() {
        let items: Vec<String> = BruteForceIterator::new("", 1, 3).collect();
        assert!(items.is_empty());
    }

    #[test]
    fn bruteforce_min_len_zero_treated_as_one() {
        let items: Vec<String> = BruteForceIterator::new("a", 0, 2).collect();
        assert_eq!(items, vec!["a", "aa"]);
    }

    #[test]
    fn bruteforce_max_less_than_min_clamped() {
        // max < min → 钳位为 min，所以得到所有 3 字符的 "ab" 组合（2^3 = 8 个）
        let items: Vec<String> = BruteForceIterator::new("ab", 3, 1).collect();
        assert_eq!(items.len(), 8);
        assert_eq!(items[0], "aaa");
        assert_eq!(items[7], "bbb");
    }

    // ─── skip_to 测试 ───────────────────────────────────────────────

    #[test]
    fn bruteforce_skip_to_matches_sequential() {
        // skip_to(3) 后的序列应与完整序列的第 3 个元素之后完全一致
        let full: Vec<String> = BruteForceIterator::new("abc", 1, 2).collect();
        let mut iter = BruteForceIterator::new("abc", 1, 2);
        iter.skip_to(3);
        let rest: Vec<String> = iter.collect();
        assert_eq!(&full[3..], &rest[..]);
    }

    #[test]
    fn bruteforce_skip_to_zero_is_noop() {
        let full: Vec<String> = BruteForceIterator::new("ab", 1, 2).collect();
        let mut iter = BruteForceIterator::new("ab", 1, 2);
        iter.skip_to(0);
        let rest: Vec<String> = iter.collect();
        assert_eq!(full, rest);
    }

    #[test]
    fn bruteforce_skip_to_past_end_produces_nothing() {
        let mut iter = BruteForceIterator::new("ab", 1, 2);
        iter.skip_to(999);
        assert!(iter.next().is_none());
    }

    #[test]
    fn bruteforce_skip_to_exact_boundary() {
        let full: Vec<String> = BruteForceIterator::new("ab", 1, 2).collect();
        for skip in 0..6 {
            let mut iter = BruteForceIterator::new("ab", 1, 2);
            iter.skip_to(skip);
            let rest: Vec<String> = iter.collect();
            assert_eq!(
                &full[skip as usize..],
                &rest[..],
                "skip_to({}) mismatch",
                skip
            );
        }
    }

    // ─── shard_passwords 测试 ───────────────────────────────────────

    #[test]
    fn shard_passwords_bruteforce_covers_full_space() {
        let mode = AttackMode::BruteForce {
            charset: "ab".to_string(),
            min_length: 1,
            max_length: 2,
        };
        let shard_0: Vec<String> = shard_passwords(&mode, 0, 3).collect();
        let shard_1: Vec<String> = shard_passwords(&mode, 3, 6).collect();
        let full: Vec<String> = BruteForceIterator::new("ab", 1, 2).collect();

        // 两个分片拼接后应等于完整序列
        assert_eq!([shard_0, shard_1].concat(), full);
    }

    #[test]
    fn shard_passwords_dictionary_covers_full_space() {
        let mode = AttackMode::Dictionary {
            wordlist: vec!["a".into(), "b".into(), "c".into(), "d".into(), "e".into()],
        };
        let shard_0: Vec<String> = shard_passwords(&mode, 0, 2).collect();
        let shard_1: Vec<String> = shard_passwords(&mode, 2, 5).collect();
        assert_eq!(shard_0, vec!["a", "b"]);
        assert_eq!(shard_1, vec!["c", "d", "e"]);
    }

    #[test]
    fn parse_mask_supports_literals_and_tokens() {
        // 掩码 "?dA??" 解析为：[数字集, 字面量'A', 字面量'?']（共 3 个槽位）
        let slots = parse_mask("?dA??").unwrap();
        assert_eq!(slots.len(), 3);
        assert_eq!(slots[0].len(), 10); // ?d = 10 个数字
        assert_eq!(slots[1], vec!['A']); // 字面量 A
        assert_eq!(slots[2], vec!['?']); // ?? = 字面量 ?
    }

    #[test]
    fn mask_iterator_generates_expected_sequence() {
        // "?dA" = 一位数字 + 固定 A，应生成 "0A", "1A", ..., "9A"
        let items: Vec<String> = MaskIterator::new("?dA").unwrap().collect();
        assert_eq!(items.first().unwrap(), "0A");
        assert_eq!(items.last().unwrap(), "9A");
        assert_eq!(items.len(), 10);
    }

    #[test]
    fn mask_iterator_skip_to_matches_sequence() {
        let full: Vec<String> = MaskIterator::new("?l?d").unwrap().collect();
        let mut iter = MaskIterator::new("?l?d").unwrap();
        iter.skip_to(13);
        let rest: Vec<String> = iter.collect();
        assert_eq!(&full[13..], &rest[..]);
    }

    #[test]
    fn total_combinations_for_mask_is_correct() {
        // "?d?dA" = 10 * 10 * 1 = 100
        assert_eq!(MaskIterator::total_combinations("?d?dA").unwrap(), 100);
    }

    #[test]
    fn shard_passwords_mask_covers_full_space() {
        let mode = AttackMode::Mask {
            mask: "?d?d".to_string(),
        };
        let shard_0: Vec<String> = shard_passwords(&mode, 0, 50).collect();
        let shard_1: Vec<String> = shard_passwords(&mode, 50, 100).collect();
        let full: Vec<String> = MaskIterator::new("?d?d").unwrap().collect();
        assert_eq!([shard_0, shard_1].concat(), full);
    }

    // ─── validate_recovery_target 测试 ─────────────────────────────

    #[test]
    fn validate_recovery_target_rejects_corrupt_7z() {
        // 写入 7z 魔数头但内容残缺（模拟损坏文件）
        let (_dir, path) = write_fake_archive(
            "broken.7z",
            &[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C, 0x00, 0x00],
        );

        let error = validate_recovery_target(&path, &ArchiveType::SevenZ).unwrap_err();
        assert!(error.contains("无法解析 7z 文件"));
    }

    #[test]
    fn validate_recovery_target_rejects_corrupt_rar() {
        let (_dir, path) = write_fake_archive(
            "broken.rar",
            &[0x52, 0x61, 0x72, 0x21, 0x1A, 0x07, 0x00, 0x00],
        );

        let error = validate_recovery_target(&path, &ArchiveType::Rar).unwrap_err();
        assert!(error.contains("RAR"));
    }

    // ─── worker panic 处理测试 ──────────────────────────────────────

    #[test]
    fn join_worker_handles_reports_panics() {
        // 让线程 panic，验证 panic 信息被正确捕获
        let handle = std::thread::spawn(|| panic!("boom"));
        let panic_messages = join_worker_handles(vec![handle]);

        assert_eq!(panic_messages.len(), 1);
        assert!(panic_messages[0].contains("boom"));
    }

    #[test]
    fn result_channel_buffers_one_hit_per_worker() {
        // 验证有界通道容量足够容纳所有 worker 的结果
        let (tx, rx) = create_result_channel::<String>(3);

        let handles: Vec<_> = ["pw1", "pw2", "pw3"]
            .into_iter()
            .map(|password| {
                let tx = tx.clone();
                std::thread::spawn(move || {
                    tx.send(password.to_string()).unwrap();
                })
            })
            .collect();
        drop(tx); // 丢弃原始 sender

        let mut results = Vec::new();
        for _ in 0..3 {
            results.push(
                rx.recv_timeout(std::time::Duration::from_secs(1))
                    .expect("all worker hits should fit into the result channel"),
            );
        }
        results.sort();

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(results, vec!["pw1", "pw2", "pw3"]);
    }

    // ─── total_combinations 测试 ────────────────────────────────────

    #[test]
    fn total_combinations_2_1_2() {
        // charset=2, min=1, max=2：2^1 + 2^2 = 2 + 4 = 6
        assert_eq!(BruteForceIterator::total_combinations(2, 1, 2), 6);
    }

    #[test]
    fn total_combinations_26_1_3() {
        // 26 + 676 + 17576 = 18278
        assert_eq!(BruteForceIterator::total_combinations(26, 1, 3), 18278);
    }

    #[test]
    fn total_combinations_zero_charset() {
        assert_eq!(BruteForceIterator::total_combinations(0, 1, 3), 0);
    }

    #[test]
    fn total_combinations_min_zero_treated_as_one() {
        // min=0 等价于 min=1
        assert_eq!(BruteForceIterator::total_combinations(2, 0, 2), 6);
    }

    // ─── try_password_zip 测试 ──────────────────────────────────────

    #[test]
    fn try_password_zip_correct_on_aes() {
        let path = zip_fixtures_dir().join("encrypted-aes.zip");
        assert!(try_password_zip(&path, "test123"));
    }

    #[test]
    fn try_password_zip_wrong_on_aes() {
        let path = zip_fixtures_dir().join("encrypted-aes.zip");
        assert!(!try_password_zip(&path, "wrong"));
    }

    #[test]
    fn try_password_zip_correct_on_strong() {
        let path = zip_fixtures_dir().join("encrypted-strong.zip");
        assert!(try_password_zip(&path, "Str0ng!P@ss"));
    }

    #[test]
    fn try_password_zip_on_unencrypted_returns_false() {
        let path = zip_fixtures_dir().join("normal.zip");
        assert!(!try_password_zip(&path, "anything"));
    }

    #[test]
    fn try_password_zip_nonexistent_file_returns_false() {
        let path = zip_fixtures_dir().join("does-not-exist.zip");
        assert!(!try_password_zip(&path, "test"));
    }

    // ─── try_password_on_archive 测试 ──────────────────────────────

    #[test]
    fn try_password_on_archive_correct() {
        let path = zip_fixtures_dir().join("encrypted-aes.zip");
        let file = std::fs::File::open(&path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();

        let index = (0..archive.len())
            .find(|&i| {
                archive
                    .by_index_raw(i)
                    .map(|e| e.encrypted() && !e.is_dir())
                    .unwrap_or(false)
            })
            .expect("should have encrypted entry");

        assert!(try_password_on_archive(&mut archive, index, "test123"));
    }

    #[test]
    fn try_password_on_archive_wrong() {
        let path = zip_fixtures_dir().join("encrypted-aes.zip");
        let file = std::fs::File::open(&path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();

        let index = (0..archive.len())
            .find(|&i| {
                archive
                    .by_index_raw(i)
                    .map(|e| e.encrypted() && !e.is_dir())
                    .unwrap_or(false)
            })
            .expect("should have encrypted entry");

        assert!(!try_password_on_archive(
            &mut archive,
            index,
            "wrong_password"
        ));
    }

    // ─── try_password_7z 测试 ───────────────────────────────────────

    #[test]
    fn try_password_7z_correct() {
        let path = sevenz_fixtures_dir().join("encrypted.7z");
        assert!(try_password_7z(&path, "test123"));
    }

    #[test]
    fn try_password_7z_wrong() {
        let path = sevenz_fixtures_dir().join("encrypted.7z");
        assert!(!try_password_7z(&path, "wrong_password"));
    }

    #[test]
    fn try_password_7z_on_unencrypted_returns_false() {
        let path = sevenz_fixtures_dir().join("normal.7z");
        assert!(!try_password_7z(&path, "anything"));
    }

    #[test]
    fn try_password_7z_content_encrypted_without_encrypted_headers() {
        let (_dir, path) = make_content_encrypted_7z("test123");
        assert!(try_password_7z(&path, "test123"));
        assert!(!try_password_7z(&path, "wrong_password"));
    }

    #[test]
    fn try_password_7z_nonexistent_file_returns_false() {
        let path = sevenz_fixtures_dir().join("does-not-exist.7z");
        assert!(!try_password_7z(&path, "test"));
    }

    // ─── try_password_rar 测试 ──────────────────────────────────────

    #[test]
    fn try_password_rar_correct() {
        let path = rar_fixtures_dir().join("encrypted.rar");
        assert!(try_password_rar(&path, "unrar"));
    }

    #[test]
    fn try_password_rar_wrong() {
        let path = rar_fixtures_dir().join("encrypted.rar");
        assert!(!try_password_rar(&path, "wrong_password"));
    }

    #[test]
    fn try_password_rar_on_unencrypted_returns_false() {
        let path = rar_fixtures_dir().join("normal.rar");
        assert!(!try_password_rar(&path, "anything"));
    }

    #[test]
    fn try_password_rar_encrypted_headers_correct() {
        let path = rar_fixtures_dir().join("encrypted-headers.rar");
        assert!(try_password_rar(&path, "password"));
    }

    #[test]
    fn try_password_rar_encrypted_headers_wrong() {
        let path = rar_fixtures_dir().join("encrypted-headers.rar");
        assert!(!try_password_rar(&path, "wrong"));
    }

    #[test]
    fn try_password_rar_nonexistent_file_returns_false() {
        let path = rar_fixtures_dir().join("does-not-exist.rar");
        assert!(!try_password_rar(&path, "test"));
    }

    // ─── 并行 worker 集成测试 ───────────────────────────────────────

    #[test]
    fn parallel_zip_worker_finds_correct_password() {
        let path = zip_fixtures_dir().join("encrypted-aes.zip");
        let cancel = Arc::new(AtomicBool::new(false));
        let counter = Arc::new(AtomicU64::new(0));
        let (tx, rx) = mpsc::sync_channel(1);

        let mode = AttackMode::Dictionary {
            wordlist: vec![
                "wrong1".to_string(),
                "wrong2".to_string(),
                "wrong3".to_string(),
                "test123".to_string(),
                "wrong4".to_string(),
            ],
        };

        run_zip_worker_shard(path, Arc::new(mode), 0, 5, cancel, counter.clone(), tx);

        let found = rx.recv().expect("should find password");
        assert_eq!(found, "test123");
        assert!(counter.load(Ordering::Relaxed) > 0);
    }

    #[test]
    fn parallel_7z_worker_finds_correct_password() {
        let path = sevenz_fixtures_dir().join("encrypted.7z");
        let cancel = Arc::new(AtomicBool::new(false));
        let counter = Arc::new(AtomicU64::new(0));
        let (tx, rx) = mpsc::sync_channel(1);

        let mode = AttackMode::Dictionary {
            wordlist: vec![
                "bad1".to_string(),
                "bad2".to_string(),
                "test123".to_string(),
            ],
        };

        run_stateless_worker_shard(
            path,
            ArchiveType::SevenZ,
            Arc::new(mode),
            0,
            3,
            cancel,
            counter,
            tx,
        );

        let found = rx.recv().expect("should find password");
        assert_eq!(found, "test123");
    }

    #[test]
    fn parallel_rar_worker_finds_correct_password() {
        let path = rar_fixtures_dir().join("encrypted.rar");
        let cancel = Arc::new(AtomicBool::new(false));
        let counter = Arc::new(AtomicU64::new(0));
        let (tx, rx) = mpsc::sync_channel(1);

        let mode = AttackMode::Dictionary {
            wordlist: vec![
                "nope".to_string(),
                "unrar".to_string(),
                "also_nope".to_string(),
            ],
        };

        run_stateless_worker_shard(
            path,
            ArchiveType::Rar,
            Arc::new(mode),
            0,
            3,
            cancel,
            counter,
            tx,
        );

        let found = rx.recv().expect("should find password");
        assert_eq!(found, "unrar");
    }

    #[test]
    fn parallel_worker_respects_cancel_flag() {
        let path = zip_fixtures_dir().join("encrypted-aes.zip");
        // 预先设置取消标志为 true，worker 应立即退出而不尝试任何密码
        let cancel = Arc::new(AtomicBool::new(true));
        let counter = Arc::new(AtomicU64::new(0));
        let (tx, rx) = mpsc::sync_channel(1);

        let mode = AttackMode::Dictionary {
            wordlist: vec!["test123".to_string()],
        };

        run_zip_worker_shard(path, Arc::new(mode), 0, 1, cancel, counter, tx);

        // 通道应为空：worker 在取消标志下提前退出，没有发送任何结果
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn parallel_multi_worker_zip_finds_password() {
        // 模拟 3 个 worker，密码在第 3 个分片中
        let path = zip_fixtures_dir().join("encrypted-aes.zip");
        let cancel = Arc::new(AtomicBool::new(false));
        let counter = Arc::new(AtomicU64::new(0));
        let (tx, rx) = mpsc::sync_channel(1);

        let words = vec![
            "w1".to_string(),
            "w2".to_string(),
            "w3".to_string(),
            "w4".to_string(),
            "w5".to_string(),
            "test123".to_string(),
        ];
        let mode = Arc::new(AttackMode::Dictionary { wordlist: words });

        // 3 个 worker，分片：[0,2), [2,4), [4,6)
        let mut handles = vec![];
        for (s, e) in [(0u64, 2u64), (2, 4), (4, 6)] {
            let p = path.clone();
            let m = Arc::clone(&mode);
            let c = Arc::clone(&cancel);
            let t = Arc::clone(&counter);
            let tx2 = tx.clone();
            handles.push(std::thread::spawn(move || {
                run_zip_worker_shard(p, m, s, e, c, t, tx2);
            }));
        }
        drop(tx); // 丢弃原始 sender，使通道在所有 worker 结束后自动关闭

        let found = rx.recv().expect("some worker should find password");
        assert_eq!(found, "test123");
        // 通知所有 worker 退出（其他 worker 可能还在运行）
        cancel.store(true, Ordering::Relaxed);
        for h in handles {
            let _ = h.join();
        }
    }
}
