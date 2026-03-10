/**
 * Advanced Example - Electric Config Options
 *
 * This example demonstrates advanced Electric configuration options:
 * - columnMapper: snakeCamelMapper() for automatic snake_case to camelCase
 * - parser: Custom type parsing (e.g., timestamptz to Date)
 * - where: Filter data with SQL WHERE clause
 * - columns: Sync only specific columns
 * - params: Positional parameters for WHERE clause
 * - backoffOptions: Retry configuration
 *
 * Run with: pnpm tsx src/advanced-example.ts
 */

import { createClient } from 'redis'
import { ElectricRedis } from '@electric-sql/redis'
import { snakeCamelMapper } from '@electric-sql/client'

// Type with camelCase properties (matching transformed data)
interface Racer {
  id: string
  name: string
  score: number
  team: string | null
  createdAt: string // transformed from created_at (kept as ISO string)
}

async function main() {
  console.log(`ElectricRedis - Advanced Electric Config Example`)
  console.log(`=================================================\n`)

  const redis = createClient({
    url: process.env.REDIS_URL || `redis://localhost:6379`,
  })

  redis.on(`error`, (err) => console.error(`Redis error:`, err))
  await redis.connect()

  const sync = new ElectricRedis({
    electric: {
      url: process.env.ELECTRIC_URL || `http://localhost:3000/v1/shape`,

      // Column mapper: Automatically converts snake_case to camelCase
      // This applies to column names in the synced data
      columnMapper: snakeCamelMapper(),

      // Parser: Custom type parsing for specific Postgres types
      // Note: The parser must return Value types (string, number, boolean, etc.)
      // For Date handling, keep as ISO string and parse in your app code
      parser: {
        // Example: parse bigint as string instead of BigInt
        int8: (value: string) => value, // Keep as string for JSON serialization
      },

      // Retry configuration for Electric connection
      backoffOptions: {
        initialDelay: 100, // Start with 100ms delay
        maxDelay: 30000, // Max 30 seconds between retries
        multiplier: 2, // Double the delay each retry
      },

      // Custom headers (e.g., for authentication)
      // headers: {
      //   'Authorization': `Bearer ${process.env.ELECTRIC_TOKEN}`,
      // },

      // Error handler for Electric connection issues
      onError: (error) => {
        console.error(`[Electric Error]`, error.message)
        // Return void to stop, or return { headers, params } to retry with new config
        // return { headers: { 'Authorization': `Bearer ${newToken}` } }
      },
    },
    redis,
  })

  // ==========================================
  // Example 1: Filtered leaderboard (WHERE clause)
  // Only sync racers with score >= 10
  // ==========================================
  const topRacers = sync.syncShape<Racer>(`top_racers`, {
    shape: {
      table: `racers`,
      where: `score >= 10`, // SQL WHERE clause
    },
    key: `id`,
    structure: `sorted_set`,
    score: `score`,
  })

  topRacers.onInsert((key, racer) => {
    console.log(`[TOP RACERS] ${racer.name} joined (score: ${racer.score})`)
  })

  // ==========================================
  // Example 2: Partial columns sync
  // Only sync id and name (less bandwidth)
  // ==========================================
  interface RacerSummary {
    id: string
    name: string
  }

  const racerNames = sync.syncShape<RacerSummary>(`racer_names`, {
    shape: {
      table: `racers`,
      columns: [`id`, `name`], // Only these columns
    },
    key: `id`,
    structure: `hash`,
  })

  racerNames.onInsert((key, racer) => {
    console.log(`[NAMES ONLY] New racer: ${racer.name}`)
  })

  // ==========================================
  // Example 3: Parameterized WHERE clause
  // Dynamic filtering with positional params
  // ==========================================
  const teamFilter = `Ferrari` // Could come from user input

  // Set structure stores only keys (IDs), not full data - no generic needed
  const ferrariRacers = sync.syncShape(`ferrari_racers`, {
    shape: {
      table: `racers`,
      where: `team = $1`, // Parameterized WHERE
      params: [teamFilter], // $1 = 'Ferrari'
    },
    key: `id`,
    structure: `set`, // Just track IDs of Ferrari racers
  })

  ferrariRacers.onInsert((key) => {
    console.log(`[FERRARI] Racer ${key} is on team Ferrari`)
  })

  // ==========================================
  // Start syncing
  // ==========================================
  await sync.start()
  console.log(`\nSync started with advanced Electric config!`)
  console.log(`- Column mapper: snake_case → camelCase`)
  console.log(`- Parser: timestamptz → Date`)
  console.log(`- Backoff: 100ms initial, 30s max, 2x multiplier`)

  // Wait for some data
  await new Promise((resolve) => setTimeout(resolve, 2000))

  // ==========================================
  // Read data using Redis client directly
  // ==========================================

  console.log(`\n=== TOP RACERS (score >= 10) ===`)
  const topRacerIds = await redis.zRangeWithScores(`top_racers`, 0, -1)
  console.log(`Racers with high scores:`)
  topRacerIds.forEach((entry, i) => {
    console.log(`  ${i + 1}. ID: ${entry.value}, Score: ${entry.score}`)
  })

  console.log(`\n=== RACER NAMES (partial columns) ===`)
  const namesData = await redis.hGetAll(`racer_names`)
  console.log(`Racer names only:`)
  Object.entries(namesData).forEach(([_key, value]) => {
    const racer = JSON.parse(value) as RacerSummary
    console.log(`  ${racer.name}`)
  })

  console.log(`\n=== FERRARI RACERS (parameterized filter) ===`)

  const ferrariIds = await redis.sMembers(`ferrari_racers`)

  console.log(`Ferrari team IDs: ${ferrariIds.join(`, `) || `none found`}`)

  // ==========================================
  // Cleanup
  // ==========================================
  console.log(`\nPress Ctrl+C to stop...`)

  const cleanup = async () => {
    console.log(`\nShutting down...`)
    await sync.stop()
    await redis.quit()
    process.exit(0)
  }

  process.on(`SIGINT`, cleanup)
  process.on(`SIGTERM`, cleanup)
}

main().catch(console.error)
