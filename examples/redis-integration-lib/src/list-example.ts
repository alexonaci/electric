/**
 * List Example - Recent Items with Maximum Length
 *
 * This example demonstrates using Redis Lists with ElectricRedis.
 *
 * Lists are perfect for:
 * - Recent activity feeds
 * - Message queues
 * - Keeping N most recent items
 * - Any ordered collection where you need index-based access
 *
 * Run with: pnpm tsx src/list-example.ts
 */

import { createClient } from 'redis'
import { ElectricRedis } from '@electric-sql/redis'

interface Item {
  id: string
  name: string
  value: number
  created_at: string
}

async function main() {
  console.log(`ElectricRedis - List (Recent Items) Example`)
  console.log(`============================================\n`)

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

  // 3. Define a list shape for recent items
  // maxLength keeps only the N most recent items
  sync.syncShape<Item>(`recent_items`, {
    shape: { table: `items` },
    key: `id`,
    structure: `list`,
    maxLength: 10, // Keep only 10 most recent
  })

  // 5. Start syncing
  await sync.start()
  console.log(`Sync started! Waiting for data...\n`)

  // 6. Query the list after a brief delay
  await new Promise((resolve) => setTimeout(resolve, 2000))

  // ==========================================
  // READ DATA USING REDIS CLIENT DIRECTLY
  // ==========================================

  // Get all items in the list using LRANGE
  const allRaw = await redis.lRange(`recent_items`, 0, -1)
  const allItems = allRaw.map((item) => JSON.parse(item))
  console.log(`\nRecent Items (${allItems.length} in list):`)
  console.log(`-----------------------------------------`)
  allItems.forEach((item, index) => {
    console.log(`  ${index + 1}. ${item.name} (value: ${item.value})`)
  })

  // Get first 3 items using LRANGE with range
  const first3Raw = await redis.lRange(`recent_items`, 0, 2)
  const first3 = first3Raw.map((item) => JSON.parse(item))
  console.log(`\nFirst 3 items (via lRange 0 2):`)
  first3.forEach((item, i) => {
    console.log(`  ${i + 1}. ${item.name}`)
  })

  // Get list length
  const length = await redis.lLen(`recent_items`)
  console.log(`\nList length: ${length}`)

  // Get item at specific index
  const firstItem = await redis.lIndex(`recent_items`, 0)
  if (firstItem) {
    console.log(`First item (via lIndex 0):`, JSON.parse(firstItem).name)
  }

  // 7. Keep running until interrupted
  console.log(`\n-----------------------------------------`)
  console.log(`List is live! Try inserting items in Postgres:`)
  console.log(
    `  INSERT INTO items (id, name, value) VALUES (gen_random_uuid()::text, 'NewItem', 50);`
  )
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
