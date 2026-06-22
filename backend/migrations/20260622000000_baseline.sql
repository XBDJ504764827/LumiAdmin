-- Baseline marker for the SQLx migration system.
--
-- Existing installations are still normalized by the legacy idempotent schema
-- migrator in src/db/*.rs before this migration runs. New schema changes should
-- be added as timestamped SQL files in this directory.
SELECT 1;
