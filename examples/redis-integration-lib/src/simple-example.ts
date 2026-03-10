/**
 * Complete Example - All Redis Data Structures
 *
 * This example demonstrates all five Redis data structures
 * that can be used with ElectricRedis.
 *
 * Run with: pnpm tsx src/simple-example.ts
 */

import { createClient } from 'redis'
import { ElectricRedis } from '@electric-sql/redis'

interface Item {
  id: string
  name: string
  value: number
  created_at: string
}

interface Racer {
  id: string
  name: string
  score: number
  team: string | null
  created_at: string
}

async function main() {
  console.log(`ElectricRedis - Complete Example`)
  console.log(`================================\n`)

  const redis = createClient({
    url: process.env.REDIS_URL || `redis://localhost:6379`,
  })

  redis.on(`error`, (err) => console.error(`Redis error:`, err))
  await redis.connect()

  const sync = new ElectricRedis({
    electric: {
      url: process.env.ELECTRIC_URL || `http://localhost:3000/v1/shape`,
    },
    redis,
  })

  // ==========================================
  // 1. HASH - Standard key-value storage
  // ==========================================
  const items = sync.syncShape<Item>(`items`, {
    shape: { table: `items` },
    key: `id`,
    // structure: 'hash' is the default
  })

  items.onInsert((key, item) => console.log(`[HASH INSERT] ${item.name}`))
  items.onUpdate((key, item) => console.log(`[HASH UPDATE] ${item.name}`))
  items.onDelete((key) => console.log(`[HASH DELETE] ${key}`))

  // ==========================================
  // 2. SORTED SET - Ordered by score (leaderboard)
  // ==========================================
  const leaderboard = sync.syncShape<Racer>(`leaderboard`, {
    shape: { table: `racers` },
    key: `id`,
    structure: `sorted_set`,
    score: `score`, // REQUIRED for sorted_set
  })

  leaderboard.onInsert((key, racer) =>
    console.log(`[SORTED SET INSERT] ${racer.name}: ${racer.score}`)
  )
  leaderboard.onUpdate((key, racer) =>
    console.log(`[SORTED SET UPDATE] ${racer.name}: ${racer.score}`)
  )

  // ==========================================
  // 3. LIST - Ordered collection with max length
  // ==========================================
  const recentItems = sync.syncShape<Item>(`recent`, {
    shape: { table: `items` },
    key: `id`,
    structure: `list`,
    maxLength: 5,
  })

  recentItems.onInsert((key, item) => console.log(`[LIST INSERT] ${item.name}`))

  // ==========================================
  // 4. SET - Membership tracking (keys only)
  // ==========================================
  const itemIds = sync.syncShape(`item_ids`, {
    shape: { table: `items` },
    key: `id`,
    structure: `set`,
  })

  itemIds.onInsert((id) => console.log(`[SET ADD] ${id}`))
  itemIds.onDelete((id) => console.log(`[SET REMOVE] ${id}`))

  // ==========================================
  // 5. STREAM - Append-only change log
  // ==========================================
  sync.syncShape<Item>(`changelog`, {
    shape: { table: `items` },
    key: `id`,
    structure: `stream`,
  })

  // Start all syncs
  await sync.start()
  console.log(`\nAll syncs started! Waiting for data...\n`)

  await new Promise((resolve) => setTimeout(resolve, 2000))

  // ==========================================
  // READ DATA USING REDIS CLIENT DIRECTLY
  // ==========================================

  console.log(`\n=== HASH (using redis.hGetAll) ===`)
  const hashData = await redis.hGetAll(`items`)
  const hashItems = Object.entries(hashData).map(([key, value]) => ({
    key,
    ...JSON.parse(value),
  }))
  console.log(`Items: ${hashItems.length}`)
  if (hashItems.length > 0) {
    const firstItem = await redis.hGet(`items`, hashItems[0].key)
    console.log(
      `First item:`,
      firstItem ? JSON.parse(firstItem).name : `not found`
    )
  }

  console.log(`\n=== SORTED SET (using redis.zRange) ===`)
  // Sorted set stores ID as member, score from 'score' field
  const racersWithScores = await redis.zRangeWithScores(`leaderboard`, 0, -1)
  console.log(`Racers (by score ascending):`)
  racersWithScores.forEach((entry, i) => {
    // entry.value is the ID, entry.score is the score
    console.log(`  ${i + 1}. ID: ${entry.value}, Score: ${entry.score}`)
  })
  // Get top 3 IDs (highest scores) - reverse order
  const top3 = await redis.zRange(`leaderboard`, 0, 2, { REV: true })
  console.log(`Top 3 IDs:`, top3.join(`, `))

  console.log(`\n=== LIST (using redis.lRange) ===`)
  const recent = await redis.lRange(`recent`, 0, -1)
  const recentParsed = recent.map((item) => JSON.parse(item))
  console.log(
    `Recent items (max 5): ${recentParsed.map((i) => i.name).join(`, `)}`
  )

  console.log(`\n=== SET (using redis.sMembers) ===`)
  const ids = await redis.sMembers(`item_ids`)
  console.log(`Item IDs: ${ids.length} members`)

  console.log(`\n=== STREAM (using redis.xRange) ===`)
  const entries = await redis.xRange(`changelog`, `-`, `+`)
  console.log(`Stream entries: ${entries.length}`)
  if (entries.length > 0) {
    console.log(`Latest entry:`, entries[entries.length - 1])
  }

  console.log(`\n================================`)
  console.log(`Press Ctrl+C to stop...`)

  const cleanup = async () => {
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
