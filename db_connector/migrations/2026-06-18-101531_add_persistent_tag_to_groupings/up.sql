ALTER TABLE "device_groupings"
    ADD COLUMN "default" BOOLEAN NOT NULL DEFAULT FALSE;

CREATE UNIQUE INDEX "device_groupings_one_default_per_user"
    ON "device_groupings" ("user_id")
    WHERE "default" = TRUE;
