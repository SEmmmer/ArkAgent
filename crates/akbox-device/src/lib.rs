use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScreenshotCaptureRequest {
    pub adb_executable: Option<PathBuf>,
}

pub fn capture_device_screenshot_png(
    _request: &ScreenshotCaptureRequest,
) -> Result<Vec<u8>, ScreenshotCaptureError> {
    Err(ScreenshotCaptureError::BackendNotReady)
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ScreenshotCaptureError {
    #[error("阶段 4 尚未接入 MuMu / ADB 截图链路")]
    BackendNotReady,
}

#[cfg(test)]
mod tests {
    use super::ScreenshotCaptureError;
    use super::ScreenshotCaptureRequest;
    use super::capture_device_screenshot_png;
    use std::path::PathBuf;

    #[test]
    fn request_can_store_optional_adb_path() {
        let request = ScreenshotCaptureRequest {
            adb_executable: Some(PathBuf::from("C:/MuMu/adb.exe")),
        };

        assert_eq!(
            request.adb_executable,
            Some(PathBuf::from("C:/MuMu/adb.exe"))
        );
    }

    #[test]
    fn capture_returns_backend_not_ready_until_m4() {
        let error =
            capture_device_screenshot_png(&ScreenshotCaptureRequest::default()).unwrap_err();

        assert_eq!(error, ScreenshotCaptureError::BackendNotReady);
    }
}
