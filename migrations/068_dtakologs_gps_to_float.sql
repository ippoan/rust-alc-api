-- Change GPS fields from INTEGER to DOUBLE PRECISION
-- Scraper raw data contains float values (e.g. GPSDirection: 319.8)
-- Previously gRPC protobuf auto-truncated to int32, REST deserialize rejects floats

ALTER TABLE alc_api.dtako_logs
    ALTER COLUMN gps_direction TYPE DOUBLE PRECISION,
    ALTER COLUMN gps_latitude TYPE DOUBLE PRECISION,
    ALTER COLUMN gps_longitude TYPE DOUBLE PRECISION;
