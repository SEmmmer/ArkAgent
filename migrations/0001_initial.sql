CREATE TABLE app_meta (
    meta_key TEXT PRIMARY KEY,
    meta_value TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE sync_source_state (
    source_id TEXT PRIMARY KEY,
    status TEXT NOT NULL,
    last_attempt_at TEXT,
    last_success_at TEXT,
    cursor_value TEXT,
    last_error TEXT
);

CREATE TABLE raw_source_cache (
    cache_key TEXT PRIMARY KEY,
    source_name TEXT NOT NULL,
    revision TEXT,
    content_type TEXT NOT NULL,
    payload BLOB NOT NULL,
    fetched_at TEXT NOT NULL,
    expires_at TEXT
);

CREATE TABLE external_operator_def (
    operator_id TEXT PRIMARY KEY,
    name_zh TEXT NOT NULL,
    rarity INTEGER NOT NULL,
    profession TEXT NOT NULL,
    branch TEXT,
    server TEXT NOT NULL DEFAULT 'CN',
    raw_json TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE external_operator_growth (
    growth_id TEXT PRIMARY KEY,
    operator_id TEXT NOT NULL,
    stage_label TEXT NOT NULL,
    material_slot TEXT NOT NULL,
    raw_json TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (operator_id) REFERENCES external_operator_def(operator_id)
);

CREATE TABLE external_operator_building_skill (
    skill_id TEXT PRIMARY KEY,
    operator_id TEXT NOT NULL,
    room_type TEXT NOT NULL,
    skill_name TEXT NOT NULL,
    raw_json TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (operator_id) REFERENCES external_operator_def(operator_id)
);

CREATE TABLE external_item_def (
    item_id TEXT PRIMARY KEY,
    name_zh TEXT NOT NULL,
    item_type TEXT NOT NULL,
    rarity INTEGER,
    raw_json TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE external_recipe (
    recipe_id TEXT PRIMARY KEY,
    output_item_id TEXT NOT NULL,
    room_type TEXT NOT NULL,
    raw_json TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (output_item_id) REFERENCES external_item_def(item_id)
);

CREATE TABLE external_stage_def (
    stage_id TEXT PRIMARY KEY,
    zone_id TEXT,
    code TEXT NOT NULL,
    is_open INTEGER NOT NULL DEFAULT 1,
    raw_json TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE external_drop_matrix (
    matrix_id TEXT PRIMARY KEY,
    stage_id TEXT NOT NULL,
    item_id TEXT NOT NULL,
    sample_count INTEGER NOT NULL,
    drop_count INTEGER NOT NULL,
    window_start_at TEXT,
    window_end_at TEXT,
    raw_json TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (stage_id) REFERENCES external_stage_def(stage_id),
    FOREIGN KEY (item_id) REFERENCES external_item_def(item_id)
);

CREATE TABLE external_event_notice (
    notice_id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    notice_type TEXT NOT NULL,
    published_at TEXT NOT NULL,
    start_at TEXT,
    end_at TEXT,
    source_url TEXT NOT NULL,
    confirmed INTEGER NOT NULL DEFAULT 0,
    raw_json TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE inventory_snapshot (
    snapshot_id TEXT PRIMARY KEY,
    source TEXT NOT NULL,
    confidence REAL,
    note TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE inventory_item_state (
    item_id TEXT PRIMARY KEY,
    name_zh TEXT NOT NULL,
    item_type TEXT NOT NULL,
    rarity INTEGER,
    quantity INTEGER NOT NULL,
    data_source TEXT NOT NULL,
    recognition_confidence REAL,
    last_confirmed_at TEXT,
    last_changed_by TEXT,
    is_protected INTEGER NOT NULL DEFAULT 0,
    is_convertible INTEGER NOT NULL DEFAULT 0,
    participates_in_floor INTEGER NOT NULL DEFAULT 1,
    snapshot_id TEXT,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (snapshot_id) REFERENCES inventory_snapshot(snapshot_id)
);

CREATE TABLE operator_snapshot (
    snapshot_id TEXT PRIMARY KEY,
    source TEXT NOT NULL,
    confidence REAL,
    note TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE operator_state (
    operator_id TEXT PRIMARY KEY,
    name_zh TEXT NOT NULL,
    owned INTEGER NOT NULL DEFAULT 0,
    rarity INTEGER NOT NULL,
    profession TEXT NOT NULL,
    branch TEXT,
    elite_stage INTEGER NOT NULL DEFAULT 0,
    level INTEGER NOT NULL DEFAULT 1,
    skill_level INTEGER NOT NULL DEFAULT 1,
    mastery_1 INTEGER NOT NULL DEFAULT 0,
    mastery_2 INTEGER NOT NULL DEFAULT 0,
    mastery_3 INTEGER NOT NULL DEFAULT 0,
    module_state TEXT,
    module_level INTEGER,
    starred INTEGER NOT NULL DEFAULT 0,
    emergency_target INTEGER NOT NULL DEFAULT 0,
    recognition_confidence REAL,
    last_scanned_at TEXT,
    snapshot_id TEXT,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (snapshot_id) REFERENCES operator_snapshot(snapshot_id)
);

CREATE TABLE scan_artifact (
    artifact_id TEXT PRIMARY KEY,
    scan_kind TEXT NOT NULL,
    page_id TEXT NOT NULL,
    file_path TEXT,
    payload_json TEXT,
    confidence REAL,
    created_at TEXT NOT NULL
);

CREATE TABLE recognition_review_queue (
    review_id TEXT PRIMARY KEY,
    artifact_id TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    entity_id TEXT,
    proposed_value_json TEXT NOT NULL,
    confidence REAL NOT NULL,
    status TEXT NOT NULL,
    review_note TEXT,
    created_at TEXT NOT NULL,
    reviewed_at TEXT,
    FOREIGN KEY (artifact_id) REFERENCES scan_artifact(artifact_id)
);

CREATE TABLE resource_policy (
    policy_id TEXT PRIMARY KEY,
    item_id TEXT NOT NULL,
    policy_class TEXT NOT NULL,
    auto_consume_allowed INTEGER NOT NULL DEFAULT 0,
    user_override INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (item_id) REFERENCES external_item_def(item_id)
);

CREATE TABLE floor_profile (
    profile_id TEXT PRIMARY KEY,
    profile_name TEXT NOT NULL,
    profile_kind TEXT NOT NULL,
    allow_crafting INTEGER NOT NULL DEFAULT 0,
    allow_drone INTEGER NOT NULL DEFAULT 0,
    allow_soft_protected INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL
);

CREATE TABLE floor_profile_member (
    member_id TEXT PRIMARY KEY,
    profile_id TEXT NOT NULL,
    target_kind TEXT NOT NULL,
    target_id TEXT NOT NULL,
    quantity INTEGER NOT NULL,
    metadata_json TEXT,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (profile_id) REFERENCES floor_profile(profile_id)
);

CREATE TABLE planner_run (
    run_id TEXT PRIMARY KEY,
    planner_kind TEXT NOT NULL,
    status TEXT NOT NULL,
    input_snapshot_json TEXT NOT NULL,
    result_summary TEXT,
    created_at TEXT NOT NULL,
    finished_at TEXT
);

CREATE TABLE planner_recommendation (
    recommendation_id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL,
    recommendation_type TEXT NOT NULL,
    target_id TEXT,
    priority INTEGER NOT NULL DEFAULT 0,
    explanation TEXT NOT NULL,
    payload_json TEXT,
    created_at TEXT NOT NULL,
    FOREIGN KEY (run_id) REFERENCES planner_run(run_id)
);

CREATE TABLE base_layout_config (
    layout_id TEXT PRIMARY KEY,
    layout_name TEXT NOT NULL,
    layout_type TEXT NOT NULL,
    room_config_json TEXT NOT NULL,
    production_goal TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE base_shift_plan (
    plan_id TEXT PRIMARY KEY,
    layout_id TEXT NOT NULL,
    plan_name TEXT NOT NULL,
    score REAL NOT NULL,
    rationale TEXT NOT NULL,
    plan_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (layout_id) REFERENCES base_layout_config(layout_id)
);

CREATE TABLE alert (
    alert_id TEXT PRIMARY KEY,
    alert_type TEXT NOT NULL,
    severity TEXT NOT NULL,
    title TEXT NOT NULL,
    message TEXT NOT NULL,
    status TEXT NOT NULL,
    trigger_at TEXT NOT NULL,
    resolved_at TEXT,
    payload_json TEXT
);

CREATE TABLE audit_log (
    audit_id TEXT PRIMARY KEY,
    entity_type TEXT NOT NULL,
    entity_id TEXT,
    action TEXT NOT NULL,
    summary TEXT NOT NULL,
    payload_json TEXT,
    source TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX idx_external_operator_growth_operator_id
    ON external_operator_growth(operator_id);
CREATE INDEX idx_external_operator_building_skill_operator_id
    ON external_operator_building_skill(operator_id);
CREATE INDEX idx_external_recipe_output_item_id
    ON external_recipe(output_item_id);
CREATE INDEX idx_external_drop_matrix_stage_item
    ON external_drop_matrix(stage_id, item_id);
CREATE INDEX idx_inventory_item_state_snapshot_id
    ON inventory_item_state(snapshot_id);
CREATE INDEX idx_operator_state_snapshot_id
    ON operator_state(snapshot_id);
CREATE INDEX idx_recognition_review_queue_status
    ON recognition_review_queue(status);
CREATE INDEX idx_floor_profile_member_profile_id
    ON floor_profile_member(profile_id);
CREATE INDEX idx_planner_recommendation_run_id
    ON planner_recommendation(run_id);
CREATE INDEX idx_base_shift_plan_layout_id
    ON base_shift_plan(layout_id);
CREATE INDEX idx_alert_status_trigger_at
    ON alert(status, trigger_at);
CREATE INDEX idx_audit_log_entity_created_at
    ON audit_log(entity_type, entity_id, created_at);
