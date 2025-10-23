-- Your SQL goes here
CREATE TABLE "device_grouping_members"(
    "id" UUID PRIMARY KEY,
    "grouping_id" UUID NOT NULL REFERENCES device_groupings(id) ON DELETE CASCADE,
    "charger_id" UUID NOT NULL REFERENCES chargers(id) ON DELETE CASCADE,
    "added_at" TIMESTAMP NOT NULL DEFAULT NOW(),
    UNIQUE(grouping_id, charger_id)
);

CREATE INDEX idx_device_grouping_members_grouping_id ON device_grouping_members(grouping_id);
CREATE INDEX idx_device_grouping_members_charger_id ON device_grouping_members(charger_id);
