# Templates Layout

`assets/templates` is the root for page-state configs and template marker assets.

Naming rules:

- Page configs: `assets/templates/pages/<page_id>.json`
- Marker templates: `assets/templates/markers/<page_id>/<marker_id>.png`
- Optional ROI reference crops: `assets/templates/rois/<page_id>/<roi_id>.png`

Config rules:

- `page_id` must match the config filename stem.
- `marker_id` and `roi_id` should use ASCII snake_case or kebab-case.
- `PageConfirmationMarker.template_path` is resolved relative to the templates root.
- `reference_resolution` is the coordinate base; runtime screenshots scale ROI and marker rects from this size.

Current CLI debug entry:

- `akbox-cli debug vision-inspect [--templates-root path] <page_config_path> <page_id> <input_png> [output_dir]`

This command validates one page config, runs page confirmation marker matching, crops all declared ROIs, and writes a `manifest.json` plus ROI PNGs into the output directory.

Bundled page presets:

- `inventory_materials_cn`: 仓库页里的“养成材料”子页，包含 `全部` / `养成材料` 顶部标签 marker 和一个可见数量数字样本 ROI。
- `inventory_materials_scan_cn`: 仓库“养成材料”子页的扫描签名模板，复用同一组顶部标签 marker，并额外声明 4 个可见物品签名 ROI + 1 个数量 OCR 样本 ROI，供后续翻页后的重复页 / 结束页比较使用。
- `operator_detail_status_cn`: 干员详情的“状态”页，包含 `信赖值` / `攻击范围` 左侧标签 marker 和信赖值数字样本 ROI。
