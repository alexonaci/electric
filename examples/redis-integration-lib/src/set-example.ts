/**
 * Set Example - Membership Tracking
 *
 * This example demonstrates using Redis Sets with ElectricRedis.
 *
 * Sets are perfect for:
 * - Tracking unique IDs (e.g., active users, visited pages)
 * - Membership checks (is this ID in the set?)
 * - Tagging systems
 * - Any collection where you only need to know "is it there?"
 *
 * Run with: pnpm tsx src/set-example.ts
 */

import { createClient } from 'redis'
import { ElectricRedis } from '@electric-sql/redis'

async function main() {
  console.log(`ElectricRedis - Set (Membership) Example`)
  console.log(`========================================\n`)

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

  // 3. Define a set shape for tracking item IDs
  // Sets only store keys (membership), not the full data
  sync.syncShape(`item_ids`, {
    shape: { table: `items` },
    key: `id`,
    structure: `set`,
  })

  // 5. Start syncing
  await sync.start()
  console.log(`Sync started! Waiting for data...\n`)

  // 6. Query the set after a brief delay
  await new Promise((resolve) => setTimeout(resolve, 2000))

  // ==========================================
  // READ DATA USING REDIS CLIENT DIRECTLY
  // ==========================================

  // Get all members of the set using SMEMBERS
  const allIds = await redis.sMembers(`item_ids`)
  console.log(`\nItem IDs in Set (${allIds.length} members):`)
  console.log(`-----------------------------------------`)
  allIds.forEach((id) => {
    console.log(`  • ${id}`)
  })

  // Check membership using SISMEMBER (Redis native operation)
  const testId = allIds[0]
  if (testId) {
    const isMember = await redis.sIsMember(`item_ids`, testId)
    console.log(
      `\nMembership check (sIsMember): "${testId}" in set? ${isMember ? `Yes ✓` : `No ✗`}`
    )
  }

  // Get cardinality (number of members) using SCARD
  const count = await redis.sCard(`item_ids`)
  console.log(`Set cardinality (sCard): ${count}`)

  // Get a random member using SRANDMEMBER
  const randomMember = await redis.sRandMember(`item_ids`)
  console.log(`Random member (sRandMember): ${randomMember}`)

  // 7. Keep running until interrupted
  console.log(`\n-----------------------------------------`)
  console.log(`Set is live! Try inserting/deleting items in Postgres:`)
  console.log(
    `  INSERT INTO items (id, name, value) VALUES ('test-id', 'Test', 1);`
  )
  console.log(`  DELETE FROM items WHERE id = 'test-id';`)
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
