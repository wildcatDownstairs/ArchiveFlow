use tauri::{command, AppHandle};

use crate::errors::AppError;
use crate::services::app_log_service;

#[command]
pub async fn append_app_log(
    app_handle: AppHandle,
    level: Option<String>,
    category: Option<String>,
    message: String,
) -> Result<(), AppError> {
    app_log_service::append_app_log(
        &app_handle,
        level.as_deref().unwrap_or("INFO"),
        category.as_deref(),
        &message,
    )
}

#[command]
pub async fn get_log_dir(app_handle: AppHandle) -> Result<String, AppError> {
    Ok(app_log_service::log_dir(&app_handle)?
        .to_string_lossy()
        .to_string())
}

#[command]
pub async fn open_log_dir(app_handle: AppHandle) -> Result<String, AppError> {
    Ok(app_log_service::open_log_dir(&app_handle)?
        .to_string_lossy()
        .to_string())
}
