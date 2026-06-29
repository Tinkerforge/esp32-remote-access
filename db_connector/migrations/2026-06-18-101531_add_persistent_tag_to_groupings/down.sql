DROP INDEX "device_groupings_one_default_per_user";

ALTER TABLE "device_groupings"
    DROP COLUMN "is_default";
