use std::collections::HashSet;
use std::fs;
use std::io::Cursor;
use std::path::Path;

use image::DynamicImage;
use image::GenericImageView;
use image::ImageFormat;
use image::imageops::FilterType;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PageStateCatalog {
    #[serde(default)]
    pub pages: Vec<PageStateDefinition>,
}

impl PageStateCatalog {
    pub fn validate(&self) -> Result<(), VisionConfigError> {
        let mut page_ids = HashSet::new();
        for page in &self.pages {
            if page.page_id.trim().is_empty() {
                return Err(VisionConfigError::InvalidConfig {
                    message: "page_id 不能为空".to_string(),
                });
            }
            if !page_ids.insert(page.page_id.clone()) {
                return Err(VisionConfigError::InvalidConfig {
                    message: format!("page_id 重复：{}", page.page_id),
                });
            }
            page.validate()?;
        }

        for page in &self.pages {
            for action in page
                .supported_actions
                .iter()
                .chain(page.recovery_actions.iter())
            {
                let Some(target_page_id) = action.target_page_id.as_deref() else {
                    continue;
                };
                if page_ids.contains(target_page_id) {
                    continue;
                }

                return Err(VisionConfigError::InvalidConfig {
                    message: format!(
                        "页面 `{}` 的动作 `{}` 指向不存在的目标页面 `{target_page_id}`",
                        page.page_id, action.action_id
                    ),
                });
            }
        }

        Ok(())
    }

    pub fn find_page(&self, page_id: &str) -> Option<&PageStateDefinition> {
        self.pages.iter().find(|page| page.page_id == page_id)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageStateDefinition {
    pub page_id: String,
    pub display_name: String,
    pub reference_resolution: ReferenceResolution,
    #[serde(default)]
    pub confirmation_markers: Vec<PageConfirmationMarker>,
    #[serde(default)]
    pub rois: Vec<RoiDefinition>,
    #[serde(default)]
    pub supported_actions: Vec<PageActionDefinition>,
    #[serde(default)]
    pub recovery_actions: Vec<PageActionDefinition>,
}

impl PageStateDefinition {
    pub fn validate(&self) -> Result<(), VisionConfigError> {
        self.reference_resolution.validate("reference_resolution")?;

        let mut marker_ids = HashSet::new();
        for marker in &self.confirmation_markers {
            if marker.marker_id.trim().is_empty() {
                return Err(VisionConfigError::InvalidConfig {
                    message: format!("页面 `{}` 存在空的 marker_id", self.page_id),
                });
            }
            if !marker_ids.insert(marker.marker_id.clone()) {
                return Err(VisionConfigError::InvalidConfig {
                    message: format!(
                        "页面 `{}` 的确认特征 marker_id 重复：{}",
                        self.page_id, marker.marker_id
                    ),
                });
            }
            marker
                .rect
                .validate_within(&self.reference_resolution, &marker.marker_id)?;
            validate_threshold(
                marker.pass_threshold,
                &format!(
                    "页面 `{}` 的 marker `{}` pass_threshold",
                    self.page_id, marker.marker_id
                ),
            )?;
        }

        let mut roi_ids = HashSet::new();
        for roi in &self.rois {
            if roi.roi_id.trim().is_empty() {
                return Err(VisionConfigError::InvalidConfig {
                    message: format!("页面 `{}` 存在空的 roi_id", self.page_id),
                });
            }
            if !roi_ids.insert(roi.roi_id.clone()) {
                return Err(VisionConfigError::InvalidConfig {
                    message: format!("页面 `{}` 的 roi_id 重复：{}", self.page_id, roi.roi_id),
                });
            }
            roi.rect
                .validate_within(&self.reference_resolution, &roi.roi_id)?;
            validate_threshold(
                roi.confidence_threshold,
                &format!(
                    "页面 `{}` 的 ROI `{}` confidence_threshold",
                    self.page_id, roi.roi_id
                ),
            )?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferenceResolution {
    pub width: u32,
    pub height: u32,
}

impl ReferenceResolution {
    fn validate(&self, field_name: &str) -> Result<(), VisionConfigError> {
        if self.width == 0 || self.height == 0 {
            return Err(VisionConfigError::InvalidConfig {
                message: format!(
                    "{field_name} 必须为正尺寸，当前为 {}x{}",
                    self.width, self.height
                ),
            });
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageConfirmationMarker {
    pub marker_id: String,
    pub rect: RoiRect,
    pub strategy: PageConfirmationStrategy,
    #[serde(default)]
    pub match_method: TemplateMatchMethod,
    pub template_path: Option<String>,
    pub pass_threshold: Option<f32>,
    pub expected_hint: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PageConfirmationStrategy {
    TemplateFingerprint,
    TextHint,
    IconTemplate,
    DominantColor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TemplateMatchMethod {
    #[default]
    NormalizedGrayscaleMae,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageActionDefinition {
    pub action_id: String,
    pub description: String,
    pub target_page_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoiDefinition {
    pub roi_id: String,
    pub display_name: String,
    pub rect: RoiRect,
    pub purpose: RoiPurpose,
    #[serde(default)]
    pub preprocess_steps: Vec<RoiPreprocessStep>,
    pub confidence_threshold: Option<f32>,
    #[serde(default)]
    pub low_confidence_policy: LowConfidencePolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoiPurpose {
    PageAnchor,
    NumericOcr,
    ShortTextOcr,
    IconTemplate,
    ColorFingerprint,
    Generic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RoiPreprocessStep {
    Grayscale,
    Upscale2x,
    Threshold { cutoff: u8 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LowConfidencePolicy {
    AutoAccept,
    #[default]
    QueueReview,
    Reject,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoiRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl RoiRect {
    fn validate_within(
        &self,
        resolution: &ReferenceResolution,
        region_id: &str,
    ) -> Result<(), VisionConfigError> {
        if self.width == 0 || self.height == 0 {
            return Err(VisionConfigError::InvalidConfig {
                message: format!("区域 `{region_id}` 的尺寸必须大于 0"),
            });
        }

        let right = u64::from(self.x) + u64::from(self.width);
        let bottom = u64::from(self.y) + u64::from(self.height);
        if right > u64::from(resolution.width) || bottom > u64::from(resolution.height) {
            return Err(VisionConfigError::InvalidConfig {
                message: format!(
                    "区域 `{region_id}` 超出参考分辨率边界：rect=({}, {}, {}, {})，reference={}x{}",
                    self.x, self.y, self.width, self.height, resolution.width, resolution.height
                ),
            });
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedRoiRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoiArtifactPayload {
    pub page_id: String,
    pub roi_id: String,
    pub display_name: String,
    pub purpose: RoiPurpose,
    pub source_image_width: u32,
    pub source_image_height: u32,
    pub resolved_rect: ResolvedRoiRect,
    pub preprocess_steps: Vec<RoiPreprocessStep>,
    pub confidence_threshold: Option<f32>,
    pub low_confidence_policy: LowConfidencePolicy,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RoiCropResult {
    pub page_id: String,
    pub roi: RoiDefinition,
    pub source_image_width: u32,
    pub source_image_height: u32,
    pub resolved_rect: ResolvedRoiRect,
    pub png_bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryPageSignatureEntry {
    pub roi_id: String,
    pub hash64: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryPageSignature {
    pub page_id: String,
    pub entries: Vec<InventoryPageSignatureEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryPageSignatureComparison {
    pub page_id: String,
    pub total_regions: usize,
    pub matched_regions: usize,
    pub max_hamming_distance: u32,
    pub is_same_page: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarkerMatchResult {
    pub marker_id: String,
    pub strategy: PageConfirmationStrategy,
    pub template_path: Option<String>,
    pub match_method: TemplateMatchMethod,
    pub score: Option<f32>,
    pub pass_threshold: Option<f32>,
    pub passed: bool,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageConfirmationResult {
    pub page_id: String,
    pub matched: bool,
    pub matched_markers: usize,
    pub total_markers: usize,
    pub marker_results: Vec<MarkerMatchResult>,
}

impl RoiCropResult {
    pub fn artifact_payload(&self) -> RoiArtifactPayload {
        RoiArtifactPayload {
            page_id: self.page_id.clone(),
            roi_id: self.roi.roi_id.clone(),
            display_name: self.roi.display_name.clone(),
            purpose: self.roi.purpose,
            source_image_width: self.source_image_width,
            source_image_height: self.source_image_height,
            resolved_rect: self.resolved_rect,
            preprocess_steps: self.roi.preprocess_steps.clone(),
            confidence_threshold: self.roi.confidence_threshold,
            low_confidence_policy: self.roi.low_confidence_policy,
        }
    }

    pub fn artifact_payload_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(&self.artifact_payload())
    }
}

#[derive(Debug, Error)]
pub enum InventoryPageSignatureError {
    #[error("failed to crop inventory signature ROIs: {source}")]
    Crop {
        #[from]
        source: RoiCropError,
    },
    #[error("page `{page_id}` does not define any generic signature ROI")]
    MissingSignatureRegions { page_id: String },
    #[error("failed to decode ROI PNG while computing inventory page signature: {source}")]
    DecodePng { source: image::ImageError },
}

pub fn load_page_state_catalog_from_json(
    json_bytes: &[u8],
) -> Result<PageStateCatalog, VisionConfigError> {
    let catalog = serde_json::from_slice::<PageStateCatalog>(json_bytes)
        .map_err(|source| VisionConfigError::DeserializeJson { source })?;
    catalog.validate()?;
    Ok(catalog)
}

pub fn load_page_state_catalog_from_path(
    path: &Path,
) -> Result<PageStateCatalog, VisionConfigError> {
    let bytes = fs::read(path).map_err(|source| VisionConfigError::ReadConfigFile {
        path: path.to_path_buf(),
        source,
    })?;
    load_page_state_catalog_from_json(&bytes)
}

pub fn crop_all_rois_from_png(
    page: &PageStateDefinition,
    png_bytes: &[u8],
) -> Result<Vec<RoiCropResult>, RoiCropError> {
    page.validate().map_err(RoiCropError::InvalidConfig)?;

    let image = decode_png(png_bytes)?;
    let (image_width, image_height) = image.dimensions();

    page.rois
        .iter()
        .map(|roi| crop_roi_from_image(page, roi, &image, image_width, image_height))
        .collect()
}

pub fn crop_single_roi_from_png(
    page: &PageStateDefinition,
    roi_id: &str,
    png_bytes: &[u8],
) -> Result<RoiCropResult, RoiCropError> {
    page.validate().map_err(RoiCropError::InvalidConfig)?;

    let roi = page
        .rois
        .iter()
        .find(|roi| roi.roi_id == roi_id)
        .ok_or_else(|| RoiCropError::RoiNotFound {
            page_id: page.page_id.clone(),
            roi_id: roi_id.to_string(),
        })?;

    let image = decode_png(png_bytes)?;
    let (image_width, image_height) = image.dimensions();
    crop_roi_from_image(page, roi, &image, image_width, image_height)
}

pub fn build_inventory_page_signature(
    page: &PageStateDefinition,
    png_bytes: &[u8],
) -> Result<InventoryPageSignature, InventoryPageSignatureError> {
    let crops = crop_all_rois_from_png(page, png_bytes)?;
    let mut entries = crops
        .into_iter()
        .filter(|crop| is_inventory_signature_roi(&crop.roi))
        .map(|crop| {
            Ok(InventoryPageSignatureEntry {
                roi_id: crop.roi.roi_id,
                hash64: compute_average_hash64(crop.png_bytes.as_slice())?,
            })
        })
        .collect::<Result<Vec<_>, InventoryPageSignatureError>>()?;

    if entries.is_empty() {
        return Err(InventoryPageSignatureError::MissingSignatureRegions {
            page_id: page.page_id.clone(),
        });
    }

    entries.sort_by(|left, right| left.roi_id.cmp(&right.roi_id));
    Ok(InventoryPageSignature {
        page_id: page.page_id.clone(),
        entries,
    })
}

pub fn compare_inventory_page_signatures(
    left: &InventoryPageSignature,
    right: &InventoryPageSignature,
    max_hamming_distance: u32,
) -> InventoryPageSignatureComparison {
    if left.page_id != right.page_id {
        return InventoryPageSignatureComparison {
            page_id: left.page_id.clone(),
            total_regions: 0,
            matched_regions: 0,
            max_hamming_distance: u32::MAX,
            is_same_page: false,
        };
    }

    let mut total_regions = 0_usize;
    let mut matched_regions = 0_usize;
    let mut worst_distance = 0_u32;

    for left_entry in &left.entries {
        let Some(right_entry) = right
            .entries
            .iter()
            .find(|entry| entry.roi_id == left_entry.roi_id)
        else {
            continue;
        };

        total_regions += 1;
        let distance = (left_entry.hash64 ^ right_entry.hash64).count_ones();
        worst_distance = worst_distance.max(distance);
        if distance <= max_hamming_distance {
            matched_regions += 1;
        }
    }

    InventoryPageSignatureComparison {
        page_id: left.page_id.clone(),
        total_regions,
        matched_regions,
        max_hamming_distance: worst_distance,
        is_same_page: total_regions > 0 && matched_regions == total_regions,
    }
}

pub fn evaluate_page_confirmation_from_png(
    page: &PageStateDefinition,
    png_bytes: &[u8],
    templates_root: &Path,
) -> Result<PageConfirmationResult, PageConfirmationError> {
    page.validate()
        .map_err(PageConfirmationError::InvalidConfig)?;

    let screenshot =
        decode_png_for_confirmation(png_bytes).map_err(PageConfirmationError::DecodeScreenshot)?;
    let (image_width, image_height) = screenshot.dimensions();
    let mut marker_results = Vec::new();

    for marker in &page.confirmation_markers {
        marker_results.push(evaluate_marker(
            page,
            marker,
            templates_root,
            &screenshot,
            image_width,
            image_height,
        )?);
    }

    let matched_markers = marker_results.iter().filter(|result| result.passed).count();
    let matched = marker_results.iter().all(|result| result.passed);

    Ok(PageConfirmationResult {
        page_id: page.page_id.clone(),
        matched,
        matched_markers,
        total_markers: marker_results.len(),
        marker_results,
    })
}

fn decode_png(png_bytes: &[u8]) -> Result<DynamicImage, RoiCropError> {
    image::load_from_memory_with_format(png_bytes, ImageFormat::Png)
        .map_err(|source| RoiCropError::DecodePng { source })
}

fn decode_png_for_confirmation(png_bytes: &[u8]) -> Result<DynamicImage, image::ImageError> {
    image::load_from_memory_with_format(png_bytes, ImageFormat::Png)
}

fn is_inventory_signature_roi(roi: &RoiDefinition) -> bool {
    roi.roi_id.starts_with("signature_") && matches!(roi.purpose, RoiPurpose::Generic)
}

fn compute_average_hash64(png_bytes: &[u8]) -> Result<u64, InventoryPageSignatureError> {
    let image = image::load_from_memory_with_format(png_bytes, ImageFormat::Png)
        .map_err(|source| InventoryPageSignatureError::DecodePng { source })?;
    let luma = image
        .resize_exact(8, 8, FilterType::Triangle)
        .to_luma8()
        .into_vec();
    let average = luma.iter().map(|value| u64::from(*value)).sum::<u64>() / 64;
    let mut hash = 0_u64;

    for (index, value) in luma.iter().enumerate() {
        if u64::from(*value) >= average {
            hash |= 1_u64 << index;
        }
    }

    Ok(hash)
}

fn crop_roi_from_image(
    page: &PageStateDefinition,
    roi: &RoiDefinition,
    image: &DynamicImage,
    image_width: u32,
    image_height: u32,
) -> Result<RoiCropResult, RoiCropError> {
    let resolved_rect = resolve_roi_rect(
        &roi.rect,
        &page.reference_resolution,
        image_width,
        image_height,
    );
    validate_resolved_rect(
        &page.page_id,
        &roi.roi_id,
        resolved_rect,
        image_width,
        image_height,
    )?;

    let cropped = image.crop_imm(
        resolved_rect.x,
        resolved_rect.y,
        resolved_rect.width,
        resolved_rect.height,
    );
    let mut encoded = Cursor::new(Vec::new());
    cropped
        .write_to(&mut encoded, ImageFormat::Png)
        .map_err(|source| RoiCropError::EncodePng { source })?;

    Ok(RoiCropResult {
        page_id: page.page_id.clone(),
        roi: roi.clone(),
        source_image_width: image_width,
        source_image_height: image_height,
        resolved_rect,
        png_bytes: encoded.into_inner(),
    })
}

fn evaluate_marker(
    page: &PageStateDefinition,
    marker: &PageConfirmationMarker,
    templates_root: &Path,
    screenshot: &DynamicImage,
    image_width: u32,
    image_height: u32,
) -> Result<MarkerMatchResult, PageConfirmationError> {
    let resolved_rect = resolve_roi_rect(
        &marker.rect,
        &page.reference_resolution,
        image_width,
        image_height,
    );
    validate_resolved_rect(
        &page.page_id,
        &marker.marker_id,
        resolved_rect,
        image_width,
        image_height,
    )
    .map_err(PageConfirmationError::InvalidResolvedRect)?;

    match marker.strategy {
        PageConfirmationStrategy::TemplateFingerprint | PageConfirmationStrategy::IconTemplate => {
            let template_path = marker.template_path.clone().ok_or_else(|| {
                PageConfirmationError::MissingTemplatePath {
                    page_id: page.page_id.clone(),
                    marker_id: marker.marker_id.clone(),
                }
            })?;
            let template_full_path = templates_root.join(&template_path);
            let template_png = fs::read(&template_full_path).map_err(|source| {
                PageConfirmationError::ReadTemplate {
                    path: template_full_path.clone(),
                    source,
                }
            })?;
            let template = decode_png_for_confirmation(&template_png).map_err(|source| {
                PageConfirmationError::DecodeTemplate {
                    path: template_full_path.clone(),
                    source,
                }
            })?;

            let cropped = screenshot.crop_imm(
                resolved_rect.x,
                resolved_rect.y,
                resolved_rect.width,
                resolved_rect.height,
            );
            let score = compare_template_similarity(&cropped, &template, marker.match_method);
            let pass_threshold = marker.pass_threshold.unwrap_or(0.98);

            Ok(MarkerMatchResult {
                marker_id: marker.marker_id.clone(),
                strategy: marker.strategy,
                template_path: Some(template_path),
                match_method: marker.match_method,
                score: Some(score),
                pass_threshold: Some(pass_threshold),
                passed: score >= pass_threshold,
                note: marker.note.clone(),
            })
        }
        PageConfirmationStrategy::TextHint | PageConfirmationStrategy::DominantColor => {
            Ok(MarkerMatchResult {
                marker_id: marker.marker_id.clone(),
                strategy: marker.strategy,
                template_path: marker.template_path.clone(),
                match_method: marker.match_method,
                score: None,
                pass_threshold: marker.pass_threshold,
                passed: false,
                note: Some(format!(
                    "marker strategy `{}` 当前仍是骨架，尚未接入实际判定后端",
                    marker.strategy.label()
                )),
            })
        }
    }
}

fn validate_resolved_rect(
    page_id: &str,
    roi_id: &str,
    rect: ResolvedRoiRect,
    image_width: u32,
    image_height: u32,
) -> Result<(), RoiCropError> {
    if rect.width == 0 || rect.height == 0 {
        return Err(RoiCropError::InvalidResolvedRect {
            page_id: page_id.to_string(),
            roi_id: roi_id.to_string(),
            image_width,
            image_height,
            rect,
        });
    }

    let right = u64::from(rect.x) + u64::from(rect.width);
    let bottom = u64::from(rect.y) + u64::from(rect.height);
    if rect.x >= image_width
        || rect.y >= image_height
        || right > u64::from(image_width)
        || bottom > u64::from(image_height)
    {
        return Err(RoiCropError::InvalidResolvedRect {
            page_id: page_id.to_string(),
            roi_id: roi_id.to_string(),
            image_width,
            image_height,
            rect,
        });
    }

    Ok(())
}

fn resolve_roi_rect(
    rect: &RoiRect,
    reference: &ReferenceResolution,
    image_width: u32,
    image_height: u32,
) -> ResolvedRoiRect {
    let x = scale_dimension(rect.x, image_width, reference.width);
    let y = scale_dimension(rect.y, image_height, reference.height);
    let width = scale_dimension(rect.width, image_width, reference.width).max(1);
    let height = scale_dimension(rect.height, image_height, reference.height).max(1);

    ResolvedRoiRect {
        x,
        y,
        width,
        height,
    }
}

fn scale_dimension(value: u32, actual: u32, reference: u32) -> u32 {
    (((u64::from(value) * u64::from(actual)) + u64::from(reference / 2)) / u64::from(reference))
        as u32
}

fn validate_threshold(threshold: Option<f32>, field_name: &str) -> Result<(), VisionConfigError> {
    let Some(threshold) = threshold else {
        return Ok(());
    };

    if !(0.0..=1.0).contains(&threshold) {
        return Err(VisionConfigError::InvalidConfig {
            message: format!("{field_name} 必须落在 0.0..=1.0，当前为 {threshold}"),
        });
    }

    Ok(())
}

fn compare_template_similarity(
    cropped: &DynamicImage,
    template: &DynamicImage,
    method: TemplateMatchMethod,
) -> f32 {
    match method {
        TemplateMatchMethod::NormalizedGrayscaleMae => {
            let cropped = cropped.to_luma8();
            let template = if template.dimensions() == cropped.dimensions() {
                template.to_luma8()
            } else {
                image::imageops::resize(
                    &template.to_luma8(),
                    cropped.width(),
                    cropped.height(),
                    FilterType::Triangle,
                )
            };

            let total_diff = cropped
                .pixels()
                .zip(template.pixels())
                .map(|(left, right)| u64::from(left[0].abs_diff(right[0])))
                .sum::<u64>();
            let pixel_count = u64::from(cropped.width()) * u64::from(cropped.height());
            if pixel_count == 0 {
                return 0.0;
            }

            let max_diff = 255.0 * pixel_count as f32;
            1.0 - (total_diff as f32 / max_diff)
        }
    }
}

#[derive(Debug, Error)]
pub enum VisionConfigError {
    #[error("读取页面配置文件失败：{path}：{source}")]
    ReadConfigFile {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("页面配置 JSON 解析失败：{source}")]
    DeserializeJson {
        #[source]
        source: serde_json::Error,
    },
    #[error("页面配置无效：{message}")]
    InvalidConfig { message: String },
}

#[derive(Debug, Error)]
pub enum RoiCropError {
    #[error("{0}")]
    InvalidConfig(#[from] VisionConfigError),
    #[error("PNG 解码失败：{source}")]
    DecodePng {
        #[source]
        source: image::ImageError,
    },
    #[error("ROI PNG 编码失败：{source}")]
    EncodePng {
        #[source]
        source: image::ImageError,
    },
    #[error("页面 `{page_id}` 中不存在 ROI `{roi_id}`")]
    RoiNotFound { page_id: String, roi_id: String },
    #[error(
        "页面 `{page_id}` 的 ROI `{roi_id}` 在当前截图上的实际区域无效：rect=({}, {}, {}, {})，image={}x{}",
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        image_width,
        image_height
    )]
    InvalidResolvedRect {
        page_id: String,
        roi_id: String,
        image_width: u32,
        image_height: u32,
        rect: ResolvedRoiRect,
    },
}

#[derive(Debug, Error)]
pub enum PageConfirmationError {
    #[error("{0}")]
    InvalidConfig(#[from] VisionConfigError),
    #[error("页面确认时截图 PNG 解码失败：{0}")]
    DecodeScreenshot(#[source] image::ImageError),
    #[error("页面 `{page_id}` 的 marker `{marker_id}` 缺少 template_path")]
    MissingTemplatePath { page_id: String, marker_id: String },
    #[error("读取 marker 模板失败：{path}：{source}")]
    ReadTemplate {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("解析 marker 模板 PNG 失败：{path}：{source}")]
    DecodeTemplate {
        path: std::path::PathBuf,
        #[source]
        source: image::ImageError,
    },
    #[error("{0}")]
    InvalidResolvedRect(#[from] RoiCropError),
}

impl PageConfirmationStrategy {
    fn label(self) -> &'static str {
        match self {
            Self::TemplateFingerprint => "template_fingerprint",
            Self::TextHint => "text_hint",
            Self::IconTemplate => "icon_template",
            Self::DominantColor => "dominant_color",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LowConfidencePolicy;
    use super::PageActionDefinition;
    use super::PageConfirmationError;
    use super::PageConfirmationMarker;
    use super::PageConfirmationResult;
    use super::PageConfirmationStrategy;
    use super::PageStateDefinition;
    use super::ReferenceResolution;
    use super::RoiDefinition;
    use super::RoiPreprocessStep;
    use super::RoiPurpose;
    use super::RoiRect;
    use super::TemplateMatchMethod;
    use super::build_inventory_page_signature;
    use super::compare_inventory_page_signatures;
    use super::crop_all_rois_from_png;
    use super::crop_single_roi_from_png;
    use super::evaluate_page_confirmation_from_png;
    use super::load_page_state_catalog_from_json;
    use super::load_page_state_catalog_from_path;
    use image::DynamicImage;
    use image::GenericImage;
    use image::ImageBuffer;
    use image::ImageFormat;
    use image::Rgba;
    use std::fs;
    use std::io::Cursor;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn page_catalog_rejects_missing_action_target() {
        let json = r#"{
            "pages": [
                {
                    "page_id": "inventory_main",
                    "display_name": "仓库主页",
                    "reference_resolution": { "width": 1920, "height": 1080 },
                    "supported_actions": [
                        {
                            "action_id": "open_detail",
                            "description": "打开详情",
                            "target_page_id": "inventory_detail"
                        }
                    ]
                }
            ]
        }"#;

        let error = load_page_state_catalog_from_json(json.as_bytes()).unwrap_err();
        assert!(error.to_string().contains("指向不存在的目标页面"));
    }

    #[test]
    fn crop_all_rois_preserves_declared_order_and_metadata() {
        let page = sample_page_definition(4, 4);
        let screenshot = sample_png(4, 4);

        let crops = crop_all_rois_from_png(&page, &screenshot).unwrap();

        assert_eq!(crops.len(), 2);
        assert_eq!(crops[0].roi.roi_id, "title_text");
        assert_eq!(crops[0].resolved_rect.width, 2);
        assert_eq!(crops[0].resolved_rect.height, 2);
        assert_eq!(
            crops[0].artifact_payload().low_confidence_policy,
            LowConfidencePolicy::QueueReview
        );
        assert_eq!(crops[1].roi.roi_id, "item_icon");
    }

    #[test]
    fn crop_single_roi_scales_reference_coordinates_to_actual_image() {
        let page = sample_page_definition(4, 4);
        let screenshot = sample_png(8, 8);

        let crop = crop_single_roi_from_png(&page, "item_icon", &screenshot).unwrap();

        assert_eq!(crop.resolved_rect.x, 4);
        assert_eq!(crop.resolved_rect.y, 4);
        assert_eq!(crop.resolved_rect.width, 4);
        assert_eq!(crop.resolved_rect.height, 4);

        let decoded =
            image::load_from_memory_with_format(&crop.png_bytes, ImageFormat::Png).unwrap();
        assert_eq!(decoded.width(), 4);
        assert_eq!(decoded.height(), 4);
    }

    #[test]
    fn page_confirmation_matches_when_template_fingerprint_is_identical() {
        let temp_dir = unique_test_dir("page-confirm-match");
        fs::create_dir_all(temp_dir.join("markers")).unwrap();
        fs::write(
            temp_dir.join("markers/title_marker.png"),
            sample_red_png(2, 1),
        )
        .unwrap();

        let page = sample_page_definition(4, 4);
        let screenshot = sample_png(4, 4);

        let result = evaluate_page_confirmation_from_png(&page, &screenshot, &temp_dir).unwrap();

        assert_eq!(
            result,
            PageConfirmationResult {
                page_id: "inventory_main".to_string(),
                matched: true,
                matched_markers: 1,
                total_markers: 1,
                marker_results: vec![result.marker_results[0].clone()],
            }
        );
        assert_eq!(result.marker_results[0].marker_id, "title_marker");
        assert!(result.marker_results[0].passed);
        assert!(result.marker_results[0].score.unwrap_or_default() > 0.99);

        fs::remove_dir_all(temp_dir).unwrap();
    }

    #[test]
    fn page_confirmation_reports_missing_template_path() {
        let page = PageStateDefinition {
            confirmation_markers: vec![PageConfirmationMarker {
                marker_id: "title_marker".to_string(),
                rect: RoiRect {
                    x: 0,
                    y: 0,
                    width: 2,
                    height: 1,
                },
                strategy: PageConfirmationStrategy::TemplateFingerprint,
                match_method: TemplateMatchMethod::NormalizedGrayscaleMae,
                template_path: None,
                pass_threshold: Some(0.98),
                expected_hint: None,
                note: None,
            }],
            ..sample_page_definition(4, 4)
        };

        let error = evaluate_page_confirmation_from_png(&page, &sample_png(4, 4), Path::new("."))
            .unwrap_err();

        match error {
            PageConfirmationError::MissingTemplatePath { page_id, marker_id } => {
                assert_eq!(page_id, "inventory_main");
                assert_eq!(marker_id, "title_marker");
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn bundled_inventory_materials_template_matches_golden_screenshot() {
        let workspace_root = workspace_root();
        let catalog = load_page_state_catalog_from_path(
            &workspace_root.join("assets/templates/pages/inventory_materials_cn.json"),
        )
        .unwrap();
        let page = catalog.find_page("inventory_materials_cn").unwrap();
        let screenshot =
            fs::read(workspace_root.join("assets/golden/vision/inventory_materials_cn.png"))
                .unwrap();

        let result = evaluate_page_confirmation_from_png(
            page,
            &screenshot,
            &workspace_root.join("assets/templates"),
        )
        .unwrap();

        assert!(result.matched, "{result:?}");
        assert_eq!(result.matched_markers, 2);
        assert_eq!(result.total_markers, 2);
    }

    #[test]
    fn bundled_operator_detail_template_matches_golden_screenshot() {
        let workspace_root = workspace_root();
        let catalog = load_page_state_catalog_from_path(
            &workspace_root.join("assets/templates/pages/operator_detail_status_cn.json"),
        )
        .unwrap();
        let page = catalog.find_page("operator_detail_status_cn").unwrap();
        let screenshot =
            fs::read(workspace_root.join("assets/golden/vision/operator_detail_status_cn.png"))
                .unwrap();

        let result = evaluate_page_confirmation_from_png(
            page,
            &screenshot,
            &workspace_root.join("assets/templates"),
        )
        .unwrap();

        assert!(result.matched, "{result:?}");
        assert_eq!(result.matched_markers, 2);
        assert_eq!(result.total_markers, 2);
    }

    #[test]
    fn bundled_inventory_scan_template_matches_golden_and_crops_all_signature_rois() {
        let workspace_root = workspace_root();
        let catalog = load_page_state_catalog_from_path(
            &workspace_root.join("assets/templates/pages/inventory_materials_scan_cn.json"),
        )
        .unwrap();
        let page = catalog.find_page("inventory_materials_scan_cn").unwrap();
        let screenshot =
            fs::read(workspace_root.join("assets/golden/vision/inventory_materials_cn.png"))
                .unwrap();

        let confirmation = evaluate_page_confirmation_from_png(
            page,
            &screenshot,
            &workspace_root.join("assets/templates"),
        )
        .unwrap();
        let crops = crop_all_rois_from_png(page, &screenshot).unwrap();

        assert!(confirmation.matched, "{confirmation:?}");
        assert_eq!(confirmation.matched_markers, 2);
        assert_eq!(confirmation.total_markers, 2);
        assert_eq!(crops.len(), 5);
        assert_eq!(crops[0].roi.roi_id, "signature_count_left");
        assert_eq!(crops[4].roi.roi_id, "signature_count_mid_numeric");
    }

    #[test]
    fn inventory_page_signature_matches_for_same_golden_screenshot() {
        let workspace_root = workspace_root();
        let catalog = load_page_state_catalog_from_path(
            &workspace_root.join("assets/templates/pages/inventory_materials_scan_cn.json"),
        )
        .unwrap();
        let page = catalog.find_page("inventory_materials_scan_cn").unwrap();
        let screenshot =
            fs::read(workspace_root.join("assets/golden/vision/inventory_materials_cn.png"))
                .unwrap();

        let left = build_inventory_page_signature(page, &screenshot).unwrap();
        let right = build_inventory_page_signature(page, &screenshot).unwrap();
        let comparison = compare_inventory_page_signatures(&left, &right, 4);

        assert_eq!(left.entries.len(), 4);
        assert_eq!(right.entries.len(), 4);
        assert_eq!(comparison.total_regions, 4);
        assert_eq!(comparison.matched_regions, 4);
        assert_eq!(comparison.max_hamming_distance, 0);
        assert!(comparison.is_same_page);
    }

    #[test]
    fn inventory_page_signature_detects_modified_signature_roi() {
        let workspace_root = workspace_root();
        let catalog = load_page_state_catalog_from_path(
            &workspace_root.join("assets/templates/pages/inventory_materials_scan_cn.json"),
        )
        .unwrap();
        let page = catalog.find_page("inventory_materials_scan_cn").unwrap();
        let screenshot =
            fs::read(workspace_root.join("assets/golden/vision/inventory_materials_cn.png"))
                .unwrap();
        let original = build_inventory_page_signature(page, &screenshot).unwrap();

        let mut mutated =
            image::load_from_memory_with_format(&screenshot, ImageFormat::Png).unwrap();
        for x in 700..900 {
            for y in 790..910 {
                mutated.put_pixel(x, y, Rgba([255, 255, 255, 255]));
            }
        }
        let mut encoded = Cursor::new(Vec::new());
        mutated.write_to(&mut encoded, ImageFormat::Png).unwrap();
        let modified = build_inventory_page_signature(page, encoded.get_ref()).unwrap();
        let comparison = compare_inventory_page_signatures(&original, &modified, 4);

        assert_eq!(comparison.total_regions, 4);
        assert!(comparison.matched_regions < comparison.total_regions);
        assert!(!comparison.is_same_page);
    }

    fn sample_page_definition(reference_width: u32, reference_height: u32) -> PageStateDefinition {
        PageStateDefinition {
            page_id: "inventory_main".to_string(),
            display_name: "仓库主页".to_string(),
            reference_resolution: ReferenceResolution {
                width: reference_width,
                height: reference_height,
            },
            confirmation_markers: vec![PageConfirmationMarker {
                marker_id: "title_marker".to_string(),
                rect: RoiRect {
                    x: 0,
                    y: 0,
                    width: 2,
                    height: 1,
                },
                strategy: PageConfirmationStrategy::TemplateFingerprint,
                match_method: TemplateMatchMethod::NormalizedGrayscaleMae,
                template_path: Some("markers/title_marker.png".to_string()),
                pass_threshold: Some(0.98),
                expected_hint: Some("仓库".to_string()),
                note: Some("页面标题".to_string()),
            }],
            rois: vec![
                RoiDefinition {
                    roi_id: "title_text".to_string(),
                    display_name: "标题文本".to_string(),
                    rect: RoiRect {
                        x: 0,
                        y: 0,
                        width: 2,
                        height: 2,
                    },
                    purpose: RoiPurpose::ShortTextOcr,
                    preprocess_steps: vec![RoiPreprocessStep::Grayscale],
                    confidence_threshold: Some(0.98),
                    low_confidence_policy: LowConfidencePolicy::QueueReview,
                },
                RoiDefinition {
                    roi_id: "item_icon".to_string(),
                    display_name: "物品图标".to_string(),
                    rect: RoiRect {
                        x: 2,
                        y: 2,
                        width: 2,
                        height: 2,
                    },
                    purpose: RoiPurpose::IconTemplate,
                    preprocess_steps: vec![RoiPreprocessStep::Upscale2x],
                    confidence_threshold: Some(0.9),
                    low_confidence_policy: LowConfidencePolicy::AutoAccept,
                },
            ],
            supported_actions: vec![PageActionDefinition {
                action_id: "open_detail".to_string(),
                description: "打开条目详情".to_string(),
                target_page_id: Some("inventory_detail".to_string()),
            }],
            recovery_actions: vec![PageActionDefinition {
                action_id: "back".to_string(),
                description: "返回上一页".to_string(),
                target_page_id: None,
            }],
        }
    }

    fn sample_png(width: u32, height: u32) -> Vec<u8> {
        let image = ImageBuffer::from_fn(width, height, |x, y| {
            if x < width / 2 && y < height / 2 {
                Rgba([255, 0, 0, 255])
            } else if x >= width / 2 && y < height / 2 {
                Rgba([0, 255, 0, 255])
            } else if x < width / 2 && y >= height / 2 {
                Rgba([0, 0, 255, 255])
            } else {
                Rgba([255, 255, 0, 255])
            }
        });

        let mut encoded = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(image)
            .write_to(&mut encoded, ImageFormat::Png)
            .unwrap();
        encoded.into_inner()
    }

    fn sample_red_png(width: u32, height: u32) -> Vec<u8> {
        let image = ImageBuffer::from_pixel(width, height, Rgba([255, 0, 0, 255]));
        let mut encoded = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(image)
            .write_to(&mut encoded, ImageFormat::Png)
            .unwrap();
        encoded.into_inner()
    }

    fn unique_test_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "arkagent-device-vision-{label}-{}-{nanos}",
            std::process::id()
        ))
    }

    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap()
    }
}
