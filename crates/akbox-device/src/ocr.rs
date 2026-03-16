use image::DynamicImage;
use image::GenericImageView;
use image::ImageFormat;
use image::imageops::FilterType;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[cfg(windows)]
use windows::Globalization::Language;
#[cfg(windows)]
use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
#[cfg(windows)]
use windows::Media::Ocr::{OcrEngine, OcrResult as WindowsOcrResult};
#[cfg(windows)]
use windows::Security::Cryptography::CryptographicBuffer;
#[cfg(windows)]
use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
#[cfg(windows)]
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OcrBackend {
    WindowsNative,
    Stub,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcrRequest {
    #[serde(default = "default_ocr_languages")]
    pub languages: Vec<String>,
    #[serde(default)]
    pub numeric_only: bool,
    pub hint_text: Option<String>,
}

impl Default for OcrRequest {
    fn default() -> Self {
        Self {
            languages: default_ocr_languages(),
            numeric_only: false,
            hint_text: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcrLine {
    pub text: String,
    pub confidence: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcrResult {
    pub backend: OcrBackend,
    pub text: String,
    pub lines: Vec<OcrLine>,
}

pub fn recognize_text_from_png(
    png_bytes: &[u8],
    request: &OcrRequest,
) -> Result<OcrResult, OcrError> {
    let image = image::load_from_memory_with_format(png_bytes, ImageFormat::Png)
        .map_err(|source| OcrError::InvalidPng { source })?;

    #[cfg(windows)]
    {
        recognize_text_with_windows_backend(image, request)
    }
    #[cfg(not(windows))]
    {
        let _ = image;
        let _ = request;
        Err(OcrError::BackendUnavailable {
            backend: OcrBackend::Stub,
            details: "当前构建目标不是 Windows，无法使用 Windows OCR 后端".to_string(),
        })
    }
}

#[cfg(windows)]
fn recognize_text_with_windows_backend(
    image: DynamicImage,
    request: &OcrRequest,
) -> Result<OcrResult, OcrError> {
    let _apartment = initialize_winrt_apartment()?;
    let max_dimension = OcrEngine::MaxImageDimension().map_err(|source| OcrError::WindowsApi {
        context: "读取 Windows OCR 最大图像尺寸",
        source,
    })?;
    let prepared = prepare_image_for_windows_ocr(image, max_dimension);
    let engine = create_windows_ocr_engine(request)?;
    let software_bitmap = create_windows_software_bitmap(&prepared)?;
    let raw_result = engine
        .RecognizeAsync(&software_bitmap)
        .map_err(|source| OcrError::WindowsApi {
            context: "启动 Windows OCR 识别任务",
            source,
        })?
        .get()
        .map_err(|source| OcrError::WindowsApi {
            context: "等待 Windows OCR 识别结果",
            source,
        })?;

    build_ocr_result_from_windows(raw_result, request)
}

#[cfg(windows)]
fn create_windows_ocr_engine(request: &OcrRequest) -> Result<OcrEngine, OcrError> {
    for language_tag in request.languages.iter().map(|value| value.trim()) {
        if language_tag.is_empty() {
            continue;
        }

        let language_tag = windows::core::HSTRING::from(language_tag);
        let is_well_formed =
            Language::IsWellFormed(&language_tag).map_err(|source| OcrError::WindowsApi {
                context: "校验 OCR 语言标签格式",
                source,
            })?;
        if !is_well_formed {
            continue;
        }

        let language =
            Language::CreateLanguage(&language_tag).map_err(|source| OcrError::WindowsApi {
                context: "创建 OCR 语言实例",
                source,
            })?;
        let is_supported =
            OcrEngine::IsLanguageSupported(&language).map_err(|source| OcrError::WindowsApi {
                context: "检查 OCR 语言支持情况",
                source,
            })?;
        if is_supported {
            return OcrEngine::TryCreateFromLanguage(&language).map_err(|source| {
                OcrError::WindowsApi {
                    context: "按指定语言创建 Windows OCR 引擎",
                    source,
                }
            });
        }
    }

    if let Ok(engine) = OcrEngine::TryCreateFromUserProfileLanguages() {
        return Ok(engine);
    }

    let available =
        OcrEngine::AvailableRecognizerLanguages().map_err(|source| OcrError::WindowsApi {
            context: "读取系统可用 OCR 语言列表",
            source,
        })?;
    let available_size = available.Size().map_err(|source| OcrError::WindowsApi {
        context: "读取系统可用 OCR 语言列表长度",
        source,
    })?;
    if available_size > 0 {
        let language = available.GetAt(0).map_err(|source| OcrError::WindowsApi {
            context: "读取首个可用 OCR 语言",
            source,
        })?;
        return OcrEngine::TryCreateFromLanguage(&language).map_err(|source| {
            OcrError::WindowsApi {
                context: "按系统首个可用语言创建 Windows OCR 引擎",
                source,
            }
        });
    }

    Err(OcrError::BackendUnavailable {
        backend: OcrBackend::WindowsNative,
        details: format!(
            "当前系统没有可用的 Windows OCR 识别语言；请求语言：{}",
            if request.languages.is_empty() {
                "<空>".to_string()
            } else {
                request.languages.join(", ")
            }
        ),
    })
}

fn default_ocr_languages() -> Vec<String> {
    vec!["zh-CN".to_string()]
}

#[cfg(windows)]
fn create_windows_software_bitmap(prepared: &PreparedOcrImage) -> Result<SoftwareBitmap, OcrError> {
    let width = i32::try_from(prepared.width).map_err(|_| OcrError::BackendUnavailable {
        backend: OcrBackend::WindowsNative,
        details: format!(
            "OCR 图像宽度超出 Windows SoftwareBitmap 支持范围：{}",
            prepared.width
        ),
    })?;
    let height = i32::try_from(prepared.height).map_err(|_| OcrError::BackendUnavailable {
        backend: OcrBackend::WindowsNative,
        details: format!(
            "OCR 图像高度超出 Windows SoftwareBitmap 支持范围：{}",
            prepared.height
        ),
    })?;
    let buffer = CryptographicBuffer::CreateFromByteArray(&prepared.pixels).map_err(|source| {
        OcrError::WindowsApi {
            context: "将 OCR 像素写入 WinRT Buffer",
            source,
        }
    })?;

    SoftwareBitmap::CreateCopyFromBuffer(&buffer, BitmapPixelFormat::Gray8, width, height).map_err(
        |source| OcrError::WindowsApi {
            context: "从灰度像素创建 SoftwareBitmap",
            source,
        },
    )
}

#[cfg(windows)]
fn build_ocr_result_from_windows(
    raw_result: WindowsOcrResult,
    request: &OcrRequest,
) -> Result<OcrResult, OcrError> {
    let lines_view = raw_result.Lines().map_err(|source| OcrError::WindowsApi {
        context: "读取 Windows OCR 行结果",
        source,
    })?;
    let line_count = lines_view.Size().map_err(|source| OcrError::WindowsApi {
        context: "读取 Windows OCR 行数",
        source,
    })?;
    let mut lines = Vec::new();
    for index in 0..line_count {
        let line = lines_view
            .GetAt(index)
            .map_err(|source| OcrError::WindowsApi {
                context: "读取 Windows OCR 单行结果",
                source,
            })?;
        let text = normalize_ocr_text(
            &line
                .Text()
                .map_err(|source| OcrError::WindowsApi {
                    context: "读取 Windows OCR 单行文本",
                    source,
                })?
                .to_string(),
            request.numeric_only,
        );
        if !text.is_empty() {
            lines.push(OcrLine {
                text,
                confidence: None,
            });
        }
    }

    let mut text = if lines.is_empty() {
        normalize_ocr_text(
            &raw_result
                .Text()
                .map_err(|source| OcrError::WindowsApi {
                    context: "读取 Windows OCR 汇总文本",
                    source,
                })?
                .to_string(),
            request.numeric_only,
        )
    } else {
        lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    };
    if request.numeric_only {
        text = normalize_numeric_ocr_text(&text);
    }

    Ok(OcrResult {
        backend: OcrBackend::WindowsNative,
        text,
        lines,
    })
}

fn normalize_ocr_text(text: &str, numeric_only: bool) -> String {
    if numeric_only {
        normalize_numeric_ocr_text(text)
    } else {
        text.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn normalize_numeric_ocr_text(text: &str) -> String {
    text.chars().filter_map(normalize_numeric_char).collect()
}

fn normalize_numeric_char(ch: char) -> Option<char> {
    match ch {
        '0'..='9' | '+' | '-' | '/' | ':' | '.' | '%' | ',' | 'x' | 'X' => Some(ch),
        '０'..='９' => char::from_u32((ch as u32) - ('０' as u32) + ('0' as u32)),
        '＋' => Some('+'),
        '－' | '—' | '–' | '﹣' => Some('-'),
        '／' => Some('/'),
        '：' => Some(':'),
        '．' | '。' | '・' | '·' => Some('.'),
        '％' => Some('%'),
        '，' => Some(','),
        _ if ch.is_whitespace() => None,
        _ => None,
    }
}

fn prepare_image_for_windows_ocr(image: DynamicImage, max_dimension: u32) -> PreparedOcrImage {
    let (width, height) = image.dimensions();
    let grayscale = if max_dimension > 0 && (width > max_dimension || height > max_dimension) {
        image
            .resize(max_dimension, max_dimension, FilterType::Triangle)
            .to_luma8()
    } else {
        image.to_luma8()
    };
    let (width, height) = grayscale.dimensions();

    PreparedOcrImage {
        width,
        height,
        pixels: grayscale.into_raw(),
    }
}

struct PreparedOcrImage {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

#[cfg(windows)]
struct WinrtApartmentGuard {
    should_uninitialize: bool,
}

#[cfg(windows)]
impl Drop for WinrtApartmentGuard {
    fn drop(&mut self) {
        if self.should_uninitialize {
            unsafe {
                RoUninitialize();
            }
        }
    }
}

#[cfg(windows)]
fn initialize_winrt_apartment() -> Result<WinrtApartmentGuard, OcrError> {
    unsafe {
        match RoInitialize(RO_INIT_MULTITHREADED) {
            Ok(()) => Ok(WinrtApartmentGuard {
                should_uninitialize: true,
            }),
            Err(source) if source.code() == RPC_E_CHANGED_MODE => Ok(WinrtApartmentGuard {
                should_uninitialize: false,
            }),
            Err(source) => Err(OcrError::WindowsApi {
                context: "初始化 WinRT OCR apartment",
                source,
            }),
        }
    }
}

#[derive(Debug, Error)]
pub enum OcrError {
    #[error("OCR 输入不是有效 PNG：{source}")]
    InvalidPng {
        #[source]
        source: image::ImageError,
    },
    #[cfg(windows)]
    #[error("Windows OCR 在 `{context}` 阶段失败：{source}")]
    WindowsApi {
        context: &'static str,
        #[source]
        source: windows::core::Error,
    },
    #[error("OCR 后端 `{backend:?}` 当前不可用：{details}")]
    BackendUnavailable {
        backend: OcrBackend,
        details: String,
    },
}

#[cfg(test)]
mod tests {
    use super::OcrBackend;
    use super::OcrError;
    use super::OcrRequest;
    use super::normalize_numeric_ocr_text;
    use super::prepare_image_for_windows_ocr;
    use super::recognize_text_from_png;
    use image::DynamicImage;
    use image::ImageBuffer;
    use image::ImageFormat;
    use image::Luma;
    use image::Rgba;
    use std::io::Cursor;

    #[test]
    fn recognize_text_rejects_invalid_png() {
        let error = recognize_text_from_png(b"not-a-png", &OcrRequest::default()).unwrap_err();

        assert!(matches!(error, OcrError::InvalidPng { .. }));
    }

    #[test]
    fn normalize_numeric_ocr_text_maps_fullwidth_digits_and_symbols() {
        let normalized = normalize_numeric_ocr_text("１２３：４５／６７．８％＋９");

        assert_eq!(normalized, "123:45/67.8%+9");
    }

    #[test]
    fn prepare_image_for_windows_ocr_downscales_to_max_dimension() {
        let image = DynamicImage::ImageLuma8(ImageBuffer::from_pixel(600, 300, Luma([255])));

        let prepared = prepare_image_for_windows_ocr(image, 200);

        assert_eq!(prepared.width, 200);
        assert_eq!(prepared.height, 100);
        assert_eq!(prepared.pixels.len(), 20_000);
    }

    #[cfg(windows)]
    #[test]
    fn recognize_text_for_valid_png_uses_windows_backend_or_structured_failure() {
        let png_bytes = sample_png();

        match recognize_text_from_png(&png_bytes, &OcrRequest::default()) {
            Ok(result) => {
                assert_eq!(result.backend, OcrBackend::WindowsNative);
            }
            Err(OcrError::BackendUnavailable { backend, .. }) => {
                assert_eq!(backend, OcrBackend::WindowsNative);
            }
            Err(OcrError::WindowsApi { .. }) => {}
            Err(other) => panic!("unexpected error: {other}"),
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn recognize_text_for_valid_png_reports_stub_backend_unavailable() {
        let png_bytes = sample_png();

        let error = recognize_text_from_png(&png_bytes, &OcrRequest::default()).unwrap_err();
        match error {
            OcrError::BackendUnavailable { backend, details } => {
                assert_eq!(backend, OcrBackend::Stub);
                assert!(details.contains("不是 Windows"));
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    fn sample_png() -> Vec<u8> {
        let image = ImageBuffer::from_pixel(2, 2, Rgba([255, 255, 255, 255]));
        let mut encoded = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(image)
            .write_to(&mut encoded, ImageFormat::Png)
            .unwrap();
        encoded.into_inner()
    }
}
