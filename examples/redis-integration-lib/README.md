# @electric-sql/redis Example Project

This example demonstrates how to use the `@electric-sql/redis` library to sync Electric SQL Shapes to various Redis data structures in real-time.

## Features

- ✅ **Multiple Redis Clients** - Works with node-redis, ioredis, and iovalkey
- ✅ **Multiple Data Structures** - Hash, Sorted Set, List, Set, Stream
- ✅ **Type Safety** - Full TypeScript support with generics
- ✅ **Real-time Sync** - Automatic updates from Postgres changes
- ✅ **Valkey Compatible** - Tested with both Redis and Valkey servers

## Quick Start

```bash
# Start all services (Postgres, Electric, Redis, Valkey)
docker-compose up -d

# Install dependencies
pnpm install

# Run the main example
pnpm dev
```

## Examples

| File                        | Description                        |
| --------------------------- | ---------------------------------- |
| `src/index.ts`              | Basic Hash example with node-redis |
| `src/simple-example.ts`     | All 5 data structures demo         |
| `src/sorted-set-example.ts` | Leaderboard with racer scores      |
| `src/list-example.ts`       | Recent items with max length       |
| `src/set-example.ts`        | Membership tracking                |
| `src/stream-example.ts`     | Change log / audit trail           |
| `src/ioredis-example.ts`    | Using ioredis client               |
| `src/iovalkey-example.ts`   | Using iovalkey with Valkey server  |

### Run Individual Examples

```bash
# Basic hash example (default)
pnpm dev

# All data structures
pnpm tsx src/simple-example.ts

# Sorted Set / Leaderboard
pnpm tsx src/sorted-set-example.ts

# List with max length
pnpm tsx src/list-example.ts

# Set membership
pnpm tsx src/set-example.ts

# Stream changelog
pnpm tsx src/stream-example.ts

# ioredis client
pnpm tsx src/ioredis-example.ts

# iovalkey + Valkey server (port 6389)
REDIS_URL=redis://localhost:6389 pnpm tsx src/iovalkey-example.ts
```

## Data Structures

### Hash (Default)

Best for: Key-value lookups, full record storage

```typescript
const items = sync.syncShape<Item>('items', {
  shape: { table: 'items' },
  key: 'id',
})

const item = await items.get('item-1') // Single lookup
const all = await items.getAll() // All records
```

### Sorted Set

Best for: Leaderboards, priority queues, range queries by score

```typescript
const leaderboard = sync.syncShape<Racer>('scores', {
  shape: { table: 'racers' },
  structure: 'sorted_set',
  score: 'score', // REQUIRED: numeric field for ordering
})

const top10 = await leaderboard.getRange(0, 9) // By rank
const all = await leaderboard.getAll() // All, ordered by score
```

### List

Best for: Recent items, activity feeds, fixed-size collections

```typescript
const recent = sync.syncShape<Item>('recent', {
  shape: { table: 'items' },
  structure: 'list',
  maxLength: 100, // Keep only 100 most recent
})

const items = await recent.getAll()
const first5 = await recent.getRange(0, 4)
```

### Set

Best for: Membership checks, unique IDs, tags

```typescript
const activeIds = sync.syncShape('active', {
  shape: { table: 'users' },
  structure: 'set',
})

const ids = await activeIds.getAll() // string[]
const isMember = ids.includes('user-1')
```

### Stream

Best for: Audit logs, change history, event sourcing

```typescript
const changelog = sync.syncShape<Item>('changelog', {
  shape: { table: 'items' },
  structure: 'stream',
})

const entries = await changelog.getAll() // StreamEntry<Item>[]
// entries[0].id = "1234567890123-0" (Redis stream ID)
// entries[0].data = { id: "...", name: "...", value: 10 }
```

## Docker Services

The `docker-compose.yml` includes:

| Service  | Port  | Description                            |
| -------- | ----- | -------------------------------------- |
| postgres | 54321 | PostgreSQL 16 with logical replication |
| electric | 3000  | Electric SQL sync service              |
| redis    | 6379  | Redis 7 (for node-redis, ioredis)      |
| valkey   | 6389  | Valkey 8 (Redis fork, for iovalkey)    |

## Sample Data

The migration creates two tables:

**items** - General purpose items

```sql
id, name, value, created_at
```

**racers** - For leaderboard examples (like Redis docs)

```sql
id, name, score, team, created_at
```

Sample racers (from Redis documentation):

- Norem: 10 (Red Team)
- Castilla: 12 (Blue Team)
- Sam-Bodden: 8 (Red Team)
- Royce: 10 (Green Team)
- Ford: 6 (Blue Team)
- Prickett: 14 (Green Team)

## Test Database Changes

While an example is running, try making changes in Postgres:

```bash
# Connect to Postgres
docker exec -it redis-integration-lib-postgres-1 psql -U postgres -d electric

# Update a racer's score
UPDATE racers SET score = score + 10 WHERE name = 'Ford';

# Insert a new item
INSERT INTO items (id, name, value) VALUES ('new-1', 'NewItem', 50);

# Delete an item
DELETE FROM items WHERE id = 'item-1';
```

Watch the console output for real-time updates!

## Cleanup

```bash
# Stop all services
docker-compose down

# Stop and remove volumes
docker-compose down -v
```
