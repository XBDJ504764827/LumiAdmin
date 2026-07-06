ALTER TABLE player_access_cache
  ADD COLUMN IF NOT EXISTS rating_source TEXT NOT NULL DEFAULT 'legacy';

ALTER TABLE player_access_cache
  ALTER COLUMN rating_source SET DEFAULT 'legacy';

ALTER TABLE player_access_cache
  ALTER COLUMN steamid64 SET NOT NULL;

ALTER TABLE player_access_cache
  ALTER COLUMN rating_source SET NOT NULL;

DO $$
DECLARE
  pk_name TEXT;
  pk_columns TEXT;
BEGIN
  SELECT c.conname, string_agg(a.attname, ',' ORDER BY cols.ordinality)
    INTO pk_name, pk_columns
  FROM pg_constraint c
  JOIN unnest(c.conkey) WITH ORDINALITY AS cols(attnum, ordinality) ON true
  JOIN pg_attribute a ON a.attrelid = c.conrelid AND a.attnum = cols.attnum
  WHERE c.conrelid = 'player_access_cache'::regclass
    AND c.contype = 'p'
  GROUP BY c.conname;

  IF pk_columns IS DISTINCT FROM 'steamid64,rating_source' THEN
    IF pk_name IS NOT NULL THEN
      EXECUTE format('ALTER TABLE player_access_cache DROP CONSTRAINT %I', pk_name);
    END IF;

    ALTER TABLE player_access_cache
      ADD PRIMARY KEY (steamid64, rating_source);
  END IF;
END $$;
