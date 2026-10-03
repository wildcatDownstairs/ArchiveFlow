use crate::errors::AppError;
use crate::services::app_log_service;

pub fn append_app_log(
    app_handle: crate::runtime::AppRuntime,
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

pub fn get_log_dir(app_handle: crate::runtime::AppRuntime) -> Result<String, AppError> {
    Ok(app_log_service::log_dir(&app_handle)?
        .to_string_lossy()
        .to_string())
}

pub fn open_log_dir(app_handle: crate::runtime::AppRuntime) -> Result<String, AppError> {
    Ok(app_log_service::open_log_dir(&app_handle)?
        .to_string_lossy()
        .to_string())
}
