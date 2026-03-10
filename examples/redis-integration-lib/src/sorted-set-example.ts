/**
 * Sorted Set Example - Leaderboard with Racer Scores
 *
 * This example demonstrates using Redis Sorted Sets with ElectricRedis,
 * following the patterns from Redis documentation for leaderboards.
 *
 * Sorted sets are perfect for:
 * - Leaderboards (ordered by score)
 * - Rate limiters
 * - Priority queues
 * - Any data that needs to be ordered by a numeric field
 *
 * Run with: pnpm tsx src/sorted-set-example.ts
 */

import { createClient } from 'redis'
import { ElectricRedis } from '@electric-sql/redis'

// Racer type matching the database schema
interface Racer {
  id: string
  name: string
  score: number
  team: string | null
  created_at: string
}

async function main() {
  console.log(`ElectricRedis - Sorted Set (Leaderboard) Example`)
  console.log(`================================================\n`)

  // 1. Create Redis client
  const redis = createClient({
    url: process.env.REDIS_URL || `redis://localhost:6379`,
  })

  redis.on(`error`, (err) => console.error(`Redis error:`, err))
  await redis.connect()

  // 2. Create ElectricRedis instance
  const sync = new ElectricRedis({
    electric: {
      url: process.env.ELECTRIC_URL || `http://localhost:3000/v1/shape`,
    },
    redis,
  })

  // 3. Define a sorted set shape for the leaderboard
  // The 'score' field is REQUIRED for sorted_set structure
  const leaderboard = sync.syncShape<Racer>(`racer_scores`, {
    shape: { table: `racers` },
    key: `id`,
    structure: `sorted_set`,
    score: `score`,
  })

  // 4. Set up event listeners for real-time updates
  leaderboard.onInsert((key, racer) => {
    console.log(`[NEW RACER] ${racer.name} joined with score ${racer.score}`)
  })

  leaderboard.onUpdate((key, racer) => {
    console.log(`[SCORE UPDATE] ${racer.name} now has score ${racer.score}`)
  })

  leaderboard.onDelete((key) => {
    console.log(`[RACER LEFT] ${key}`)
  })

  // 5. Start syncing
  await sync.start()
  console.log(`Sync started! Waiting for data...\n`)

  // 6. Query the leaderboard after a brief delay
  await new Promise((resolve) => setTimeout(resolve, 2000))

  // ==========================================
  // READ DATA USING REDIS CLIENT DIRECTLY
  // ==========================================

  // Get all racers with scores - ZRANGE with WITHSCORES
  // Member is the racer ID, score is from the 'score' field
  const racersWithScores = await redis.zRangeWithScores(`racer_scores`, 0, -1)
  console.log(`\nLeaderboard (${racersWithScores.length} racers):`)
  console.log(`----------------------------------------`)

  // Get top 3 racers using ZRANGE with REV option (highest first)
  const top3 = await redis.zRange(`racer_scores`, 0, 2, { REV: true })
  console.log(`\nTop 3 Racer IDs (via zRange REV):`)
  top3.forEach((id, i) => {
    console.log(`  ${i + 1}. ${id}`)
  })

  // Get racers with score <= 10 using ZRANGEBYSCORE
  const lowScorers = await redis.zRangeByScore(`racer_scores`, `-inf`, `10`)
  console.log(`\nRacers with score <= 10 (via zRangeByScore):`)
  lowScorers.forEach((id) => {
    console.log(`  - ${id}`)
  })

  // 7. Keep running until interrupted
  console.log(`\n----------------------------------------`)
  console.log(`Leaderboard is live! Try updating scores in Postgres:`)
  console.log(`  UPDATE racers SET score = score + 5 WHERE name = 'Ford';`)
  console.log(`\nNote: Sorted set stores IDs as members. To get full data,`)
  console.log(`use a hash alongside or query the database by ID.`)
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

main().catch((err) => {
  console.error(`Error:`, err)
  process.exit(1)
})
