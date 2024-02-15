-- Your SQL goes here




CREATE TABLE "verification"(
	"id" UUID NOT NULL PRIMARY KEY,
	"user" UUID NOT NULL,
	CONSTRAINT fk_users FOREIGN KEY ("user") REFERENCES "users"("id")
);
