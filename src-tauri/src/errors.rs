// thiserror 是一个常用的第三方库，用宏（#[derive(Error)]）帮你自动实现
// std::error::Error trait，省去大量样板代码。
use thiserror::Error;

// #[derive(Error, Debug)] 让编译器自动为这个枚举实现：
//   - std::error::Error trait（thiserror 提供）
//   - std::fmt::Debug trait（标准库提供，让你可以用 {:?} 打印）
//
// 在 Rust 中，枚举（enum）非常适合用来表示"多种可能的错误类型"，
// 每个变体（variant）代表一种具体的错误情况。
#[derive(Error, Debug)]
pub enum AppError {
    // #[error("...")] 定义这个错误变体被打印时显示的文字。
    // {0} 表示第一个字段的值。
    //
    // #[from] 让 Rust 自动把 rusqlite::Error 转换成 AppError::Database。
    // 这样在函数里遇到数据库错误时，可以直接用 `?` 操作符传播，
    // 而不需要手动写 .map_err(AppError::Database)。
    #[error("数据库错误: {0}")]
    Database(#[from] rusqlite::Error),

    // 文件相关错误，直接包含一个描述字符串。
    // String 是堆分配的字符串，适合存储运行时才知道内容的错误信息。
    #[error("文件错误: {0}")]
    FileError(String),

    // 压缩包解析失败时使用这个变体。
    #[error("压缩包解析错误: {0}")]
    ArchiveError(String),

    // 当用户指定的任务 ID 在数据库中找不到时使用。
    #[error("任务不存在: {0}")]
    TaskNotFound(String),

    // 传入参数不合法时使用，例如空字符串、超出范围的数值等。
    #[error("无效参数: {0}")]
    InvalidArgument(String),

    // serde_json::Error 可以通过 #[from] 自动转换，
    // 适用于 JSON 序列化/反序列化失败的场景。
    #[error("JSON 序列化错误: {0}")]
    SerializationError(#[from] serde_json::Error),

    // #[allow(dead_code)] 告诉编译器：即使这个变体当前没被使用，
    // 也不要发出"未使用代码"的警告。这在预留功能时很常见。
    #[error("操作未授权")]
    #[allow(dead_code)]
    Unauthorized,
}

// Tauri 的 #[command] 函数返回 Result<T, E> 时，E 必须实现 serde::Serialize，
// 这样错误才能被序列化成 JSON 发送给前端 JavaScript。
// 标准库的 Error trait 并没有实现 Serialize，所以我们手动实现：
// 把错误转成字符串再序列化，简单有效。
impl serde::Serialize for AppError {
    // S: serde::Serializer 是泛型约束（trait bound），
    // 意思是"S 必须是某种能序列化数据的类型"。
    // 这里用 where 子句写是 Rust 的标准风格，与 fn serialize<S: serde::Serializer> 等价。
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // self.to_string() 调用 Display trait（由 #[error("...")] 自动生成），
        // 得到人类可读的错误字符串，再序列化为 JSON 字符串。
        serializer.serialize_str(self.to_string().as_str())
    }
}
