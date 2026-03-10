-- Create items table for basic examples
CREATE TABLE IF NOT EXISTS items (
  id TEXT PRIMARY KEY NOT NULL,
  name TEXT NOT NULL,
  value INTEGER NOT NULL DEFAULT 0,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create racers table for sorted set leaderboard example
CREATE TABLE IF NOT EXISTS racers (
  id TEXT PRIMARY KEY NOT NULL,
  name TEXT NOT NULL,
  score INTEGER NOT NULL DEFAULT 0,
  team TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Populate items table
INSERT INTO items (id, name, value, created_at) VALUES
  ('item-1', 'Apple', 10, NOW() - INTERVAL '5 minutes'),
  ('item-2', 'Banana', 25, NOW() - INTERVAL '4 minutes'),
  ('item-3', 'Cherry', 15, NOW() - INTERVAL '3 minutes'),
  ('item-4', 'Date', 30, NOW() - INTERVAL '2 minutes'),
  ('item-5', 'Elderberry', 5, NOW() - INTERVAL '1 minute');

-- Populate racers table (like Redis docs example)
INSERT INTO racers (id, name, score, team) VALUES
  ('racer-1', 'Norem', 10, 'Red Team'),
  ('racer-2', 'Castilla', 12, 'Blue Team'),
  ('racer-3', 'Sam-Bodden', 8, 'Red Team'),
  ('racer-4', 'Royce', 10, 'Green Team'),
  ('racer-5', 'Ford', 6, 'Blue Team'),
  ('racer-6', 'Prickett', 14, 'Green Team');
