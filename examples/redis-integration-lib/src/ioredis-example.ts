/**
 * ioredis Example
 *
 * This example demonstrates using @electric-sql/redis with ioredis client.
 * ioredis auto-connects on instantiation and uses lowercase method names.
 *
 * Run with: pnpm tsx src/ioredis-example.ts
 */

import Redis from 'ioredis'
import { ElectricRedis } from '@electric-sql/redis'

interface Racer {
  id: string
  name: string
  score: number
  team: string | null
  created_at: string
}

async function main() {
  console.log(`ElectricRedis with ioredis - Leaderboard Example`)
  console.log(`=================================================\n`)

  // ioredis auto-connects on instantiation
  const redis = new Redis(process.env.REDIS_URL || `redis://localhost:6379`)

  redis.on(`error`, (err) => console.error(`Redis error:`, err))
  redis.on(`connect`, () => console.log(`Connected to Redis via ioredis`))

  const sync = new ElectricRedis({
    electric: {
      url: process.env.ELECTRIC_URL || `http://localhost:3000/v1/shape`,
    },
    redis,
  })

  // Sync racers to a sorted set (leaderboard)
  const leaderboard = sync.syncShape<Racer>(`racer_scores`, {
    shape: { table: `racers` },
    key: `id`,
    structure: `sorted_set`,
    score: `score`,
  })

  leaderboard.onInsert((key, racer) => {
    console.log(`[NEW] ${racer.name} joined with score ${racer.score}`)
  })

  leaderboard.onUpdate((key, racer) => {
    console.log(`[UPDATE] ${racer.name} now has score ${racer.score}`)
  })

  await sync.start()
  console.log(`Sync started!\n`)

  await new Promise((resolve) => setTimeout(resolve, 2000))

  // Read data using ioredis directly (lowercase method names)
  // Sorted set stores ID as member, score from 'score' field
  // zrange with WITHSCORES returns flat array: [id1, score1, id2, score2, ...]
  const racerMembers = await redis.zrange(`racer_scores`, 0, -1, `WITHSCORES`)

  const racers: Array<{ id: string; score: number }> = []
  for (let i = 0; i < racerMembers.length; i += 2) {
    const id = racerMembers[i]
    const score = parseFloat(racerMembers[i + 1])
    racers.push({ id, score })
  }

  console.log(`\nLeaderboard (${racers.length} racers):`)
  // Sort descending by score for display
  const sorted = racers.sort((a, b) => b.score - a.score)
  sorted.forEach((r, i) => {
    const medal = i === 0 ? `🥇` : i === 1 ? `🥈` : i === 2 ? `🥉` : `  `
    console.log(`${medal} #${i + 1} ID: ${r.id.padEnd(20)} Score: ${r.score}`)
  })

  // Get top 3 IDs using ZREVRANGE (descending order)
  const top3 = await redis.zrevrange(`racer_scores`, 0, 2)
  console.log(`\nTop 3 IDs:`, top3.join(`, `))

  console.log(`\nPress Ctrl+C to stop...`)

  const cleanup = async () => {
    await sync.stop()
    redis.disconnect()
    process.exit(0)
  }

  process.on(`SIGINT`, cleanup)
  process.on(`SIGTERM`, cleanup)
}

main().catch((err) => {
  console.error(`Error:`, err)
  process.exit(1)
})
