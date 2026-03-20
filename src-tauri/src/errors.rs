use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("数据库错误: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("文件错误: {0}")]
    FileError(String),

    #[error("压缩包解析错误: {0}")]
    ArchiveError(String),

    #[error("任务不存在: {0}")]
    TaskNotFound(String),

    #[error("无效参数: {0}")]
    InvalidArgument(String),

    #[error("JSON 序列化错误: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("操作未授权")]
    Unauthorized,
}

impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.to_string().as_str())
    }
}
