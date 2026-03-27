-- Fix: tenko_call_numbers に INSERT/UPDATE/DELETE GRANT が不足。
-- migration 031 で SELECT のみ付与されていた。
GRANT INSERT, UPDATE, DELETE ON tenko_call_numbers TO alc_api_app;
