CREATE TABLE player_status_snapshot (
    snapshot_id TEXT PRIMARY KEY,
    source TEXT NOT NULL,
    uid TEXT NOT NULL,
    account_name TEXT,
    store_ts INTEGER,
    status_keys_json TEXT NOT NULL,
    raw_json TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE player_status_state (
    uid TEXT PRIMARY KEY,
    account_name TEXT,
    store_ts INTEGER,
    status_keys_json TEXT NOT NULL,
    snapshot_id TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    raw_json TEXT NOT NULL,
    FOREIGN KEY (snapshot_id) REFERENCES player_status_snapshot(snapshot_id)
);

CREATE TABLE base_building_snapshot (
    snapshot_id TEXT PRIMARY KEY,
    source TEXT NOT NULL,
    uid TEXT NOT NULL,
    has_control INTEGER NOT NULL DEFAULT 0,
    has_meeting INTEGER NOT NULL DEFAULT 0,
    has_training INTEGER NOT NULL DEFAULT 0,
    has_hire INTEGER NOT NULL DEFAULT 0,
    dormitory_count INTEGER NOT NULL DEFAULT 0,
    manufacture_count INTEGER NOT NULL DEFAULT 0,
    trading_count INTEGER NOT NULL DEFAULT 0,
    power_count INTEGER NOT NULL DEFAULT 0,
    tired_char_count INTEGER NOT NULL DEFAULT 0,
    building_keys_json TEXT NOT NULL,
    raw_json TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE base_building_state (
    uid TEXT PRIMARY KEY,
    has_control INTEGER NOT NULL DEFAULT 0,
    has_meeting INTEGER NOT NULL DEFAULT 0,
    has_training INTEGER NOT NULL DEFAULT 0,
    has_hire INTEGER NOT NULL DEFAULT 0,
    dormitory_count INTEGER NOT NULL DEFAULT 0,
    manufacture_count INTEGER NOT NULL DEFAULT 0,
    trading_count INTEGER NOT NULL DEFAULT 0,
    power_count INTEGER NOT NULL DEFAULT 0,
    tired_char_count INTEGER NOT NULL DEFAULT 0,
    building_keys_json TEXT NOT NULL,
    snapshot_id TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    raw_json TEXT NOT NULL,
    FOREIGN KEY (snapshot_id) REFERENCES base_building_snapshot(snapshot_id)
);

CREATE INDEX idx_player_status_state_snapshot_id
    ON player_status_state(snapshot_id);

CREATE INDEX idx_base_building_state_snapshot_id
    ON base_building_state(snapshot_id);
